use std::path::PathBuf;

use async_trait::async_trait;
use heikas_domain::command::CommandSpecification;
use heikas_domain::identity::{CandidateId, CommitHash, RunId};
use heikas_domain::review::ReviewReport;
use heikas_domain::test_evidence::TestEvidence;

use crate::configuration::EffectiveConfiguration;
use crate::error::ApplicationResult;
use crate::ports::process::CancellationSignal;

#[derive(Debug, Clone)]
pub struct GateContext {
    pub run_id: RunId,
    pub candidate_id: Option<CandidateId>,
    pub worktree: PathBuf,
    pub repository: PathBuf,
    pub baseline: CommitHash,
    pub changed_paths: Vec<String>,
    pub plan_expected_files: Vec<String>,
    pub configuration: EffectiveConfiguration,
    pub cancellation: CancellationSignal,
}

#[derive(Debug, Clone)]
pub struct GateArtifact {
    pub label: String,
    pub relative_path: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct TestGateOutput {
    pub evidence: TestEvidence,
    pub artifacts: Vec<GateArtifact>,
}

#[derive(Debug, Clone)]
pub struct ReviewGateOutput {
    pub report: ReviewReport,
    pub artifacts: Vec<GateArtifact>,
}

#[async_trait]
pub trait TestGateRunner: Send + Sync {
    async fn run_tests(
        &self,
        context: &GateContext,
        commands: &[CommandSpecification],
    ) -> ApplicationResult<TestGateOutput>;
}

#[async_trait]
pub trait ReviewProvider: Send + Sync {
    fn name(&self) -> &str;
    fn required(&self) -> bool;
    fn advisory(&self) -> bool;
    async fn available(&self) -> ApplicationResult<bool>;
    async fn review(&self, context: &GateContext) -> ApplicationResult<ReviewGateOutput>;
}
