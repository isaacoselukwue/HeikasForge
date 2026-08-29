use std::path::{Path, PathBuf};

use async_trait::async_trait;
use heikas_domain::identity::{BranchName, CandidateId, CommitHash, RunId};
use heikas_domain::path_policy::WorktreeRole;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepositoryFacts {
    pub root: PathBuf,
    pub git_directory: PathBuf,
    pub head_commit: CommitHash,
    pub default_branch: String,
    pub current_branch: Option<String>,
    pub is_clean: bool,
    pub staged_paths: Vec<String>,
    pub unstaged_paths: Vec<String>,
    pub untracked_paths: Vec<String>,
    pub configured_user_name: Option<String>,
    pub configured_user_email: Option<String>,
    pub signing_enabled: bool,
    pub signing_key: Option<String>,
    pub has_submodules: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DirtySnapshot {
    pub tracked_patch: Vec<u8>,
    pub untracked_archive: Vec<u8>,
    pub untracked_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeHandle {
    pub path: PathBuf,
    pub branch: BranchName,
    pub role: WorktreeRole,
    pub baseline: CommitHash,
    pub reused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct DiffSummary {
    pub changed_files: u32,
    pub added_lines: u64,
    pub removed_lines: u64,
    pub paths: Vec<String>,
    pub is_empty: bool,
}

impl DiffSummary {
    pub fn changed_lines(&self) -> u64 {
        self.added_lines.saturating_add(self.removed_lines)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRequest {
    pub worktree: PathBuf,
    pub branch: BranchName,
    pub paths: Vec<String>,
    pub subject: String,
    pub body: String,
    pub author_name: String,
    pub committer_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub commit_hash: CommitHash,
    pub branch: BranchName,
    pub signed: bool,
    pub changed_files: u32,
}

#[async_trait]
pub trait GitService: Send + Sync {
    async fn inspect(&self, repository: &Path) -> ApplicationResult<RepositoryFacts>;
    async fn capture_dirty_snapshot(&self, repository: &Path) -> ApplicationResult<DirtySnapshot>;
    async fn create_worktree(
        &self,
        repository: &Path,
        run_id: RunId,
        candidate: Option<&CandidateId>,
        role: WorktreeRole,
        baseline: &CommitHash,
        branch: &BranchName,
    ) -> ApplicationResult<WorktreeHandle>;
    async fn apply_snapshot(
        &self,
        worktree: &Path,
        snapshot: &DirtySnapshot,
    ) -> ApplicationResult<()>;
    async fn diff_against_baseline(
        &self,
        worktree: &Path,
        baseline: &CommitHash,
    ) -> ApplicationResult<(Vec<u8>, DiffSummary)>;
    async fn apply_patch(&self, worktree: &Path, patch: &[u8]) -> ApplicationResult<()>;
    async fn check_patch_applies(&self, worktree: &Path, patch: &[u8]) -> ApplicationResult<Result<(), String>>;
    async fn reset_worktree(&self, worktree: &Path, baseline: &CommitHash) -> ApplicationResult<()>;
    async fn remove_worktree(&self, repository: &Path, worktree: &Path) -> ApplicationResult<()>;
    async fn list_run_worktrees(&self, repository: &Path, run_id: RunId) -> ApplicationResult<Vec<PathBuf>>;
    async fn create_commit(&self, request: &CommitRequest) -> ApplicationResult<CommitOutcome>;
    async fn changed_paths(&self, worktree: &Path, baseline: &CommitHash) -> ApplicationResult<Vec<String>>;
    async fn list_paths(&self, worktree: &Path, limit: usize) -> ApplicationResult<Vec<String>>;
    async fn file_at_commit(
        &self,
        repository: &Path,
        commit: &CommitHash,
        relative_path: &str,
    ) -> ApplicationResult<Option<Vec<u8>>>;
}
