use serde::{Deserialize, Serialize};

use crate::candidate::{CandidateRecord, CandidateStatus};
use crate::clock::{DurationMs, Timestamp};
use crate::error::{DomainError, DomainResult};
use crate::event::{DurableEvent, EventPayload, GENESIS_HASH};
use crate::graph::NodeId;
use crate::identity::{
    AttemptNumber, BranchName, CandidateId, CommitHash, ContentDigest, RunId,
};
use crate::node::NodeStatus;
use crate::plan::{ApprovalDecision, PlanApproval, PlanHistory, PlanVersion};
use crate::run::{CommitPolicy, RunStatus};
use crate::score::Ranking;

pub const RUN_PROJECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeAttemptRecord {
    pub node_id: NodeId,
    pub candidate_id: Option<CandidateId>,
    pub attempt: AttemptNumber,
    pub status: NodeAttemptStatus,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub duration: DurationMs,
    pub failure_summary: Option<String>,
    pub failure_class: Option<crate::failure::FailureClass>,
    pub next: Option<NodeId>,
    pub sequence: u64,
}

impl NodeAttemptRecord {
    pub fn key(&self) -> (NodeId, Option<CandidateId>, AttemptNumber) {
        (self.node_id, self.candidate_id.clone(), self.attempt)
    }

    pub fn is_open(&self) -> bool {
        self.status == NodeAttemptStatus::Started
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeAttemptStatus {
    Started,
    Succeeded,
    Failed,
    Paused,
    Cancelled,
    Interrupted,
}

impl NodeAttemptStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeAttemptStatus::Started => "started",
            NodeAttemptStatus::Succeeded => "succeeded",
            NodeAttemptStatus::Failed => "failed",
            NodeAttemptStatus::Paused => "paused",
            NodeAttemptStatus::Cancelled => "cancelled",
            NodeAttemptStatus::Interrupted => "interrupted",
        }
    }

    pub fn from_node_status(status: NodeStatus) -> Self {
        match status {
            NodeStatus::Succeeded => NodeAttemptStatus::Succeeded,
            NodeStatus::Failed => NodeAttemptStatus::Failed,
            NodeStatus::Paused => NodeAttemptStatus::Paused,
            NodeStatus::Cancelled => NodeAttemptStatus::Cancelled,
            NodeStatus::Interrupted => NodeAttemptStatus::Interrupted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitRecord {
    pub branch: BranchName,
    pub commit_hash: CommitHash,
    pub author_name: String,
    pub committer_name: String,
    pub changed_files: u32,
    pub signed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct IntegrationState {
    pub attempted_candidates: Vec<CandidateId>,
    pub applied_candidate: Option<CandidateId>,
    pub final_tests_passed: Option<bool>,
    pub final_review_passed: Option<bool>,
    pub last_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct RunMetrics {
    pub node_executions: u64,
    pub node_failures: u64,
    pub automatic_retries: u64,
    pub repair_loops: u64,
    pub candidate_duration_ms: u64,
    pub test_duration_ms: u64,
    pub review_duration_ms: u64,
    pub agent_duration_ms: u64,
    pub changed_lines: u64,
    pub events_recorded: u64,
    pub artifacts_stored: u64,
    pub processes_supervised: u64,
    pub processes_timed_out: u64,
    pub reported_input_tokens: Option<u64>,
    pub reported_output_tokens: Option<u64>,
    pub reported_cost_minor_units: Option<u64>,
    pub reported_cost_currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunProjection {
    pub schema_version: u32,
    pub run_id: RunId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub last_event_sequence: u64,
    pub last_event_hash: String,
    pub status: RunStatus,
    pub repository_path: String,
    pub task_title: String,
    pub task_digest: ContentDigest,
    pub candidate_count: u8,
    pub commit_policy: CommitPolicy,
    pub agent_driver: String,
    pub demonstration_mode: bool,
    pub baseline_commit: Option<CommitHash>,
    pub default_branch: Option<String>,
    pub dirty_snapshot: bool,
    pub configuration_digest: Option<ContentDigest>,
    pub command_ids: Vec<String>,
    pub required_command_ids: Vec<String>,
    pub review_providers: Vec<String>,
    pub plan: PlanHistory,
    pub candidates: Vec<CandidateRecord>,
    pub attempts: Vec<NodeAttemptRecord>,
    pub ranking: Option<Ranking>,
    pub winner: Option<CandidateId>,
    pub integration: IntegrationState,
    pub commit: Option<CommitRecord>,
    pub commit_approved: bool,
    pub cancellation_requested: bool,
    pub recovery_reason: Option<String>,
    pub metrics: RunMetrics,
    pub last_event_summary: Option<String>,
    pub export_paths: Vec<String>,
}

impl RunProjection {
    pub fn genesis(run_id: RunId, created_at: Timestamp) -> Self {
        Self {
            schema_version: RUN_PROJECTION_SCHEMA_VERSION,
            run_id,
            created_at,
            updated_at: created_at,
            last_event_sequence: 0,
            last_event_hash: GENESIS_HASH.to_string(),
            status: RunStatus::Created,
            repository_path: String::new(),
            task_title: String::new(),
            task_digest: ContentDigest::of_str(""),
            candidate_count: 0,
            commit_policy: CommitPolicy::Manual,
            agent_driver: String::new(),
            demonstration_mode: false,
            baseline_commit: None,
            default_branch: None,
            dirty_snapshot: false,
            configuration_digest: None,
            command_ids: Vec::new(),
            required_command_ids: Vec::new(),
            review_providers: Vec::new(),
            plan: PlanHistory::default(),
            candidates: Vec::new(),
            attempts: Vec::new(),
            ranking: None,
            winner: None,
            integration: IntegrationState::default(),
            commit: None,
            commit_approved: false,
            cancellation_requested: false,
            recovery_reason: None,
            metrics: RunMetrics::default(),
            last_event_summary: None,
            export_paths: Vec::new(),
        }
    }

    pub fn candidate(&self, id: &CandidateId) -> Option<&CandidateRecord> {
        self.candidates.iter().find(|candidate| &candidate.id == id)
    }

    pub fn candidate_mut(&mut self, id: &CandidateId) -> Option<&mut CandidateRecord> {
        self.candidates.iter_mut().find(|candidate| &candidate.id == id)
    }

    pub fn all_candidates_terminal(&self) -> bool {
        !self.candidates.is_empty()
            && self
                .candidates
                .iter()
                .all(|candidate| candidate.status.is_terminal())
    }

    pub fn eligible_candidates(&self) -> Vec<&CandidateRecord> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.status == CandidateStatus::Eligible)
            .collect()
    }

    pub fn open_attempts(&self) -> Vec<&NodeAttemptRecord> {
        self.attempts.iter().filter(|attempt| attempt.is_open()).collect()
    }

    pub fn latest_attempt(&self, node: NodeId, candidate: Option<&CandidateId>) -> Option<&NodeAttemptRecord> {
        self.attempts
            .iter()
            .filter(|record| record.node_id == node && record.candidate_id.as_ref() == candidate)
            .max_by_key(|record| record.attempt.get())
    }

    pub fn next_attempt_number(&self, node: NodeId, candidate: Option<&CandidateId>) -> AttemptNumber {
        match self.latest_attempt(node, candidate) {
            Some(record) => record.attempt.next(),
            None => AttemptNumber::FIRST,
        }
    }

    pub fn completed_successfully(&self, node: NodeId, candidate: Option<&CandidateId>) -> bool {
        self.attempts.iter().any(|record| {
            record.node_id == node
                && record.candidate_id.as_ref() == candidate
                && record.status == NodeAttemptStatus::Succeeded
        })
    }

    pub fn apply(&mut self, event: &DurableEvent) -> DomainResult<()> {
        if event.run_id != self.run_id {
            return Err(DomainError::InvariantViolated(format!(
                "event for run {} cannot be applied to run {}",
                event.run_id, self.run_id
            )));
        }
        event.verify(self.last_event_sequence + 1, &self.last_event_hash)?;
        self.reduce(&event.payload, event.recorded_at, event.sequence)?;
        self.last_event_sequence = event.sequence;
        self.last_event_hash = event.chain_hash();
        self.updated_at = event.recorded_at;
        self.metrics.events_recorded = self.metrics.events_recorded.saturating_add(1);
        self.last_event_summary = Some(event.payload.human_summary());
        Ok(())
    }

    fn reduce(&mut self, payload: &EventPayload, at: Timestamp, sequence: u64) -> DomainResult<()> {
        match payload {
            EventPayload::RunCreated {
                repository_path,
                task_title,
                task_digest,
                candidate_count,
                commit_policy,
                agent_driver,
                demonstration_mode,
            } => {
                self.repository_path = repository_path.clone();
                self.task_title = task_title.clone();
                self.task_digest = task_digest.clone();
                self.candidate_count = *candidate_count;
                self.commit_policy = *commit_policy;
                self.agent_driver = agent_driver.clone();
                self.demonstration_mode = *demonstration_mode;
                self.created_at = at;
            }
            EventPayload::RunStatusChanged { from, to, reason } => {
                if self.status != *from {
                    return Err(DomainError::IllegalRunTransition {
                        from: self.status,
                        to: *to,
                    });
                }
                self.status = self.status.transition_to(*to)?;
                if *to == RunStatus::RecoveryRequired {
                    self.recovery_reason = reason.clone();
                } else {
                    self.recovery_reason = None;
                }
            }
            EventPayload::BaselineResolved {
                baseline_commit,
                default_branch,
                dirty_snapshot,
            } => {
                self.baseline_commit = Some(baseline_commit.clone());
                self.default_branch = Some(default_branch.clone());
                self.dirty_snapshot = *dirty_snapshot;
            }
            EventPayload::ConfigurationSnapshotted {
                digest,
                command_ids,
                required_command_ids,
                review_providers,
            } => {
                self.configuration_digest = Some(digest.clone());
                self.command_ids = command_ids.clone();
                self.required_command_ids = required_command_ids.clone();
                self.review_providers = review_providers.clone();
            }
            EventPayload::NodeStarted {
                node_id,
                candidate_id,
                attempt,
                ..
            } => {
                let key = (*node_id, candidate_id.clone(), *attempt);
                if self.attempts.iter().any(|record| record.key() == key) {
                    return Err(DomainError::InvariantViolated(format!(
                        "attempt {} for node {} was already recorded",
                        attempt, node_id
                    )));
                }
                self.attempts.push(NodeAttemptRecord {
                    node_id: *node_id,
                    candidate_id: candidate_id.clone(),
                    attempt: *attempt,
                    status: NodeAttemptStatus::Started,
                    started_at: at,
                    finished_at: None,
                    duration: DurationMs::ZERO,
                    failure_summary: None,
                    failure_class: None,
                    next: None,
                    sequence,
                });
                self.metrics.node_executions = self.metrics.node_executions.saturating_add(1);
            }
            EventPayload::NodeSucceeded {
                node_id,
                candidate_id,
                attempt,
                duration,
                next,
                ..
            } => {
                self.close_attempt(
                    *node_id,
                    candidate_id,
                    *attempt,
                    NodeAttemptStatus::Succeeded,
                    at,
                    *duration,
                    None,
                    None,
                    *next,
                )?;
                self.accumulate_duration(*node_id, *duration);
            }
            EventPayload::NodeFailed {
                node_id,
                candidate_id,
                attempt,
                duration,
                failure,
                next,
                ..
            } => {
                self.close_attempt(
                    *node_id,
                    candidate_id,
                    *attempt,
                    NodeAttemptStatus::Failed,
                    at,
                    *duration,
                    Some(failure.message.clone()),
                    Some(failure.class),
                    *next,
                )?;
                self.accumulate_duration(*node_id, *duration);
                self.metrics.node_failures = self.metrics.node_failures.saturating_add(1);
            }
            EventPayload::NodePaused {
                node_id,
                candidate_id,
                attempt,
                reason,
                ..
            } => {
                self.close_attempt(
                    *node_id,
                    candidate_id,
                    *attempt,
                    NodeAttemptStatus::Paused,
                    at,
                    DurationMs::ZERO,
                    Some(reason.clone()),
                    None,
                    None,
                )?;
            }
            EventPayload::NodeCancelled {
                node_id,
                candidate_id,
                attempt,
                ..
            } => {
                self.close_attempt(
                    *node_id,
                    candidate_id,
                    *attempt,
                    NodeAttemptStatus::Cancelled,
                    at,
                    DurationMs::ZERO,
                    None,
                    None,
                    None,
                )?;
            }
            EventPayload::NodeInterrupted {
                node_id,
                candidate_id,
                attempt,
                detected_at,
            } => {
                self.close_attempt(
                    *node_id,
                    candidate_id,
                    *attempt,
                    NodeAttemptStatus::Interrupted,
                    *detected_at,
                    DurationMs::ZERO,
                    Some("the dispatcher stopped before this attempt finished".to_string()),
                    None,
                    None,
                )?;
            }
            EventPayload::NodeRetryScheduled { .. } => {
                self.metrics.automatic_retries = self.metrics.automatic_retries.saturating_add(1);
            }
            EventPayload::PlanVersionWritten {
                version,
                plan_hash,
                author,
                revision_note,
                byte_length,
            } => {
                if self.plan.versions.iter().any(|entry| entry.version == *version) {
                    return Err(DomainError::InvariantViolated(format!(
                        "plan version {version} was already recorded"
                    )));
                }
                self.plan.versions.push(PlanVersion {
                    version: *version,
                    hash: plan_hash.clone(),
                    created_at: at,
                    author: *author,
                    revision_note: revision_note.clone(),
                    byte_length: *byte_length,
                });
                self.plan.versions.sort_by_key(|entry| entry.version);
            }
            EventPayload::PlanDecisionRecorded {
                approval_id,
                decision,
                plan_version,
                plan_hash,
                local_user,
                note,
            } => {
                if self.plan.version(*plan_version).is_none() {
                    return Err(DomainError::UnknownPlanVersion {
                        version: *plan_version,
                    });
                }
                self.plan.approval = Some(PlanApproval {
                    id: *approval_id,
                    decision: *decision,
                    plan_version: *plan_version,
                    plan_hash: plan_hash.clone(),
                    decided_at: at,
                    local_user: local_user.clone(),
                    note: note.clone(),
                });
            }
            EventPayload::PlanApprovalInvalidated { .. } => {
                if let Some(approval) = self.plan.approval.as_mut() {
                    approval.decision = ApprovalDecision::RevisionRequested;
                }
            }
            EventPayload::CandidateRegistered {
                candidate_id,
                ordinal,
                strategy,
                branch,
                worktree_relative_path,
                repair_budget,
            } => {
                if self.candidate(candidate_id).is_some() {
                    return Err(DomainError::InvariantViolated(format!(
                        "candidate {candidate_id} was already registered"
                    )));
                }
                let baseline = self
                    .baseline_commit
                    .clone()
                    .ok_or(DomainError::MissingField {
                        field: "baseline_commit",
                    })?;
                self.candidates.push(CandidateRecord::new(
                    candidate_id.clone(),
                    *ordinal,
                    *strategy,
                    baseline,
                    branch.clone(),
                    worktree_relative_path.clone(),
                    *repair_budget,
                ));
                self.candidates.sort_by_key(|candidate| candidate.ordinal);
            }
            EventPayload::CandidateStatusChanged {
                candidate_id,
                from,
                to,
                ..
            } => {
                let started = at;
                let candidate = self
                    .candidate_mut(candidate_id)
                    .ok_or_else(|| DomainError::UnknownCandidate {
                        candidate: candidate_id.to_string(),
                    })?;
                if candidate.status != *from {
                    return Err(DomainError::IllegalCandidateTransition {
                        from: candidate.status,
                        to: *to,
                    });
                }
                candidate.status = candidate.status.transition_to(*to)?;
                if candidate.started_at.is_none() && *to != CandidateStatus::Pending {
                    candidate.started_at = Some(started);
                }
                if candidate.status.is_terminal() {
                    candidate.finished_at = Some(started);
                }
            }
            EventPayload::CandidateDiffRecorded {
                candidate_id,
                diff_digest,
                changed_files,
                changed_lines,
            } => {
                let candidate = self
                    .candidate_mut(candidate_id)
                    .ok_or_else(|| DomainError::UnknownCandidate {
                        candidate: candidate_id.to_string(),
                    })?;
                candidate.diff_digest = Some(diff_digest.clone());
                candidate.changed_files = *changed_files;
                candidate.changed_lines = *changed_lines;
                self.metrics.changed_lines = self
                    .candidates
                    .iter()
                    .map(|candidate| candidate.changed_lines)
                    .sum();
            }
            EventPayload::CandidateRepairStarted {
                candidate_id,
                repairs_used,
                failure_fingerprint,
                ..
            } => {
                let candidate = self
                    .candidate_mut(candidate_id)
                    .ok_or_else(|| DomainError::UnknownCandidate {
                        candidate: candidate_id.to_string(),
                    })?;
                candidate.repairs_used = *repairs_used;
                if let Some(fingerprint) = failure_fingerprint {
                    candidate.observe_failure_fingerprint(fingerprint.clone());
                }
                self.metrics.repair_loops = self.metrics.repair_loops.saturating_add(1);
            }
            EventPayload::TestEvidenceRecorded {
                candidate_id,
                node_id,
                passed,
                duration,
                ..
            } => {
                self.metrics.test_duration_ms =
                    self.metrics.test_duration_ms.saturating_add(duration.millis());
                if let Some(id) = candidate_id {
                    if let Some(candidate) = self.candidate_mut(id) {
                        candidate.gate_duration = candidate.gate_duration.saturating_add(*duration);
                    }
                }
                if *node_id == NodeId::FinalTest {
                    self.integration.final_tests_passed = Some(*passed);
                }
            }
            EventPayload::ReviewEvidenceRecorded {
                candidate_id,
                node_id,
                passed,
                duration,
                ..
            } => {
                self.metrics.review_duration_ms =
                    self.metrics.review_duration_ms.saturating_add(duration.millis());
                if let Some(id) = candidate_id {
                    if let Some(candidate) = self.candidate_mut(id) {
                        candidate.gate_duration = candidate.gate_duration.saturating_add(*duration);
                    }
                }
                if *node_id == NodeId::FinalReview {
                    self.integration.final_review_passed = Some(*passed);
                }
            }
            EventPayload::CandidateScored { candidate_id, score } => {
                let candidate = self
                    .candidate_mut(candidate_id)
                    .ok_or_else(|| DomainError::UnknownCandidate {
                        candidate: candidate_id.to_string(),
                    })?;
                candidate.score = Some(score.clone());
            }
            EventPayload::CandidateExcluded {
                candidate_id,
                reasons,
            } => {
                let candidate = self
                    .candidate_mut(candidate_id)
                    .ok_or_else(|| DomainError::UnknownCandidate {
                        candidate: candidate_id.to_string(),
                    })?;
                candidate.exclusion_reasons = reasons.clone();
                candidate.promotable = false;
            }
            EventPayload::RankingComputed { ranking } => {
                self.ranking = Some(ranking.clone());
            }
            EventPayload::WinnerSelected { candidate_id, .. } => {
                if self.candidate(candidate_id).is_none() {
                    return Err(DomainError::UnknownCandidate {
                        candidate: candidate_id.to_string(),
                    });
                }
                self.winner = Some(candidate_id.clone());
            }
            EventPayload::IntegrationAttempted {
                candidate_id,
                applied,
                detail,
            } => {
                if !self.integration.attempted_candidates.contains(candidate_id) {
                    self.integration.attempted_candidates.push(candidate_id.clone());
                }
                self.integration.applied_candidate = if *applied {
                    Some(candidate_id.clone())
                } else {
                    None
                };
                self.integration.last_detail = detail.clone();
                self.integration.final_tests_passed = None;
                self.integration.final_review_passed = None;
                if let Some(candidate) = self.candidate_mut(candidate_id) {
                    candidate.integration_attempted = true;
                    if !*applied {
                        candidate.promotable = false;
                    }
                }
            }
            EventPayload::CandidatePromotionRequested {
                previous_candidate_id,
                next_candidate_id,
                ..
            } => {
                if let Some(candidate) = self.candidate_mut(previous_candidate_id) {
                    candidate.promotable = false;
                }
                self.winner = next_candidate_id.clone();
            }
            EventPayload::CommitApprovalRecorded { .. } => {
                self.commit_approved = true;
            }
            EventPayload::CommitCreated {
                branch,
                commit_hash,
                author_name,
                committer_name,
                changed_files,
                signed,
            } => {
                self.commit = Some(CommitRecord {
                    branch: branch.clone(),
                    commit_hash: commit_hash.clone(),
                    author_name: author_name.clone(),
                    committer_name: committer_name.clone(),
                    changed_files: *changed_files,
                    signed: *signed,
                });
            }
            EventPayload::CancellationRequested { .. } => {
                self.cancellation_requested = true;
            }
            EventPayload::RecoveryStarted { .. } => {}
            EventPayload::RecoveryCompleted { .. } => {}
            EventPayload::ProcessSupervisionRecorded { timed_out, .. } => {
                self.metrics.processes_supervised =
                    self.metrics.processes_supervised.saturating_add(1);
                if *timed_out {
                    self.metrics.processes_timed_out =
                        self.metrics.processes_timed_out.saturating_add(1);
                }
            }
            EventPayload::ArtifactStored { .. } => {
                self.metrics.artifacts_stored = self.metrics.artifacts_stored.saturating_add(1);
            }
            EventPayload::RunExported {
                archive_relative_path,
                ..
            } => {
                if !self.export_paths.contains(archive_relative_path) {
                    self.export_paths.push(archive_relative_path.clone());
                }
            }
            EventPayload::DiagnosticRecorded { .. } => {}
        }
        Ok(())
    }

    fn accumulate_duration(&mut self, node: NodeId, duration: DurationMs) {
        match node.class() {
            crate::graph::NodeClass::Agent => {
                self.metrics.agent_duration_ms =
                    self.metrics.agent_duration_ms.saturating_add(duration.millis());
            }
            crate::graph::NodeClass::Command => {}
            crate::graph::NodeClass::Review => {}
            _ => {}
        }
        if node.scope() == crate::graph::NodeScope::Candidate {
            self.metrics.candidate_duration_ms = self
                .metrics
                .candidate_duration_ms
                .saturating_add(duration.millis());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn close_attempt(
        &mut self,
        node: NodeId,
        candidate: &Option<CandidateId>,
        attempt: AttemptNumber,
        status: NodeAttemptStatus,
        at: Timestamp,
        duration: DurationMs,
        failure_summary: Option<String>,
        failure_class: Option<crate::failure::FailureClass>,
        next: Option<NodeId>,
    ) -> DomainResult<()> {
        let record = self
            .attempts
            .iter_mut()
            .find(|record| {
                record.node_id == node
                    && record.candidate_id == *candidate
                    && record.attempt == attempt
            })
            .ok_or_else(|| {
                DomainError::InvariantViolated(format!(
                    "no started attempt {attempt} exists for node {node}"
                ))
            })?;
        if !record.is_open() {
            return Err(DomainError::InvariantViolated(format!(
                "attempt {attempt} for node {node} is already {}",
                record.status.as_str()
            )));
        }
        record.status = status;
        record.finished_at = Some(at);
        record.duration = if duration == DurationMs::ZERO {
            at.duration_since(record.started_at)
        } else {
            duration
        };
        record.failure_summary = failure_summary;
        record.failure_class = failure_class;
        record.next = next;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunManifest {
    pub schema_version: u32,
    pub run_id: RunId,
    pub last_event_sequence: u64,
    pub last_event_hash: String,
    pub node_evidence_paths: Vec<String>,
    pub candidate_paths: Vec<String>,
    pub artifact_count: u64,
}

impl RunManifest {
    pub fn empty(run_id: RunId) -> Self {
        Self {
            schema_version: RUN_PROJECTION_SCHEMA_VERSION,
            run_id,
            last_event_sequence: 0,
            last_event_hash: GENESIS_HASH.to_string(),
            node_evidence_paths: Vec::new(),
            candidate_paths: Vec::new(),
            artifact_count: 0,
        }
    }
}

pub fn replay(run_id: RunId, created_at: Timestamp, events: &[DurableEvent]) -> DomainResult<RunProjection> {
    let mut projection = RunProjection::genesis(run_id, created_at);
    for event in events {
        projection.apply(event)?;
    }
    Ok(projection)
}

pub fn replay_from(
    projection: &mut RunProjection,
    events: &[DurableEvent],
) -> DomainResult<u64> {
    let mut applied = 0;
    for event in events {
        if event.sequence <= projection.last_event_sequence {
            continue;
        }
        projection.apply(event)?;
        applied += 1;
    }
    Ok(applied)
}
