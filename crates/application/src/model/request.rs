use std::path::PathBuf;

use heikas_domain::budget::QualityProfile;
use heikas_domain::command::CommandKind;
use heikas_domain::run::CommitPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommandDeclaration {
    pub kind: CommandKind,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateRunRequest {
    pub repository_path: PathBuf,
    pub task_markdown: String,
    pub candidate_count: Option<u8>,
    pub max_parallel_candidates: Option<u8>,
    pub max_repairs_per_candidate: Option<u32>,
    pub commit_policy: Option<CommitPolicy>,
    pub quality_profile: Option<QualityProfile>,
    pub minimum_line_coverage: Option<f64>,
    pub include_dirty: bool,
    pub agent_driver: Option<String>,
    pub agent_model: Option<String>,
    pub demonstration_mode: bool,
    pub wall_clock_seconds: Option<u32>,
    #[serde(default)]
    pub command_declarations: Vec<CommandDeclaration>,
}

impl CreateRunRequest {
    pub fn new(repository_path: PathBuf, task_markdown: String) -> Self {
        Self {
            repository_path,
            task_markdown,
            candidate_count: None,
            max_parallel_candidates: None,
            max_repairs_per_candidate: None,
            commit_policy: None,
            quality_profile: None,
            minimum_line_coverage: None,
            include_dirty: false,
            agent_driver: None,
            agent_model: None,
            demonstration_mode: false,
            wall_clock_seconds: None,
            command_declarations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanDecisionRequest {
    pub plan_markdown: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CancelRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExportRequest {
    pub output_path: PathBuf,
    pub include_worktrees: bool,
}
