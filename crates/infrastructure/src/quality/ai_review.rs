use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::configuration::AiReviewConfiguration;
use heikas_application::error::ApplicationResult;
use heikas_application::ports::agent::{AgentDriver, AgentInvocation, AgentRole, ToolPolicy};
use heikas_application::ports::clock::Clock;
use heikas_application::ports::quality::{GateContext, ReviewGateOutput, ReviewProvider};
use heikas_application::prompt::{PromptFacts, PromptLibrary};
use heikas_domain::review::{
    IssueCategory, IssueSeverity, QualityGateOutcome, ReviewIssue, ReviewMetrics, ReviewReport,
    REVIEW_REPORT_SCHEMA_VERSION,
};
use serde_json::Value;
use std::str::FromStr;

pub const PROVIDER_NAME: &str = "ai-review";

pub struct AdvisoryAiReviewProvider {
    configuration: AiReviewConfiguration,
    agent: Arc<dyn AgentDriver>,
    clock: Arc<dyn Clock>,
}

impl AdvisoryAiReviewProvider {
    pub fn new(
        configuration: AiReviewConfiguration,
        agent: Arc<dyn AgentDriver>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            configuration,
            agent,
            clock,
        }
    }
}

#[async_trait]
impl ReviewProvider for AdvisoryAiReviewProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    fn required(&self) -> bool {
        false
    }

    fn advisory(&self) -> bool {
        true
    }

    async fn available(&self) -> ApplicationResult<bool> {
        if !self.configuration.enabled {
            return Ok(false);
        }
        Ok(self.agent.capabilities().await?.available)
    }

    async fn review(&self, context: &GateContext) -> ApplicationResult<ReviewGateOutput> {
        let started_at = self.clock.now();
        let facts = PromptFacts {
            task_title: "Advisory review of the candidate change".to_string(),
            task_body: String::new(),
            repository_summary: format!(
                "Changed paths: {}",
                context.changed_paths.join(", ")
            ),
            approved_plan_hash: None,
            approved_plan: None,
            strategy: None,
            strategy_emphasis: None,
            allowed_commands: Vec::new(),
            protected_paths: context.configuration.path_policy.protected_patterns.clone(),
            previous_evidence: context.changed_paths.clone(),
            expected_files: Vec::new(),
            attempt: 1,
        };
        let prompt = PromptLibrary::render(AgentRole::Reviewer, &facts)?;
        let invocation = AgentInvocation {
            run_id: context.run_id,
            candidate_id: context.candidate_id.clone(),
            role: AgentRole::Reviewer,
            strategy: None,
            worktree: context.worktree.clone(),
            prompt,
            tool_policy: ToolPolicy::read_only(context.configuration.path_policy.clone(), 60),
            commands: Vec::new(),
            environment_allowlist: context.configuration.environment_allowlist.clone(),
            network: context.configuration.agent.network,
            time_budget_seconds: context.configuration.agent.timeout.get().min(600),
            turn_budget: 12,
            output_budget_bytes: 262_144,
            cancellation: context.cancellation.clone(),
        };
        let outcome = self.agent.invoke(invocation).await?;
        let issues = outcome
            .structured_response
            .as_ref()
            .map(|response| parse_findings(response, &self.configuration.gate_rules))
            .unwrap_or_default();

        let gated = issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Blocker);
        let finished_at = self.clock.now();

        Ok(ReviewGateOutput {
            report: ReviewReport {
                schema_version: REVIEW_REPORT_SCHEMA_VERSION,
                provider: PROVIDER_NAME.to_string(),
                required: false,
                advisory: true,
                passed: !gated,
                quality_gate: if gated {
                    QualityGateOutcome::Failed
                } else {
                    QualityGateOutcome::NotApplicable
                },
                issues,
                metrics: ReviewMetrics::default(),
                artifacts: Vec::new(),
                started_at,
                finished_at,
                failure_summary: gated.then(|| {
                    "a configured deterministic rule converted an advisory finding into a gate"
                        .to_string()
                }),
            },
            artifacts: Vec::new(),
        })
    }
}

fn parse_findings(response: &Value, gate_rules: &[String]) -> Vec<ReviewIssue> {
    let Some(findings) = response.get("findings").and_then(Value::as_array) else {
        return Vec::new();
    };
    findings
        .iter()
        .filter_map(|finding| {
            let rule_id = finding.get("rule_id")?.as_str()?.to_string();
            let message = finding.get("message")?.as_str()?.to_string();
            let severity = finding
                .get("severity")
                .and_then(Value::as_str)
                .and_then(|value| IssueSeverity::from_str(value).ok())
                .unwrap_or(IssueSeverity::Low);
            let category = finding
                .get("category")
                .and_then(Value::as_str)
                .and_then(|value| IssueCategory::from_str(value).ok())
                .unwrap_or(IssueCategory::Maintainability);
            let file = finding
                .get("file")
                .and_then(Value::as_str)
                .map(str::to_string);
            let line = finding
                .get("line")
                .and_then(Value::as_u64)
                .map(|value| value as u32);
            let gated = gate_rules.iter().any(|rule| rule == &rule_id);
            Some(ReviewIssue {
                provider: PROVIDER_NAME.to_string(),
                fingerprint: ReviewIssue::compute_fingerprint(
                    PROVIDER_NAME,
                    &rule_id,
                    file.as_deref(),
                    &message,
                ),
                rule_id,
                category,
                severity: if gated {
                    IssueSeverity::Blocker
                } else {
                    severity.min(IssueSeverity::Medium)
                },
                file,
                line,
                column: None,
                message,
                help_reference: None,
                is_new: true,
            })
        })
        .collect()
}
