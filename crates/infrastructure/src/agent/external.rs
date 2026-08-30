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
use tokio::sync::{watch, OnceCell};

use crate::agent::changes::{difference, observe_changed_paths};

const BYPASS_ARGUMENTS: [&str; 8] = [
    "--dangerously-skip-permissions",
    "--dangerously-bypass-approvals-and-sandbox",
    "--bypass-permissions",
    "--skip-permissions",
    "--no-sandbox",
    "--full-auto",
    "--yolo",
    "--ask-for-approval",
];

const RESERVED_ARGUMENTS: [&str; 9] = [
    "--permission-mode",
    "--permission-prompt-tool",
    "--allowedtools",
    "--disallowedtools",
    "--add-dir",
    "--sandbox",
    "--print",
    "--output-format",
    "--model",
];

pub struct ExternalCliAgentDriver {
    kind: AgentDriverKind,
    configuration: AgentConfiguration,
    processes: Arc<dyn ProcessRunner>,
    capabilities: OnceCell<AgentCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RestrictionSupport {
    honours_write_restriction: bool,
    isolation: IsolationStrength,
    diagnostics: Vec<String>,
}

impl ExternalCliAgentDriver {
    pub fn new(
        kind: AgentDriverKind,
        configuration: AgentConfiguration,
        processes: Arc<dyn ProcessRunner>,
    ) -> ApplicationResult<Self> {
        validate_extra_arguments(&configuration.extra_arguments)?;
        Ok(Self {
            kind,
            configuration,
            processes,
            capabilities: OnceCell::new(),
        })
    }

    fn executable(&self) -> String {
        self.configuration
            .executable
            .clone()
            .unwrap_or_else(|| default_executable(self.kind).to_string())
    }

    fn subcommand_arguments(&self) -> Vec<String> {
        match self.kind {
            AgentDriverKind::CodexCli => vec!["exec".to_string()],
            AgentDriverKind::OpenCode => vec!["run".to_string()],
            _ => Vec::new(),
        }
    }

    fn restriction_arguments(&self, read_only: bool) -> Vec<String> {
        match self.kind {
            AgentDriverKind::ClaudeCode => {
                vec![
                    "--print".to_string(),
                    "--output-format".to_string(),
                    "json".to_string(),
                    "--permission-mode".to_string(),
                    if read_only {
                        "plan".to_string()
                    } else {
                        "acceptEdits".to_string()
                    },
                    "--allowedTools".to_string(),
                    if read_only {
                        "Read,Glob,Grep".to_string()
                    } else {
                        "Read,Glob,Grep,Edit,Write".to_string()
                    },
                ]
            }
            AgentDriverKind::CodexCli => vec![
                "--json".to_string(),
                "--sandbox".to_string(),
                if read_only {
                    "read-only".to_string()
                } else {
                    "workspace-write".to_string()
                },
            ],
            AgentDriverKind::OpenCode => vec!["--print-logs".to_string()],
            _ => Vec::new(),
        }
    }

    fn required_restriction_options(&self) -> &'static [&'static str] {
        match self.kind {
            AgentDriverKind::ClaudeCode => &["--print", "--permission-mode", "--allowedTools"],
            AgentDriverKind::CodexCli => &["--sandbox"],
            _ => &[],
        }
    }

    fn help_arguments(&self) -> Vec<String> {
        match self.kind {
            AgentDriverKind::CodexCli => vec!["exec".to_string(), "--help".to_string()],
            _ => vec!["--help".to_string()],
        }
    }

    fn prompt_reaches_stdin(&self) -> bool {
        matches!(
            self.kind,
            AgentDriverKind::ClaudeCode | AgentDriverKind::CodexCli
        )
    }

    fn prompt_arguments(&self) -> Vec<String> {
        match self.kind {
            AgentDriverKind::CodexCli => vec!["-".to_string()],
            _ => Vec::new(),
        }
    }

    async fn read_help_text(&self) -> Option<String> {
        let (_sender, cancellation) = watch::channel(false);
        let request = ProcessRequest {
            program: self.executable(),
            args: self.help_arguments(),
            working_directory: std::env::temp_dir(),
            environment: Vec::new(),
            stdin: None,
            timeout_seconds: 30,
            max_output_bytes: 262_144,
            label: format!("agent:{}:help", self.kind.as_str()),
        };
        let outcome = self.processes.run(request, cancellation).await.ok()?;
        let mut text = outcome.stdout_text();
        text.push('\n');
        text.push_str(&outcome.stderr_text());
        Some(text)
    }

    async fn detect_restrictions(&self, available: bool) -> RestrictionSupport {
        let required = self.required_restriction_options();
        if required.is_empty() {
            return RestrictionSupport {
                honours_write_restriction: false,
                isolation: IsolationStrength::None,
                diagnostics: vec![format!(
                    "the `{}` adapter publishes no restriction option, so no write restriction can be enforced and read-only roles are refused",
                    self.kind.as_str()
                )],
            };
        }
        if !available {
            return RestrictionSupport {
                honours_write_restriction: false,
                isolation: IsolationStrength::None,
                diagnostics: vec![
                    "the executable is absent, so its restriction strength could not be detected"
                        .to_string(),
                ],
            };
        }
        let Some(help) = self.read_help_text().await else {
            return RestrictionSupport {
                honours_write_restriction: false,
                isolation: IsolationStrength::None,
                diagnostics: vec![
                    "the command line interface did not describe its options, so its restriction strength could not be detected"
                        .to_string(),
                ],
            };
        };
        let lowered = help.to_lowercase();
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|option| !lowered.contains(&option.to_lowercase()))
            .collect();
        if !missing.is_empty() {
            return RestrictionSupport {
                honours_write_restriction: false,
                isolation: IsolationStrength::None,
                diagnostics: vec![format!(
                    "the installed `{}` does not accept {}, so the required restriction cannot be applied",
                    self.executable(),
                    missing.join(", ")
                )],
            };
        }
        let isolation = match self.kind {
            AgentDriverKind::CodexCli => IsolationStrength::OperatingSystemSandbox,
            _ => IsolationStrength::WorkingDirectoryRestricted,
        };
        RestrictionSupport {
            honours_write_restriction: true,
            isolation,
            diagnostics: vec![format!(
                "the installed `{}` accepts {}, which is the strongest restriction this adapter can apply",
                self.executable(),
                required.join(", ")
            )],
        }
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
        let restrictions = self.detect_restrictions(available).await;
        diagnostics.extend(restrictions.diagnostics.clone());
        if !self.prompt_reaches_stdin() {
            diagnostics.push(
                "this adapter passes the prompt as a command line argument, so the task text is visible to other local processes"
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
            isolation: restrictions.isolation,
            honours_write_restriction: restrictions.honours_write_restriction,
            context_window_tokens: None,
            endpoint: self.configuration.endpoint.clone(),
            requires_paid_account: self.kind.requires_paid_account(),
            demonstration_only: false,
            diagnostics,
        }
    }
}

pub fn validate_extra_arguments(arguments: &[String]) -> ApplicationResult<()> {
    for argument in arguments {
        let name = argument
            .split_once('=')
            .map(|(name, _)| name)
            .unwrap_or(argument)
            .to_ascii_lowercase();
        if BYPASS_ARGUMENTS.contains(&name.as_str()) {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "`{argument}` removes the safety restriction that this adapter is required to apply, so it may not appear in `agent.extra_arguments`"
            )));
        }
        if RESERVED_ARGUMENTS.contains(&name.as_str()) {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "`{argument}` collides with a restriction that this adapter sets itself, so it may not appear in `agent.extra_arguments`"
            )));
        }
    }
    Ok(())
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
        let read_only = invocation.role.is_read_only();
        if read_only && !capabilities.honours_write_restriction {
            return Err(ApplicationError::PolicyViolation(format!(
                "the `{}` adapter cannot enforce the read-only policy that the {} role requires: {}",
                self.kind.as_str(),
                invocation.role.as_str(),
                capabilities.diagnostics.join("; ")
            )));
        }

        let started = Instant::now();
        let before = observe_changed_paths(&invocation.worktree)?;
        let mut args = self.subcommand_arguments();
        args.extend(self.configuration.extra_arguments.iter().cloned());
        args.extend(self.restriction_arguments(read_only));
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
        let stdin = if self.prompt_reaches_stdin() {
            args.extend(self.prompt_arguments());
            Some(prompt.clone().into_bytes())
        } else {
            args.push(prompt.clone());
            None
        };

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
            stdin,
            timeout_seconds: invocation.time_budget_seconds,
            max_output_bytes: invocation.output_budget_bytes,
            label: format!("agent:{}", self.kind.as_str()),
        };
        let outcome = self
            .processes
            .run(request, invocation.cancellation.clone())
            .await?;
        let after = observe_changed_paths(&invocation.worktree)?;
        let changed_paths = difference(&before, &after);

        if read_only && !changed_paths.is_empty() {
            return Err(ApplicationError::PolicyViolation(format!(
                "the `{}` adapter modified {} path(s) during the read-only {} role, so its restriction is not effective",
                self.kind.as_str(),
                changed_paths.len(),
                invocation.role.as_str()
            )));
        }

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
            changed_paths,
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
