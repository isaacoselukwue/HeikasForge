use std::path::{Path, PathBuf};

use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_domain::identity::{CandidateId, RunId};

pub const HEIKAS_HOME_VARIABLE: &str = "HEIKAS_HOME";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLayout {
    root: PathBuf,
}

impl StoreLayout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn discover() -> ApplicationResult<Self> {
        if let Ok(value) = std::env::var(HEIKAS_HOME_VARIABLE) {
            if !value.trim().is_empty() {
                return Ok(Self::new(PathBuf::from(value)));
            }
        }
        let base = platform_data_root().ok_or_else(|| {
            ApplicationError::Storage(
                "the platform application data directory could not be determined".to_string(),
            )
        })?;
        Ok(Self::new(base.join("heikas-forge")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_directory(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn user_configuration(&self) -> PathBuf {
        self.config_directory().join("forge.toml")
    }

    pub fn runs_directory(&self) -> PathBuf {
        self.root.join("runs")
    }

    pub fn run_directory(&self, run_id: RunId) -> PathBuf {
        self.runs_directory().join(run_id.to_string())
    }

    pub fn task_file(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("task.md")
    }

    pub fn run_descriptor(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("run.json")
    }

    pub fn state_file(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("state.json")
    }

    pub fn manifest_file(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("manifest.json")
    }

    pub fn metrics_file(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("metrics.json")
    }

    pub fn events_file(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("events.jsonl")
    }

    pub fn quarantine_file(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("events.quarantine.jsonl")
    }

    pub fn plan_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("plan")
    }

    pub fn plan_version_file(&self, run_id: RunId, version: u32) -> PathBuf {
        self.plan_directory(run_id)
            .join(format!("plan-v{version}.md"))
    }

    pub fn nodes_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("nodes")
    }

    pub fn candidates_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("candidates")
    }

    pub fn candidate_directory(&self, run_id: RunId, candidate: &CandidateId) -> PathBuf {
        self.candidates_directory(run_id).join(candidate.as_str())
    }

    pub fn integration_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("integration")
    }

    pub fn artifacts_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("artifacts")
    }

    pub fn artifact_index(&self, run_id: RunId) -> PathBuf {
        self.artifacts_directory(run_id).join("index.json")
    }

    pub fn logs_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("logs")
    }

    pub fn run_log(&self, run_id: RunId) -> PathBuf {
        self.logs_directory(run_id).join("run.jsonl")
    }

    pub fn exports_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("exports")
    }

    pub fn locks_directory(&self, run_id: RunId) -> PathBuf {
        self.run_directory(run_id).join("locks")
    }

    pub fn dispatcher_lock(&self, run_id: RunId) -> PathBuf {
        self.locks_directory(run_id).join("dispatcher.lock")
    }

    pub fn worktrees_directory(&self) -> PathBuf {
        self.root.join("worktrees")
    }

    pub fn run_worktrees(&self, run_id: RunId) -> PathBuf {
        self.worktrees_directory().join(run_id.to_string())
    }

    pub fn candidate_worktree(&self, run_id: RunId, candidate: &CandidateId) -> PathBuf {
        self.run_worktrees(run_id).join(candidate.as_str())
    }

    pub fn integration_worktree(&self, run_id: RunId) -> PathBuf {
        self.run_worktrees(run_id).join("integration")
    }
}

#[cfg(target_os = "linux")]
fn platform_data_root() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("XDG_DATA_HOME") {
        if !value.trim().is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".local").join("share"))
}

#[cfg(target_os = "macos")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
    })
}

#[cfg(target_os = "windows")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("APPDATA").ok().map(PathBuf::from))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
