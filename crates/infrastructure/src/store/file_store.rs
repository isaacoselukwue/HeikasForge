use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::configuration::EffectiveConfiguration;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::model::attempt::{AttemptEvidence, AttemptKey, StoredArtifact};
use heikas_application::model::run_summary::RunHeader;
use heikas_application::ports::clock::{Clock, IdentifierFactory};
use heikas_application::ports::observability::Redactor;
use heikas_application::ports::store::{
    CandidateEvidenceStore, ChainVerification, EventStore, EvidenceStore, PlanStore,
    ProjectionStore, RunCatalogue,
};
use heikas_domain::clock::Timestamp;
use heikas_domain::event::{DurableEvent, EventPayload};
use heikas_domain::identity::{AttemptNumber, CandidateId, ContentDigest, RunId};
use heikas_domain::node::NodeResult;
use heikas_domain::plan::{PlanAuthor, PlanVersion};
use heikas_domain::review::AggregatedReview;
use heikas_domain::score::Ranking;
use heikas_domain::state::{RunManifest, RunProjection};
use heikas_domain::test_evidence::TestEvidence;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::atomic::{
    ensure_directory, read_json, remove_directory, rename_directory_into_place, storage,
    temporary_sibling, write_atomic, write_atomic_json,
};
use crate::layout::StoreLayout;
use crate::redaction::{redact_text_leaves, PatternRedactor};
use crate::store::event_log::EventLogFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunDescriptor {
    schema_version: u32,
    run_id: RunId,
    created_at: Timestamp,
    repository_path: String,
    configuration: EffectiveConfiguration,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArtifactIndex {
    entries: Vec<StoredArtifact>,
}

impl ArtifactIndex {
    fn find(&self, id: &ContentDigest) -> Option<&StoredArtifact> {
        self.entries.iter().find(|entry| &entry.id == id)
    }
}

pub struct FileRunStore {
    layout: StoreLayout,
    clock: Arc<dyn Clock>,
    identifiers: Arc<dyn IdentifierFactory>,
    logs: Mutex<HashMap<RunId, Arc<EventLogFile>>>,
    redactors: Mutex<HashMap<RunId, Arc<dyn Redactor>>>,
    default_redactor: Arc<dyn Redactor>,
}

impl FileRunStore {
    pub fn new(
        layout: StoreLayout,
        clock: Arc<dyn Clock>,
        identifiers: Arc<dyn IdentifierFactory>,
    ) -> Self {
        Self {
            layout,
            clock,
            identifiers,
            logs: Mutex::new(HashMap::new()),
            redactors: Mutex::new(HashMap::new()),
            default_redactor: Arc::new(PatternRedactor::without_environment()),
        }
    }

    async fn redactor_for(&self, run_id: RunId) -> Arc<dyn Redactor> {
        let mut guard = self.redactors.lock().await;
        if let Some(existing) = guard.get(&run_id) {
            return Arc::clone(existing);
        }
        let redactor: Arc<dyn Redactor> = match self.descriptor(run_id) {
            Ok(descriptor) => Arc::new(PatternRedactor::for_configuration(
                &descriptor.configuration.redaction,
            )),
            Err(_) => Arc::clone(&self.default_redactor),
        };
        guard.insert(run_id, Arc::clone(&redactor));
        redactor
    }

    async fn redact_serialisable<T: Serialize>(
        &self,
        run_id: RunId,
        value: &T,
    ) -> ApplicationResult<serde_json::Value> {
        let redactor = self.redactor_for(run_id).await;
        let encoded = serde_json::to_value(value)
            .map_err(|error| ApplicationError::Serialisation(error.to_string()))?;
        Ok(redact_text_leaves(redactor.as_ref(), &encoded))
    }

    pub fn layout(&self) -> &StoreLayout {
        &self.layout
    }

    async fn log_for(&self, run_id: RunId) -> Arc<EventLogFile> {
        let mut guard = self.logs.lock().await;
        guard
            .entry(run_id)
            .or_insert_with(|| {
                Arc::new(EventLogFile::new(
                    self.layout.events_file(run_id),
                    self.layout.quarantine_file(run_id),
                ))
            })
            .clone()
    }

    fn descriptor(&self, run_id: RunId) -> ApplicationResult<RunDescriptor> {
        read_json::<RunDescriptor>(&self.layout.run_descriptor(run_id))?
            .ok_or(ApplicationError::RunNotFound(run_id))
    }

    fn attempt_directory(&self, run_id: RunId, key: &AttemptKey) -> PathBuf {
        let mut path = self.layout.nodes_directory(run_id);
        for segment in key.directory_segments() {
            path = path.join(segment);
        }
        path
    }

    fn evidence_root(&self, run_id: RunId, candidate: Option<&CandidateId>) -> PathBuf {
        match candidate {
            Some(candidate) => self.layout.candidate_directory(run_id, candidate),
            None => self.layout.integration_directory(run_id),
        }
    }

    fn artifact_index(&self, run_id: RunId) -> ApplicationResult<ArtifactIndex> {
        Ok(read_json::<ArtifactIndex>(&self.layout.artifact_index(run_id))?.unwrap_or_default())
    }

    async fn redact_payload(
        &self,
        run_id: RunId,
        payload: EventPayload,
    ) -> ApplicationResult<EventPayload> {
        let redacted = self.redact_serialisable(run_id, &payload).await?;
        serde_json::from_value(redacted)
            .map_err(|error| ApplicationError::Serialisation(error.to_string()))
    }
}

#[async_trait]
impl EventStore for FileRunStore {
    async fn append(
        &self,
        run_id: RunId,
        payload: EventPayload,
    ) -> ApplicationResult<DurableEvent> {
        let log = self.log_for(run_id).await;
        let redacted = self.redact_payload(run_id, payload).await?;
        log.append(
            run_id,
            self.identifiers.new_event_id(),
            self.clock.now(),
            redacted,
        )
        .await
    }

    async fn read_after(
        &self,
        run_id: RunId,
        sequence: u64,
    ) -> ApplicationResult<Vec<DurableEvent>> {
        let log = self.log_for(run_id).await;
        log.read_after(sequence)
    }

    async fn read_range(
        &self,
        run_id: RunId,
        from_sequence: u64,
        limit: usize,
    ) -> ApplicationResult<Vec<DurableEvent>> {
        let log = self.log_for(run_id).await;
        log.read_range(from_sequence, limit)
    }

    async fn verify_chain(&self, run_id: RunId) -> ApplicationResult<ChainVerification> {
        let log = self.log_for(run_id).await;
        log.verify().await
    }
}

#[async_trait]
impl ProjectionStore for FileRunStore {
    async fn load(&self, run_id: RunId) -> ApplicationResult<Option<RunProjection>> {
        read_json(&self.layout.state_file(run_id))
    }

    async fn store(&self, projection: &RunProjection) -> ApplicationResult<()> {
        write_atomic_json(&self.layout.state_file(projection.run_id), projection)
    }

    async fn store_manifest(&self, manifest: &RunManifest) -> ApplicationResult<()> {
        write_atomic_json(&self.layout.manifest_file(manifest.run_id), manifest)
    }

    async fn load_manifest(&self, run_id: RunId) -> ApplicationResult<Option<RunManifest>> {
        read_json(&self.layout.manifest_file(run_id))
    }

    async fn store_metrics(
        &self,
        run_id: RunId,
        projection: &RunProjection,
    ) -> ApplicationResult<()> {
        let document = serde_json::json!({
            "schema_version": 1,
            "run_id": run_id,
            "last_event_sequence": projection.last_event_sequence,
            "metrics": projection.metrics,
            "candidates": projection.candidates.iter().map(|candidate| serde_json::json!({
                "candidate_id": candidate.id,
                "status": candidate.status.as_str(),
                "repairs_used": candidate.repairs_used,
                "changed_lines": candidate.changed_lines,
                "changed_files": candidate.changed_files,
                "gate_duration_ms": candidate.gate_duration.millis(),
            })).collect::<Vec<_>>(),
        });
        write_atomic_json(&self.layout.metrics_file(run_id), &document)
    }
}

#[async_trait]
impl RunCatalogue for FileRunStore {
    async fn initialise(
        &self,
        run_id: RunId,
        task_markdown: &str,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<()> {
        let directory = self.layout.run_directory(run_id);
        ensure_directory(&directory)?;
        ensure_directory(&self.layout.plan_directory(run_id))?;
        ensure_directory(&self.layout.nodes_directory(run_id))?;
        ensure_directory(&self.layout.candidates_directory(run_id))?;
        ensure_directory(&self.layout.integration_directory(run_id))?;
        ensure_directory(&self.layout.artifacts_directory(run_id))?;
        ensure_directory(&self.layout.logs_directory(run_id))?;
        ensure_directory(&self.layout.exports_directory(run_id))?;
        ensure_directory(&self.layout.locks_directory(run_id))?;
        write_atomic(&self.layout.task_file(run_id), task_markdown.as_bytes())?;
        let descriptor = RunDescriptor {
            schema_version: 1,
            run_id,
            created_at: self.clock.now(),
            repository_path: configuration.repository_path.display().to_string(),
            configuration: configuration.clone(),
        };
        write_atomic_json(&self.layout.run_descriptor(run_id), &descriptor)?;
        write_atomic_json(
            &self.layout.artifact_index(run_id),
            &ArtifactIndex::default(),
        )
    }

    async fn exists(&self, run_id: RunId) -> ApplicationResult<bool> {
        Ok(self.layout.run_descriptor(run_id).exists())
    }

    async fn headers(&self) -> ApplicationResult<Vec<RunHeader>> {
        let directory = self.layout.runs_directory();
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage(&directory, "read", error)),
        };
        let mut headers = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| storage(&directory, "read entry", error))?;
            let Ok(run_id) = entry.file_name().to_string_lossy().parse::<RunId>() else {
                continue;
            };
            let Some(projection) = read_json::<RunProjection>(&self.layout.state_file(run_id))?
            else {
                continue;
            };
            headers.push(RunHeader {
                run_id,
                created_at: projection.created_at,
                status: projection.status,
                repository_path: projection.repository_path.clone(),
                task_title: projection.task_title.clone(),
            });
        }
        headers.sort_by_key(|header| std::cmp::Reverse(header.created_at));
        Ok(headers)
    }

    async fn task_markdown(&self, run_id: RunId) -> ApplicationResult<String> {
        let path = self.layout.task_file(run_id);
        fs::read_to_string(&path).map_err(|error| storage(&path, "read", error))
    }

    async fn configuration(&self, run_id: RunId) -> ApplicationResult<EffectiveConfiguration> {
        Ok(self.descriptor(run_id)?.configuration)
    }

    async fn remove_worktrees(&self, run_id: RunId) -> ApplicationResult<Vec<String>> {
        let directory = self.layout.run_worktrees(run_id);
        let mut removed = Vec::new();
        if directory.exists() {
            removed.push(directory.display().to_string());
            remove_directory(&directory)?;
        }
        Ok(removed)
    }

    async fn resolve_run_reference(&self, reference: &str) -> ApplicationResult<RunId> {
        if let Ok(run_id) = reference.parse::<RunId>() {
            return Ok(run_id);
        }
        let headers = self.headers().await?;
        let normalised = reference.to_ascii_lowercase();
        let matches: Vec<_> = headers
            .iter()
            .filter(|header| {
                header.run_id.to_string().starts_with(&normalised)
                    || header.run_id.short().starts_with(&normalised)
            })
            .collect();
        match matches.as_slice() {
            [single] => Ok(single.run_id),
            [] => Err(ApplicationError::Storage(format!(
                "no run matches the reference `{reference}`"
            ))),
            _ => Err(ApplicationError::Storage(format!(
                "the reference `{reference}` matches {} runs",
                matches.len()
            ))),
        }
    }
}

#[async_trait]
impl EvidenceStore for FileRunStore {
    async fn commit_attempt(
        &self,
        run_id: RunId,
        result: &NodeResult,
        evidence: AttemptEvidence,
    ) -> ApplicationResult<()> {
        let key = AttemptKey::new(result.node_id, result.candidate_id.clone(), result.attempt);
        let destination = self.attempt_directory(run_id, &key);
        if destination.exists() {
            return Err(ApplicationError::Storage(format!(
                "attempt evidence at `{}` already exists",
                destination.display()
            )));
        }
        let redactor = self.redactor_for(run_id).await;
        let staging = temporary_sibling(&destination);
        ensure_directory(&staging)?;
        write_atomic_json(
            &staging.join("input.json"),
            &redact_text_leaves(redactor.as_ref(), &evidence.input),
        )?;
        if let Some(invocation) = &evidence.invocation {
            write_atomic_json(
                &staging.join("invocation.json"),
                &redact_text_leaves(redactor.as_ref(), invocation),
            )?;
        }
        write_atomic_json(
            &staging.join("result.json"),
            &self.redact_serialisable(run_id, result).await?,
        )?;
        write_atomic(
            &staging.join("stdout.log"),
            &redactor.redact_bytes(&evidence.stdout),
        )?;
        write_atomic(
            &staging.join("stderr.log"),
            &redactor.redact_bytes(&evidence.stderr),
        )?;
        rename_directory_into_place(&staging, &destination)
    }

    async fn read_attempt_result(
        &self,
        run_id: RunId,
        key: &AttemptKey,
    ) -> ApplicationResult<Option<NodeResult>> {
        read_json(&self.attempt_directory(run_id, key).join("result.json"))
    }

    async fn store_artifact(
        &self,
        run_id: RunId,
        label: &str,
        relative_path: &str,
        bytes: &[u8],
        truncated: bool,
    ) -> ApplicationResult<StoredArtifact> {
        let relative = heikas_domain::path_policy::RelativeWorkspacePath::parse(relative_path)?;
        let destination = self.layout.run_directory(run_id).join(relative.as_str());
        write_atomic(&destination, bytes)?;
        let artifact = StoredArtifact {
            id: ContentDigest::of_bytes(bytes),
            label: label.to_string(),
            relative_path: relative.as_str().to_string(),
            media_type: media_type_for(relative.as_str()),
            byte_length: bytes.len() as u64,
            truncated,
        };
        let mut index = self.artifact_index(run_id)?;
        index.entries.retain(|entry| entry.id != artifact.id);
        index.entries.push(artifact.clone());
        write_atomic_json(&self.layout.artifact_index(run_id), &index)?;
        Ok(artifact)
    }

    async fn read_artifact(
        &self,
        run_id: RunId,
        artifact_id: &ContentDigest,
    ) -> ApplicationResult<Vec<u8>> {
        let index = self.artifact_index(run_id)?;
        let entry = index
            .find(artifact_id)
            .ok_or_else(|| ApplicationError::ArtifactNotFound(artifact_id.to_string()))?;
        let path = self.layout.run_directory(run_id).join(&entry.relative_path);
        fs::read(&path).map_err(|error| storage(&path, "read", error))
    }

    async fn read_artifact_range(
        &self,
        run_id: RunId,
        artifact_id: &ContentDigest,
        offset: u64,
        length: u64,
    ) -> ApplicationResult<Vec<u8>> {
        let index = self.artifact_index(run_id)?;
        let entry = index
            .find(artifact_id)
            .ok_or_else(|| ApplicationError::ArtifactNotFound(artifact_id.to_string()))?;
        let path = self.layout.run_directory(run_id).join(&entry.relative_path);
        let mut file = fs::File::open(&path).map_err(|error| storage(&path, "open", error))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| storage(&path, "seek", error))?;
        let capped = length.min(4_194_304);
        let mut buffer = vec![0u8; capped as usize];
        let read = file
            .read(&mut buffer)
            .map_err(|error| storage(&path, "read", error))?;
        buffer.truncate(read);
        Ok(buffer)
    }
}

#[async_trait]
impl PlanStore for FileRunStore {
    async fn write_version(
        &self,
        run_id: RunId,
        version: u32,
        markdown: &str,
        author: PlanAuthor,
        revision_note: Option<String>,
        recorded_at: Timestamp,
    ) -> ApplicationResult<PlanVersion> {
        let path = self.layout.plan_version_file(run_id, version);
        write_atomic(&path, markdown.as_bytes())?;
        Ok(PlanVersion {
            version,
            hash: ContentDigest::of_str(markdown),
            created_at: recorded_at,
            author,
            revision_note,
            byte_length: markdown.len() as u64,
        })
    }

    async fn read_version(&self, run_id: RunId, version: u32) -> ApplicationResult<String> {
        let path = self.layout.plan_version_file(run_id, version);
        fs::read_to_string(&path).map_err(|error| storage(&path, "read", error))
    }

    async fn read_current(&self, run_id: RunId) -> ApplicationResult<Option<(u32, String)>> {
        let directory = self.layout.plan_directory(run_id);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage(&directory, "read", error)),
        };
        let mut best: Option<u32> = None;
        for entry in entries {
            let entry = entry.map_err(|error| storage(&directory, "read entry", error))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(number) = name
                .strip_prefix("plan-v")
                .and_then(|rest| rest.strip_suffix(".md"))
                .and_then(|digits| digits.parse::<u32>().ok())
            else {
                continue;
            };
            best = Some(best.map_or(number, |current| current.max(number)));
        }
        match best {
            Some(version) => {
                let markdown = self.read_version(run_id, version).await?;
                Ok(Some((version, markdown)))
            }
            None => Ok(None),
        }
    }
}

#[async_trait]
impl CandidateEvidenceStore for FileRunStore {
    async fn write_diff(
        &self,
        run_id: RunId,
        candidate: &CandidateId,
        patch: &[u8],
    ) -> ApplicationResult<ContentDigest> {
        let path = self
            .layout
            .candidate_directory(run_id, candidate)
            .join("diff.patch");
        write_atomic(&path, patch)?;
        Ok(ContentDigest::of_bytes(patch))
    }

    async fn read_diff(
        &self,
        run_id: RunId,
        candidate: &CandidateId,
    ) -> ApplicationResult<Vec<u8>> {
        let path = self
            .layout
            .candidate_directory(run_id, candidate)
            .join("diff.patch");
        match fs::read(&path) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(storage(&path, "read", error)),
        }
    }

    async fn write_test_evidence(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
        attempt: AttemptNumber,
        evidence: &TestEvidence,
    ) -> ApplicationResult<()> {
        let root = self.evidence_root(run_id, candidate).join("reports");
        let redacted = self.redact_serialisable(run_id, evidence).await?;
        write_atomic_json(
            &root.join(format!("tests-attempt-{attempt}.json")),
            &redacted,
        )?;
        write_atomic_json(&root.join("tests-latest.json"), &redacted)
    }

    async fn read_test_evidence(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
    ) -> ApplicationResult<Option<TestEvidence>> {
        read_json(
            &self
                .evidence_root(run_id, candidate)
                .join("reports")
                .join("tests-latest.json"),
        )
    }

    async fn write_review(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
        attempt: AttemptNumber,
        review: &AggregatedReview,
    ) -> ApplicationResult<()> {
        let root = self.evidence_root(run_id, candidate).join("reports");
        let redacted = self.redact_serialisable(run_id, review).await?;
        write_atomic_json(
            &root.join(format!("review-attempt-{attempt}.json")),
            &redacted,
        )?;
        write_atomic_json(&root.join("review-latest.json"), &redacted)
    }

    async fn read_review(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
    ) -> ApplicationResult<Option<AggregatedReview>> {
        read_json(
            &self
                .evidence_root(run_id, candidate)
                .join("reports")
                .join("review-latest.json"),
        )
    }

    async fn write_ranking(&self, run_id: RunId, ranking: &Ranking) -> ApplicationResult<()> {
        let directory = self.layout.integration_directory(run_id);
        write_atomic_json(&directory.join("ranking.json"), ranking)?;
        let selected = serde_json::json!({
            "winner": ranking.winner,
            "rationale": ranking.rationale,
        });
        write_atomic_json(&directory.join("selected.json"), &selected)
    }

    async fn write_integration_diff(
        &self,
        run_id: RunId,
        patch: &[u8],
    ) -> ApplicationResult<ContentDigest> {
        let path = self.layout.integration_directory(run_id).join("diff.patch");
        write_atomic(&path, patch)?;
        Ok(ContentDigest::of_bytes(patch))
    }

    async fn read_integration_diff(&self, run_id: RunId) -> ApplicationResult<Vec<u8>> {
        let path = self.layout.integration_directory(run_id).join("diff.patch");
        match fs::read(&path) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(storage(&path, "read", error)),
        }
    }
}

fn media_type_for(path: &str) -> String {
    let extension = path.rsplit('.').next().unwrap_or("");
    match extension {
        "json" => "application/json",
        "xml" => "application/xml",
        "sarif" => "application/sarif+json",
        "patch" | "diff" => "text/x-diff",
        "log" | "txt" | "info" => "text/plain",
        "md" => "text/markdown",
        "tar" => "application/x-tar",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}
