use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use heikas_application::configuration::{AgentConfiguration, AgentDriverKind};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::agent::{
    AgentCapabilities, AgentDriver, AgentExitReason, AgentInvocation, AgentOutcome, AgentUsage,
    IsolationStrength,
};
use heikas_application::ports::process::{ProcessRequest, ProcessRunner};
use heikas_domain::clock::DurationMs;
use serde_json::Value;
use tokio::sync::OnceCell;

use crate::agent::changes::{difference, observe_changed_paths};

pub struct ExternalCliAgentDriver {
    kind: AgentDriverKind,
    configuration: AgentConfiguration,
    processes: Arc<dyn ProcessRunner>,
    capabilities: OnceCell<AgentCapabilities>,
}

impl ExternalCliAgentDriver {
    pub fn new(
        kind: AgentDriverKind,
        configuration: AgentConfiguration,
        processes: Arc<dyn ProcessRunner>,
    ) -> Self {
        Self {
            kind,
            configuration,
            processes,
            capabilities: OnceCell::new(),
        }
    }

    fn executable(&self) -> String {
        self.configuration
            .executable
            .clone()
            .unwrap_or_else(|| default_executable(self.kind).to_string())
    }

    fn restriction_arguments(&self, invocation: &AgentInvocation) -> Vec<String> {
        let read_only = invocation.role.is_read_only();
        match self.kind {
            AgentDriverKind::ClaudeCode => {
                let mut args = vec![
                    "--print".to_string(),
                    "--output-format".to_string(),
                    "json".to_string(),
                    "--permission-mode".to_string(),
                    if read_only {
                        "plan".to_string()
                    } else {
                        "acceptEdits".to_string()
                    },
                ];
                if read_only {
                    args.push("--allowedTools".to_string());
                    args.push("Read,Glob,Grep".to_string());
                } else {
                    args.push("--allowedTools".to_string());
                    args.push("Read,Glob,Grep,Edit,Write".to_string());
                }
                args
            }
            AgentDriverKind::CodexCli => {
                let mut args = vec!["exec".to_string(), "--json".to_string()];
                args.push("--sandbox".to_string());
                args.push(if read_only {
                    "read-only".to_string()
                } else {
                    "workspace-write".to_string()
                });
                args
            }
            AgentDriverKind::OpenCode => {
                vec!["run".to_string(), "--print-logs".to_string()]
            }
            _ => Vec::new(),
        }
    }

    fn isolation_for(&self) -> IsolationStrength {
        match self.kind {
            AgentDriverKind::CodexCli => IsolationStrength::OperatingSystemSandbox,
            AgentDriverKind::ClaudeCode | AgentDriverKind::OpenCode => {
                IsolationStrength::WorkingDirectoryRestricted
            }
            _ => IsolationStrength::ProcessEnvironment,
        }
    }

    fn honours_write_restriction(&self) -> bool {
        matches!(
            self.kind,
            AgentDriverKind::ClaudeCode | AgentDriverKind::CodexCli
        )
    }

    async fn resolve_capabilities(&self) -> AgentCapabilities {
        let executable = self.executable();
        let version = self
            .processes
            .probe_executable(&executable)
            .await
            .ok()
            .flatten();
        let available = version.is_some();
        let mut diagnostics = Vec::new();
        if !available {
            diagnostics.push(format!(
                "the executable `{executable}` was not found on the path"
            ));
        }
        if !self.honours_write_restriction() {
            diagnostics.push(
                "this adapter cannot enforce a read-only tool policy, so planning is refused"
                    .to_string(),
            );
        }
        AgentCapabilities {
            driver: self.kind,
            available,
            version,
            model_identity: self.configuration.model.clone(),
            supports_structured_tool_calls: available,
            supports_non_interactive: true,
            isolation: self.isolation_for(),
            honours_write_restriction: self.honours_write_restriction(),
            context_window_tokens: None,
            endpoint: self.configuration.endpoint.clone(),
            requires_paid_account: self.kind.requires_paid_account(),
            demonstration_only: false,
            diagnostics,
        }
    }
}

fn default_executable(kind: AgentDriverKind) -> &'static str {
    match kind {
        AgentDriverKind::ClaudeCode => "claude",
        AgentDriverKind::CodexCli => "codex",
        AgentDriverKind::OpenCode => "opencode",
        _ => "heikas-agent",
    }
}

#[async_trait]
impl AgentDriver for ExternalCliAgentDriver {
    fn kind(&self) -> AgentDriverKind {
        self.kind
    }

    async fn capabilities(&self) -> ApplicationResult<AgentCapabilities> {
        let capabilities = self
            .capabilities
            .get_or_init(|| async { self.resolve_capabilities().await })
            .await;
        Ok(capabilities.clone())
    }

    async fn invoke(&self, invocation: AgentInvocation) -> ApplicationResult<AgentOutcome> {
        let capabilities = self.capabilities().await?;
        if !capabilities.available {
            return Err(ApplicationError::Agent(format!(
                "the `{}` adapter is not available: {}",
                self.kind.as_str(),
                capabilities.diagnostics.join("; ")
            )));
        }
        if invocation.role.is_read_only() && !capabilities.honours_write_restriction {
            return Err(ApplicationError::PolicyViolation(format!(
                "the `{}` adapter cannot enforce the read-only policy that the {} role requires",
                self.kind.as_str(),
                invocation.role.as_str()
            )));
        }

        let started = Instant::now();
        let before = observe_changed_paths(&invocation.worktree)?;
        let mut args = self.restriction_arguments(&invocation);
        args.extend(self.configuration.extra_arguments.iter().cloned());
        if let Some(model) = &self.configuration.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        let prompt = format!(
            "{}\n\nReturn a final JSON object matching this schema:\n{}\n",
            invocation.prompt.rendered,
            serde_json::to_string_pretty(&invocation.prompt.completion_schema)
                .unwrap_or_else(|_| "{}".to_string())
        );
        args.push(prompt);

        let mut environment = Vec::new();
        for name in &invocation.environment_allowlist {
            if let Ok(value) = std::env::var(name) {
                environment.push((name.clone(), value));
            }
        }
        if let Some(name) = &self.configuration.api_key_environment_variable {
            if let Ok(value) = std::env::var(name) {
                environment.push((name.clone(), value));
            }
        }

        let request = ProcessRequest {
            program: self.executable(),
            args,
            working_directory: invocation.worktree.clone(),
            environment,
            timeout_seconds: invocation.time_budget_seconds,
            max_output_bytes: invocation.output_budget_bytes,
            label: format!("agent:{}", self.kind.as_str()),
        };
        let outcome = self
            .processes
            .run(request, invocation.cancellation.clone())
            .await?;
        let after = observe_changed_paths(&invocation.worktree)?;

        let exit_reason = if outcome.cancelled {
            AgentExitReason::Cancelled
        } else if outcome.timed_out {
            AgentExitReason::TimeBudgetExhausted
        } else if outcome.succeeded() {
            AgentExitReason::Completed
        } else {
            AgentExitReason::DriverFailure
        };

        let structured_response = extract_structured_response(&outcome.stdout_text());

        Ok(AgentOutcome {
            exit_reason: if exit_reason == AgentExitReason::Completed
                && structured_response.is_none()
            {
                AgentExitReason::DriverFailure
            } else {
                exit_reason
            },
            model_identity: capabilities
                .model_identity
                .clone()
                .unwrap_or_else(|| self.kind.as_str().to_string()),
            driver: self.kind,
            tool_calls: Vec::new(),
            usage: AgentUsage::default(),
            structured_response,
            stdout: outcome.stdout_text(),
            stderr: outcome.stderr_text(),
            changed_paths: difference(&before, &after),
            duration: DurationMs::from_millis(started.elapsed().as_millis() as u64),
            diagnostics: capabilities.diagnostics.clone(),
        })
    }
}

fn extract_structured_response(stdout: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        if let Some(result) = value.get("result").and_then(Value::as_str) {
            if let Ok(inner) = serde_json::from_str::<Value>(result.trim()) {
                return Some(inner);
            }
        }
        if value.is_object() {
            return Some(value);
        }
    }
    let start = stdout.rfind('{')?;
    let candidate = &stdout[start..];
    serde_json::from_str::<Value>(candidate.trim()).ok()
}
