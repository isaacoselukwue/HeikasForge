use async_trait::async_trait;
use heikas_domain::event::{DurableEvent, EventPayload};
use heikas_domain::identity::{AttemptNumber, CandidateId, ContentDigest, RunId};
use heikas_domain::node::NodeResult;
use heikas_domain::plan::PlanVersion;
use heikas_domain::review::AggregatedReview;
use heikas_domain::score::Ranking;
use heikas_domain::state::{RunManifest, RunProjection};
use heikas_domain::test_evidence::TestEvidence;

use crate::configuration::EffectiveConfiguration;
use crate::error::ApplicationResult;
use crate::model::attempt::{AttemptEvidence, AttemptKey, StoredArtifact};
use crate::model::run_summary::RunHeader;

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, run_id: RunId, payload: EventPayload)
        -> ApplicationResult<DurableEvent>;
    async fn read_after(
        &self,
        run_id: RunId,
        sequence: u64,
    ) -> ApplicationResult<Vec<DurableEvent>>;
    async fn read_range(
        &self,
        run_id: RunId,
        from_sequence: u64,
        limit: usize,
    ) -> ApplicationResult<Vec<DurableEvent>>;
    async fn verify_chain(&self, run_id: RunId) -> ApplicationResult<ChainVerification>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainVerification {
    pub events_verified: u64,
    pub last_sequence: u64,
    pub last_hash: String,
    pub quarantined_partial_record: bool,
}

#[async_trait]
pub trait ProjectionStore: Send + Sync {
    async fn load(&self, run_id: RunId) -> ApplicationResult<Option<RunProjection>>;
    async fn store(&self, projection: &RunProjection) -> ApplicationResult<()>;
    async fn store_manifest(&self, manifest: &RunManifest) -> ApplicationResult<()>;
    async fn load_manifest(&self, run_id: RunId) -> ApplicationResult<Option<RunManifest>>;
    async fn store_metrics(
        &self,
        run_id: RunId,
        projection: &RunProjection,
    ) -> ApplicationResult<()>;
}

#[async_trait]
pub trait RunCatalogue: Send + Sync {
    async fn initialise(
        &self,
        run_id: RunId,
        task_markdown: &str,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<()>;
    async fn exists(&self, run_id: RunId) -> ApplicationResult<bool>;
    async fn headers(&self) -> ApplicationResult<Vec<RunHeader>>;
    async fn task_markdown(&self, run_id: RunId) -> ApplicationResult<String>;
    async fn configuration(&self, run_id: RunId) -> ApplicationResult<EffectiveConfiguration>;
    async fn remove_worktrees(&self, run_id: RunId) -> ApplicationResult<Vec<String>>;
    async fn resolve_run_reference(&self, reference: &str) -> ApplicationResult<RunId>;
}

#[async_trait]
pub trait EvidenceStore: Send + Sync {
    async fn commit_attempt(
        &self,
        run_id: RunId,
        result: &NodeResult,
        evidence: AttemptEvidence,
    ) -> ApplicationResult<()>;
    async fn read_attempt_result(
        &self,
        run_id: RunId,
        key: &AttemptKey,
    ) -> ApplicationResult<Option<NodeResult>>;
    async fn store_artifact(
        &self,
        run_id: RunId,
        label: &str,
        relative_path: &str,
        bytes: &[u8],
        truncated: bool,
    ) -> ApplicationResult<StoredArtifact>;
    async fn read_artifact(
        &self,
        run_id: RunId,
        artifact_id: &ContentDigest,
    ) -> ApplicationResult<Vec<u8>>;
    async fn read_artifact_range(
        &self,
        run_id: RunId,
        artifact_id: &ContentDigest,
        offset: u64,
        length: u64,
    ) -> ApplicationResult<Vec<u8>>;
}

#[async_trait]
pub trait PlanStore: Send + Sync {
    async fn write_version(
        &self,
        run_id: RunId,
        version: u32,
        markdown: &str,
        author: heikas_domain::plan::PlanAuthor,
        revision_note: Option<String>,
        recorded_at: heikas_domain::clock::Timestamp,
    ) -> ApplicationResult<PlanVersion>;
    async fn read_version(&self, run_id: RunId, version: u32) -> ApplicationResult<String>;
    async fn read_current(&self, run_id: RunId) -> ApplicationResult<Option<(u32, String)>>;
}

#[async_trait]
pub trait CandidateEvidenceStore: Send + Sync {
    async fn write_diff(
        &self,
        run_id: RunId,
        candidate: &CandidateId,
        patch: &[u8],
    ) -> ApplicationResult<ContentDigest>;
    async fn read_diff(&self, run_id: RunId, candidate: &CandidateId)
        -> ApplicationResult<Vec<u8>>;
    async fn write_test_evidence(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
        attempt: AttemptNumber,
        evidence: &TestEvidence,
    ) -> ApplicationResult<()>;
    async fn read_test_evidence(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
    ) -> ApplicationResult<Option<TestEvidence>>;
    async fn write_review(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
        attempt: AttemptNumber,
        review: &AggregatedReview,
    ) -> ApplicationResult<()>;
    async fn read_review(
        &self,
        run_id: RunId,
        candidate: Option<&CandidateId>,
    ) -> ApplicationResult<Option<AggregatedReview>>;
    async fn write_ranking(&self, run_id: RunId, ranking: &Ranking) -> ApplicationResult<()>;
    async fn write_integration_diff(
        &self,
        run_id: RunId,
        patch: &[u8],
    ) -> ApplicationResult<ContentDigest>;
    async fn read_integration_diff(&self, run_id: RunId) -> ApplicationResult<Vec<u8>>;
}

#[async_trait]
pub trait RunLockService: Send + Sync {
    async fn acquire(&self, run_id: RunId) -> ApplicationResult<Box<dyn RunLockGuard>>;
    async fn is_locked(&self, run_id: RunId) -> ApplicationResult<bool>;
}

pub trait RunLockGuard: Send + Sync {
    fn run_id(&self) -> RunId;
    fn release(self: Box<Self>);
}

pub trait RunStore:
    EventStore + ProjectionStore + RunCatalogue + EvidenceStore + PlanStore + CandidateEvidenceStore
{
}

impl<T> RunStore for T where
    T: EventStore
        + ProjectionStore
        + RunCatalogue
        + EvidenceStore
        + PlanStore
        + CandidateEvidenceStore
{
}
