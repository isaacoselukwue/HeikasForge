use std::path::{Path, PathBuf};
use std::time::Instant;

use async_trait::async_trait;
use heikas_application::configuration::AgentDriverKind;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::agent::{
    AgentCapabilities, AgentDriver, AgentExitReason, AgentInvocation, AgentOutcome, AgentRole,
    AgentToolCall, AgentUsage, IsolationStrength,
};
use heikas_domain::clock::DurationMs;
use heikas_domain::identity::ContentDigest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::changes::{difference, observe_changed_paths};

pub const FIXTURE_MARKER_FILE: &str = ".heikas-fixture";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureScript {
    pub schema_version: u32,
    pub model_identity: String,
    pub steps: Vec<FixtureStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureStep {
    pub role: String,
    #[serde(default)]
    pub candidate_ordinal: Option<u8>,
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(default)]
    pub writes: Vec<FixtureWrite>,
    #[serde(default)]
    pub deletes: Vec<String>,
    pub structured_response: Value,
    #[serde(default)]
    pub exit_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureWrite {
    pub path: String,
    pub contents: String,
}

pub struct DeterministicFakeAgentDriver {
    script: FixtureScript,
    script_path: PathBuf,
}

impl DeterministicFakeAgentDriver {
    pub fn load(script_path: &Path) -> ApplicationResult<Self> {
        let bytes = std::fs::read(script_path)
            .map_err(|error| crate::atomic::storage(script_path, "read", error))?;
        let script: FixtureScript = serde_json::from_slice(&bytes).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "the demonstration fixture script could not be decoded: {error}"
            ))
        })?;
        if script.schema_version != 1 {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "the demonstration fixture script version {} is not supported",
                script.schema_version
            )));
        }
        Ok(Self {
            script,
            script_path: script_path.to_path_buf(),
        })
    }

    fn select_step(&self, invocation: &AgentInvocation) -> Option<&FixtureStep> {
        let ordinal = invocation
            .candidate_id
            .as_ref()
            .and_then(|candidate| candidate.ordinal())
            .map(|ordinal| ordinal.get());
        let attempt_role = role_name(invocation.role);
        self.script
            .steps
            .iter()
            .filter(|step| step.role == attempt_role)
            .find(|step| match (step.candidate_ordinal, ordinal) {
                (None, _) => true,
                (Some(expected), Some(actual)) => expected == actual,
                (Some(_), None) => false,
            })
    }

    fn select_step_for_attempt(
        &self,
        invocation: &AgentInvocation,
        attempt: u32,
    ) -> Option<&FixtureStep> {
        let ordinal = invocation
            .candidate_id
            .as_ref()
            .and_then(|candidate| candidate.ordinal())
            .map(|ordinal| ordinal.get());
        let attempt_role = role_name(invocation.role);
        self.script
            .steps
            .iter()
            .filter(|step| step.role == attempt_role)
            .filter(|step| match (step.candidate_ordinal, ordinal) {
                (None, _) => true,
                (Some(expected), Some(actual)) => expected == actual,
                (Some(_), None) => false,
            })
            .find(|step| step.attempt.unwrap_or(1) == attempt)
            .or_else(|| self.select_step(invocation))
    }
}

fn role_name(role: AgentRole) -> String {
    role.as_str().to_string()
}

#[async_trait]
impl AgentDriver for DeterministicFakeAgentDriver {
    fn kind(&self) -> AgentDriverKind {
        AgentDriverKind::Fake
    }

    async fn capabilities(&self) -> ApplicationResult<AgentCapabilities> {
        Ok(AgentCapabilities {
            driver: AgentDriverKind::Fake,
            available: true,
            version: Some(format!("fixture {}", self.script_path.display())),
            model_identity: Some(self.script.model_identity.clone()),
            supports_structured_tool_calls: true,
            supports_non_interactive: true,
            isolation: IsolationStrength::WorkingDirectoryRestricted,
            honours_write_restriction: true,
            context_window_tokens: None,
            endpoint: None,
            requires_paid_account: false,
            demonstration_only: true,
            diagnostics: vec![
                "This deterministic driver replays a recorded fixture and performs no model inference."
                    .to_string(),
            ],
        })
    }

    async fn invoke(&self, invocation: AgentInvocation) -> ApplicationResult<AgentOutcome> {
        let started = Instant::now();
        let marker = invocation.worktree.join(FIXTURE_MARKER_FILE);
        if !marker.exists() {
            return Err(ApplicationError::PolicyViolation(format!(
                "the demonstration agent refuses to operate on `{}` because it carries no `{FIXTURE_MARKER_FILE}` marker",
                invocation.worktree.display()
            )));
        }

        let attempt = invocation
            .prompt
            .rendered
            .lines()
            .find_map(|line| line.strip_prefix("Repair attempt: "))
            .and_then(|value| value.trim().parse::<u32>().ok())
            .unwrap_or(1);

        let Some(step) = self.select_step_for_attempt(&invocation, attempt) else {
            return Err(ApplicationError::Agent(format!(
                "the demonstration fixture has no step for the {} role",
                invocation.role.as_str()
            )));
        };

        let before = observe_changed_paths(&invocation.worktree)?;
        let mut tool_calls = Vec::new();
        let mut ordinal = 0;

        if !invocation.role.is_read_only() {
            for write in &step.writes {
                ordinal += 1;
                let confined = crate::paths::confine(
                    &invocation.worktree,
                    &write.path,
                    heikas_domain::path_policy::PathAccess::Write,
                    &invocation.tool_policy.path_policy,
                )?;
                if let Some(parent) = confined.absolute.parent() {
                    crate::atomic::ensure_directory(parent)?;
                }
                crate::atomic::write_atomic(&confined.absolute, write.contents.as_bytes())?;
                tool_calls.push(AgentToolCall {
                    ordinal,
                    tool: "write_file".to_string(),
                    arguments_digest: ContentDigest::of_str(&write.path),
                    accepted: true,
                    rejection_reason: None,
                    duration: DurationMs::from_millis(1),
                    summary: format!("wrote `{}`", confined.relative),
                });
            }
            for path in &step.deletes {
                ordinal += 1;
                let confined = crate::paths::confine(
                    &invocation.worktree,
                    path,
                    heikas_domain::path_policy::PathAccess::Delete,
                    &invocation.tool_policy.path_policy,
                )?;
                if confined.absolute.is_file() {
                    std::fs::remove_file(&confined.absolute).map_err(|error| {
                        crate::atomic::storage(&confined.absolute, "remove", error)
                    })?;
                }
                tool_calls.push(AgentToolCall {
                    ordinal,
                    tool: "delete_file".to_string(),
                    arguments_digest: ContentDigest::of_str(path),
                    accepted: true,
                    rejection_reason: None,
                    duration: DurationMs::from_millis(1),
                    summary: format!("deleted `{}`", confined.relative),
                });
            }
        }

        ordinal += 1;
        tool_calls.push(AgentToolCall {
            ordinal,
            tool: crate::agent::tools::COMPLETION_TOOL.to_string(),
            arguments_digest: ContentDigest::of_str(&step.structured_response.to_string()),
            accepted: true,
            rejection_reason: None,
            duration: DurationMs::from_millis(1),
            summary: "structured completion returned".to_string(),
        });

        let after = observe_changed_paths(&invocation.worktree)?;
        let exit_reason = match step.exit_reason.as_deref() {
            Some("turn_budget_exhausted") => AgentExitReason::TurnBudgetExhausted,
            Some("driver_failure") => AgentExitReason::DriverFailure,
            Some("cancelled") => AgentExitReason::Cancelled,
            _ => AgentExitReason::Completed,
        };

        Ok(AgentOutcome {
            exit_reason,
            model_identity: self.script.model_identity.clone(),
            driver: AgentDriverKind::Fake,
            tool_calls,
            usage: AgentUsage::default(),
            structured_response: Some(step.structured_response.clone()),
            stdout: format!(
                "Demonstration mode replayed the `{}` fixture step.",
                invocation.role.as_str()
            ),
            stderr: String::new(),
            changed_paths: difference(&before, &after),
            duration: DurationMs::from_millis(started.elapsed().as_millis() as u64),
            diagnostics: vec!["demonstration mode".to_string()],
        })
    }
}
