use std::path::PathBuf;

use async_trait::async_trait;
use heikas_domain::clock::DurationMs;
use heikas_domain::command::{CommandId, CommandSpecification};
use heikas_domain::identity::{CandidateId, ContentDigest, RunId};
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::run::CandidateStrategy;
use serde::{Deserialize, Serialize};

use crate::configuration::{AgentDriverKind, NetworkPolicy};
use crate::error::ApplicationResult;
use crate::ports::process::CancellationSignal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Implementer,
    Repairer,
    Reviewer,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Planner => "planner",
            AgentRole::Implementer => "implementer",
            AgentRole::Repairer => "repairer",
            AgentRole::Reviewer => "reviewer",
        }
    }

    pub fn is_read_only(&self) -> bool {
        matches!(self, AgentRole::Planner | AgentRole::Reviewer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolPolicy {
    pub allow_read: bool,
    pub allow_search: bool,
    pub allow_git_inspection: bool,
    pub allow_write: bool,
    pub allow_delete: bool,
    pub allow_patch: bool,
    pub allowed_command_ids: Vec<CommandId>,
    pub path_policy: PathPolicy,
    pub maximum_tool_calls: u32,
}

impl ToolPolicy {
    pub fn read_only(path_policy: PathPolicy, maximum_tool_calls: u32) -> Self {
        Self {
            allow_read: true,
            allow_search: true,
            allow_git_inspection: true,
            allow_write: false,
            allow_delete: false,
            allow_patch: false,
            allowed_command_ids: Vec::new(),
            path_policy,
            maximum_tool_calls,
        }
    }

    pub fn editing(
        path_policy: PathPolicy,
        allowed_command_ids: Vec<CommandId>,
        maximum_tool_calls: u32,
    ) -> Self {
        Self {
            allow_read: true,
            allow_search: true,
            allow_git_inspection: true,
            allow_write: true,
            allow_delete: true,
            allow_patch: true,
            allowed_command_ids,
            path_policy,
            maximum_tool_calls,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromptContract {
    pub template_id: String,
    pub template_version: u32,
    pub template_hash: ContentDigest,
    pub rendered: String,
    pub completion_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct AgentInvocation {
    pub run_id: RunId,
    pub candidate_id: Option<CandidateId>,
    pub role: AgentRole,
    pub strategy: Option<CandidateStrategy>,
    pub worktree: PathBuf,
    pub prompt: PromptContract,
    pub tool_policy: ToolPolicy,
    pub commands: Vec<CommandSpecification>,
    pub environment_allowlist: Vec<String>,
    pub network: NetworkPolicy,
    pub time_budget_seconds: u32,
    pub turn_budget: u32,
    pub output_budget_bytes: u64,
    pub cancellation: CancellationSignal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentExitReason {
    Completed,
    TurnBudgetExhausted,
    TimeBudgetExhausted,
    Cancelled,
    ToolPolicyViolation,
    DriverFailure,
}

impl AgentExitReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentExitReason::Completed => "completed",
            AgentExitReason::TurnBudgetExhausted => "turn_budget_exhausted",
            AgentExitReason::TimeBudgetExhausted => "time_budget_exhausted",
            AgentExitReason::Cancelled => "cancelled",
            AgentExitReason::ToolPolicyViolation => "tool_policy_violation",
            AgentExitReason::DriverFailure => "driver_failure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentToolCall {
    pub ordinal: u32,
    pub tool: String,
    pub arguments_digest: ContentDigest,
    pub accepted: bool,
    pub rejection_reason: Option<String>,
    pub duration: DurationMs,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct AgentUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_minor_units: Option<u64>,
    pub cost_currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentOutcome {
    pub exit_reason: AgentExitReason,
    pub model_identity: String,
    pub driver: AgentDriverKind,
    pub tool_calls: Vec<AgentToolCall>,
    pub usage: AgentUsage,
    pub structured_response: Option<serde_json::Value>,
    pub stdout: String,
    pub stderr: String,
    pub changed_paths: Vec<String>,
    pub duration: DurationMs,
    pub diagnostics: Vec<String>,
}

impl AgentOutcome {
    pub fn completed(&self) -> bool {
        self.exit_reason == AgentExitReason::Completed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IsolationStrength {
    None,
    ProcessEnvironment,
    WorkingDirectoryRestricted,
    OperatingSystemSandbox,
}

impl IsolationStrength {
    pub fn as_str(&self) -> &'static str {
        match self {
            IsolationStrength::None => "none",
            IsolationStrength::ProcessEnvironment => "process_environment",
            IsolationStrength::WorkingDirectoryRestricted => "working_directory_restricted",
            IsolationStrength::OperatingSystemSandbox => "operating_system_sandbox",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            IsolationStrength::None => "No enforced isolation",
            IsolationStrength::ProcessEnvironment => "Environment allowlist only",
            IsolationStrength::WorkingDirectoryRestricted => {
                "Working directory and environment restricted"
            }
            IsolationStrength::OperatingSystemSandbox => "Operating system sandbox",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentCapabilities {
    pub driver: AgentDriverKind,
    pub available: bool,
    pub version: Option<String>,
    pub model_identity: Option<String>,
    pub supports_structured_tool_calls: bool,
    pub supports_non_interactive: bool,
    pub isolation: IsolationStrength,
    pub honours_write_restriction: bool,
    pub context_window_tokens: Option<u64>,
    pub endpoint: Option<String>,
    pub requires_paid_account: bool,
    pub demonstration_only: bool,
    pub diagnostics: Vec<String>,
}

#[async_trait]
pub trait AgentDriver: Send + Sync {
    fn kind(&self) -> AgentDriverKind;
    async fn capabilities(&self) -> ApplicationResult<AgentCapabilities>;
    async fn invoke(&self, invocation: AgentInvocation) -> ApplicationResult<AgentOutcome>;
}
