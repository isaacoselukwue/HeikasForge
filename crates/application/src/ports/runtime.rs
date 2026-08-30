use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use heikas_domain::identity::RunId;

use crate::configuration::{
    EffectiveConfiguration, RepositoryTrustDecision, RepositoryTrustRecord,
};
use crate::error::ApplicationResult;
use crate::model::request::CreateRunRequest;
use crate::ports::agent::AgentDriver;
use crate::ports::observability::Redactor;
use crate::ports::quality::{ReviewProvider, TestGateRunner};

#[async_trait]
pub trait ConfigurationResolver: Send + Sync {
    async fn detect(&self, repository: &Path) -> ApplicationResult<EffectiveConfiguration>;
    async fn resolve(
        &self,
        request: &CreateRunRequest,
    ) -> ApplicationResult<EffectiveConfiguration>;
    async fn write_repository_configuration(
        &self,
        repository: &Path,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<PathBuf>;
    async fn user_configuration_path(&self) -> ApplicationResult<PathBuf>;
    async fn repository_trust(
        &self,
        repository: &Path,
    ) -> ApplicationResult<RepositoryTrustDecision>;
    async fn trust_repository(&self, repository: &Path)
        -> ApplicationResult<RepositoryTrustRecord>;
    async fn revoke_repository_trust(&self, repository: &Path) -> ApplicationResult<bool>;
    async fn trusted_repositories(&self) -> ApplicationResult<Vec<RepositoryTrustRecord>>;
}

#[async_trait]
pub trait RuntimeFactory: Send + Sync {
    async fn agent_driver(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Arc<dyn AgentDriver>>;
    async fn review_providers(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Vec<Arc<dyn ReviewProvider>>>;
    async fn test_runner(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Arc<dyn TestGateRunner>>;
    async fn redactor(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Arc<dyn Redactor>>;
}

#[async_trait]
pub trait EvidenceExporter: Send + Sync {
    async fn export(
        &self,
        run_id: RunId,
        output_path: &Path,
        include_worktrees: bool,
    ) -> ApplicationResult<ExportOutcome>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    pub archive_path: PathBuf,
    pub byte_length: u64,
    pub entry_count: u64,
    pub redacted_entries: u64,
    pub unredactable_entries: u64,
    pub excluded_sensitive_paths: Vec<String>,
}

impl ExportOutcome {
    pub fn fully_redacted(&self) -> bool {
        self.unredactable_entries == 0
    }
}
