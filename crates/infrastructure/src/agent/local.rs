use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use heikas_application::configuration::{AgentConfiguration, AgentDriverKind, NetworkPolicy};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::agent::{
    AgentCapabilities, AgentDriver, AgentExitReason, AgentInvocation, AgentOutcome, AgentToolCall,
    AgentUsage, IsolationStrength,
};
use heikas_application::ports::process::ProcessRunner;
use heikas_domain::clock::DurationMs;
use heikas_domain::identity::ContentDigest;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::OnceCell;
use tracing::{debug, warn};

use crate::agent::changes::observe_changed_paths;
use crate::agent::tools::{ToolExecutor, COMPLETION_TOOL};

const PROBE_TIMEOUT: Duration = Duration::from_secs(45);

pub struct LocalModelAgentDriver {
    configuration: AgentConfiguration,
    processes: Arc<dyn ProcessRunner>,
    client: reqwest::Client,
    capabilities: OnceCell<AgentCapabilities>,
}

impl LocalModelAgentDriver {
    pub fn new(
        configuration: AgentConfiguration,
        processes: Arc<dyn ProcessRunner>,
    ) -> ApplicationResult<Self> {
        if let Some(endpoint) = &configuration.endpoint {
            enforce_network_policy(endpoint, configuration.network)?;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(u64::from(configuration.timeout.get())))
            .build()
            .map_err(|error| ApplicationError::Agent(error.to_string()))?;
        Ok(Self {
            configuration,
            processes,
            client,
            capabilities: OnceCell::new(),
        })
    }

    fn endpoint(&self) -> ApplicationResult<String> {
        self.configuration
            .endpoint
            .clone()
            .ok_or_else(|| {
                ApplicationError::InvalidConfiguration(
                    "the local agent driver requires a model endpoint".to_string(),
                )
            })
            .map(|value| value.trim_end_matches('/').to_string())
    }

    fn api_key(&self) -> Option<String> {
        self.configuration
            .api_key_environment_variable
            .as_ref()
            .and_then(|name| std::env::var(name).ok())
    }

    fn model(&self) -> ApplicationResult<String> {
        self.configuration.model.clone().ok_or_else(|| {
            ApplicationError::InvalidConfiguration(
                "the local agent driver requires a model identifier".to_string(),
            )
        })
    }

    async fn post_chat(&self, body: Value, timeout: Duration) -> ApplicationResult<Value> {
        let endpoint = format!("{}/chat/completions", self.endpoint()?);
        let mut request = self.client.post(&endpoint).timeout(timeout).json(&body);
        if let Some(key) = self.api_key() {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(|error| {
            ApplicationError::Agent(format!("the model endpoint could not be reached: {error}"))
        })?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| ApplicationError::Agent(error.to_string()))?;
        if !status.is_success() {
            return Err(ApplicationError::Agent(format!(
                "the model endpoint returned status {status}: {}",
                text.chars().take(600).collect::<String>()
            )));
        }
        serde_json::from_str(&text).map_err(|error| {
            ApplicationError::Agent(format!("the model response was not valid JSON: {error}"))
        })
    }

    async fn list_models(&self) -> ApplicationResult<Vec<String>> {
        let endpoint = format!("{}/models", self.endpoint()?);
        let mut request = self.client.get(&endpoint).timeout(PROBE_TIMEOUT);
        if let Some(key) = self.api_key() {
            request = request.bearer_auth(key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ApplicationError::Agent(error.to_string()))?;
        if !response.status().is_success() {
            return Err(ApplicationError::Agent(format!(
                "the model listing returned status {}",
                response.status()
            )));
        }
        let payload: ModelListing = response
            .json()
            .await
            .map_err(|error| ApplicationError::Agent(error.to_string()))?;
        Ok(payload.data.into_iter().map(|entry| entry.id).collect())
    }

    async fn probe_tool_calls(&self, model: &str) -> (bool, Option<String>) {
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": "You must call the provided tool exactly once." },
                { "role": "user", "content": "Call heikas_probe with value set to 7." }
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "heikas_probe",
                    "description": "A probe used to confirm structured tool calling.",
                    "parameters": {
                        "type": "object",
                        "properties": { "value": { "type": "integer" } },
                        "required": ["value"],
                        "additionalProperties": false
                    }
                }
            }],
            "tool_choice": "auto",
            "temperature": 0,
            "stream": false
        });
        match self.post_chat(body, PROBE_TIMEOUT).await {
            Ok(value) => {
                let has_tool_call = value
                    .pointer("/choices/0/message/tool_calls")
                    .and_then(Value::as_array)
                    .map(|calls| !calls.is_empty())
                    .unwrap_or(false);
                if has_tool_call {
                    (true, None)
                } else {
                    (
                        false,
                        Some(
                            "the model did not produce a structured tool call during the probe"
                                .to_string(),
                        ),
                    )
                }
            }
            Err(error) => (false, Some(error.to_string())),
        }
    }

    async fn resolve_capabilities(&self) -> AgentCapabilities {
        let mut diagnostics = Vec::new();
        let endpoint = match self.endpoint() {
            Ok(endpoint) => endpoint,
            Err(error) => {
                return unavailable(vec![error.to_string()], None);
            }
        };
        let models = match self.list_models().await {
            Ok(models) => models,
            Err(error) => {
                diagnostics.push(format!(
                    "the model runtime at {endpoint} did not answer: {error}"
                ));
                return unavailable(diagnostics, Some(endpoint));
            }
        };
        let configured = self.model().ok();
        if let Some(model) = &configured {
            if !models.is_empty() && !models.iter().any(|candidate| candidate == model) {
                diagnostics.push(format!(
                    "the configured model `{model}` was not listed by the runtime"
                ));
            }
        }
        let model = match configured {
            Some(model) => model,
            None => {
                if models.is_empty() {
                    diagnostics.push("the model runtime reports no available models".to_string());
                    return unavailable(diagnostics, Some(endpoint));
                }
                let ordered = order_by_likely_tool_support(&models);
                let mut chosen = None;
                let mut rejected: Vec<String> = Vec::new();
                for candidate in ordered.iter().take(MAXIMUM_MODEL_PROBES) {
                    let (supported, _) = self.probe_tool_calls(candidate).await;
                    if supported {
                        chosen = Some(candidate.clone());
                        break;
                    }
                    rejected.push(candidate.clone());
                }
                match chosen {
                    Some(model) => {
                        diagnostics.push(format!(
                            "no model was configured, so `{model}` was selected after confirming that it accepts structured tool calls"
                        ));
                        if !rejected.is_empty() {
                            diagnostics.push(format!(
                                "these models were tried first and do not accept tool calls: {}",
                                rejected.join(", ")
                            ));
                        }
                        model
                    }
                    None => {
                        diagnostics.push(format!(
                            "none of the {} models tried accepts structured tool calls: {}. The runtime also offers {}. Install a model that supports tool calling, such as a coding or instruction tuned model, then set it with `model` under `[agent]` in your user configuration.",
                            rejected.len(),
                            rejected.join(", "),
                            describe_remaining(&ordered, MAXIMUM_MODEL_PROBES)
                        ));
                        return unsupported(diagnostics, Some(endpoint), ordered.first().cloned());
                    }
                }
            }
        };
        let (supports_tools, tool_diagnostic) = if std::env::var("HEIKAS_SKIP_AGENT_PROBE").is_ok()
        {
            (true, None)
        } else {
            self.probe_tool_calls(&model).await
        };
        if let Some(detail) = tool_diagnostic {
            diagnostics.push(detail);
        }
        AgentCapabilities {
            driver: AgentDriverKind::Local,
            available: true,
            version: None,
            model_identity: Some(model),
            supports_structured_tool_calls: supports_tools,
            supports_non_interactive: true,
            isolation: IsolationStrength::WorkingDirectoryRestricted,
            honours_write_restriction: true,
            context_window_tokens: None,
            endpoint: Some(endpoint),
            requires_paid_account: false,
            demonstration_only: false,
            diagnostics,
        }
    }
}

const MAXIMUM_MODEL_PROBES: usize = 5;

const HOSTED_MODEL_MARKER: &str = "-cloud";

const LIKELY_TOOL_MARKERS: [&str; 8] = [
    "coder", "code", "instruct", "qwen", "llama", "mistral", "devstral", "granite",
];

const UNLIKELY_TOOL_MARKERS: [&str; 6] = ["-vl", "vl:", "vision", "-v:", "embed", "whisper"];

fn model_rank(model: &str) -> (u8, u8) {
    let lowered = model.to_ascii_lowercase();
    let hosted = u8::from(lowered.contains(HOSTED_MODEL_MARKER));
    if UNLIKELY_TOOL_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        return (hosted, 2);
    }
    if LIKELY_TOOL_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        return (hosted, 0);
    }
    (hosted, 1)
}

fn order_by_likely_tool_support(models: &[String]) -> Vec<String> {
    let mut ordered = models.to_vec();
    ordered.sort_by(|left, right| {
        model_rank(left)
            .cmp(&model_rank(right))
            .then_with(|| left.cmp(right))
    });
    ordered
}

fn describe_remaining(ordered: &[String], probed: usize) -> String {
    let remaining: Vec<&str> = ordered.iter().skip(probed).map(String::as_str).collect();
    if remaining.is_empty() {
        "no further models".to_string()
    } else {
        remaining.join(", ")
    }
}

fn unsupported(
    diagnostics: Vec<String>,
    endpoint: Option<String>,
    model: Option<String>,
) -> AgentCapabilities {
    AgentCapabilities {
        driver: AgentDriverKind::Local,
        available: true,
        version: None,
        model_identity: model,
        supports_structured_tool_calls: false,
        supports_non_interactive: true,
        isolation: IsolationStrength::WorkingDirectoryRestricted,
        honours_write_restriction: true,
        context_window_tokens: None,
        endpoint,
        requires_paid_account: false,
        demonstration_only: false,
        diagnostics,
    }
}

fn unavailable(diagnostics: Vec<String>, endpoint: Option<String>) -> AgentCapabilities {
    AgentCapabilities {
        driver: AgentDriverKind::Local,
        available: false,
        version: None,
        model_identity: None,
        supports_structured_tool_calls: false,
        supports_non_interactive: true,
        isolation: IsolationStrength::WorkingDirectoryRestricted,
        honours_write_restriction: true,
        context_window_tokens: None,
        endpoint,
        requires_paid_account: false,
        demonstration_only: false,
        diagnostics,
    }
}

#[derive(Debug, Deserialize)]
struct ModelListing {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[async_trait]
impl AgentDriver for LocalModelAgentDriver {
    fn kind(&self) -> AgentDriverKind {
        AgentDriverKind::Local
    }

    async fn capabilities(&self) -> ApplicationResult<AgentCapabilities> {
        let capabilities = self
            .capabilities
            .get_or_init(|| async { self.resolve_capabilities().await })
            .await;
        Ok(capabilities.clone())
    }

    async fn invoke(&self, invocation: AgentInvocation) -> ApplicationResult<AgentOutcome> {
        let started = Instant::now();
        let capabilities = self.capabilities().await?;
        let model = capabilities
            .model_identity
            .clone()
            .ok_or_else(|| ApplicationError::Agent("no model is available".to_string()))?;

        let executor = ToolExecutor::new(
            invocation.worktree.clone(),
            invocation.tool_policy.clone(),
            invocation.commands.clone(),
            Arc::clone(&self.processes),
            invocation.cancellation.clone(),
            invocation.output_budget_bytes,
        );
        let definitions = executor.definitions(&invocation.prompt.completion_schema);
        let tools: Vec<Value> = definitions
            .iter()
            .map(|definition| definition.to_openai_tool())
            .collect();

        let before = observe_changed_paths(&invocation.worktree)?;
        let mut messages = vec![
            json!({ "role": "system", "content": invocation.prompt.rendered }),
            json!({
                "role": "user",
                "content": format!(
                    "Begin. When the work is complete call `{COMPLETION_TOOL}` with the required structured result."
                )
            }),
        ];
        let mut tool_calls = Vec::new();
        let mut diagnostics = Vec::new();
        let mut structured_response = None;
        let mut exit_reason = AgentExitReason::TurnBudgetExhausted;
        let mut usage = AgentUsage::default();
        let mut transcript = String::new();
        let deadline = Duration::from_secs(u64::from(invocation.time_budget_seconds));
        let mut ordinal = 0u32;

        for turn in 0..invocation.turn_budget {
            if *invocation.cancellation.borrow() {
                exit_reason = AgentExitReason::Cancelled;
                break;
            }
            if started.elapsed() >= deadline {
                exit_reason = AgentExitReason::TimeBudgetExhausted;
                break;
            }
            let remaining = deadline.saturating_sub(started.elapsed());
            let body = json!({
                "model": model,
                "messages": messages,
                "tools": tools,
                "tool_choice": "auto",
                "temperature": 0,
                "stream": false
            });
            let response = match self.post_chat(body, remaining).await {
                Ok(response) => response,
                Err(error) => {
                    diagnostics.push(error.to_string());
                    exit_reason = AgentExitReason::DriverFailure;
                    break;
                }
            };
            record_usage(&response, &mut usage);
            let Some(message) = response.pointer("/choices/0/message") else {
                diagnostics.push("the model response contained no message".to_string());
                exit_reason = AgentExitReason::DriverFailure;
                break;
            };
            if let Some(content) = message.get("content").and_then(Value::as_str) {
                if !content.trim().is_empty() {
                    transcript.push_str(content);
                    transcript.push('\n');
                }
            }
            messages.push(message.clone());

            let calls = message
                .get("tool_calls")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if calls.is_empty() {
                messages.push(json!({
                    "role": "user",
                    "content": format!(
                        "Continue by calling a tool. Call `{COMPLETION_TOOL}` when the work is complete."
                    )
                }));
                debug!(turn, "the model returned no tool call");
                continue;
            }

            let mut completed = false;
            for call in calls {
                ordinal += 1;
                let call_id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call")
                    .to_string();
                let name = call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let raw_arguments = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}")
                    .to_string();
                let arguments: Value = serde_json::from_str(&raw_arguments).unwrap_or(json!({}));
                let call_started = Instant::now();
                let execution = executor.execute(&name, &arguments).await?;
                tool_calls.push(AgentToolCall {
                    ordinal,
                    tool: name.clone(),
                    arguments_digest: ContentDigest::of_str(&raw_arguments),
                    accepted: execution.accepted,
                    rejection_reason: execution.rejection_reason.clone(),
                    duration: DurationMs::from_millis(call_started.elapsed().as_millis() as u64),
                    summary: execution.summary.clone(),
                });
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "name": name,
                    "content": serde_json::to_string(&execution.result).unwrap_or_else(|_| "{}".to_string()),
                }));
                if let Some(completion) = execution.completion {
                    structured_response = Some(completion);
                    exit_reason = AgentExitReason::Completed;
                    completed = true;
                    break;
                }
                if ordinal >= invocation.tool_policy.maximum_tool_calls {
                    diagnostics.push("the tool call budget was exhausted".to_string());
                    exit_reason = AgentExitReason::TurnBudgetExhausted;
                    completed = true;
                    break;
                }
            }
            if completed {
                break;
            }
        }

        if exit_reason == AgentExitReason::Completed && structured_response.is_none() {
            exit_reason = AgentExitReason::DriverFailure;
            diagnostics.push("the completion tool returned no structured payload".to_string());
        }

        let after = observe_changed_paths(&invocation.worktree)?;
        let changed_paths = crate::agent::changes::difference(&before, &after);
        if invocation.role.is_read_only() && !changed_paths.is_empty() {
            warn!(
                changed = changed_paths.len(),
                "a read-only agent role changed files"
            );
        }

        Ok(AgentOutcome {
            exit_reason,
            model_identity: model,
            driver: AgentDriverKind::Local,
            tool_calls,
            usage,
            structured_response,
            stdout: transcript,
            stderr: diagnostics.join("\n"),
            changed_paths,
            duration: DurationMs::from_millis(started.elapsed().as_millis() as u64),
            diagnostics,
        })
    }
}

fn record_usage(response: &Value, usage: &mut AgentUsage) {
    if let Some(input) = response
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
    {
        usage.input_tokens = Some(usage.input_tokens.unwrap_or(0) + input);
    }
    if let Some(output) = response
        .pointer("/usage/completion_tokens")
        .and_then(Value::as_u64)
    {
        usage.output_tokens = Some(usage.output_tokens.unwrap_or(0) + output);
    }
}

pub fn enforce_network_policy(endpoint: &str, policy: NetworkPolicy) -> ApplicationResult<()> {
    let parsed = reqwest::Url::parse(endpoint).map_err(|error| {
        ApplicationError::InvalidConfiguration(format!(
            "the model endpoint `{endpoint}` is not a valid address: {error}"
        ))
    })?;
    match policy {
        NetworkPolicy::Disabled => Err(ApplicationError::InvalidConfiguration(format!(
            "the network policy is `{}`, so the model endpoint `{endpoint}` may not be contacted",
            policy.as_str()
        ))),
        NetworkPolicy::LoopbackOnly => {
            if is_loopback_host(&parsed) {
                Ok(())
            } else {
                Err(ApplicationError::InvalidConfiguration(format!(
                    "the network policy is `{}`, so the model endpoint must resolve to this machine, but `{endpoint}` does not. Set `agent.network` to `approved-endpoints` in your own user configuration to reach a remote model.",
                    policy.as_str()
                )))
            }
        }
        NetworkPolicy::ApprovedEndpoints => Ok(()),
    }
}

fn is_loopback_host(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let lowered = host
        .trim_matches(|character| character == '[' || character == ']')
        .to_ascii_lowercase();
    if lowered == "localhost" || lowered.ends_with(".localhost") {
        return true;
    }
    match lowered.parse::<std::net::IpAddr>() {
        Ok(address) => address.is_loopback(),
        Err(_) => false,
    }
}
