use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::configuration::{SonarMcpConfiguration, SonarScannerConfiguration};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::clock::Clock;
use heikas_application::ports::process::{ProcessRequest, ProcessRunner};
use heikas_application::ports::quality::{
    GateArtifact, GateContext, ReviewGateOutput, ReviewProvider,
};
use heikas_domain::review::{
    IssueCategory, IssueSeverity, QualityGateOutcome, ReviewIssue, ReviewMetrics, ReviewReport,
    REVIEW_REPORT_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::Value;

use crate::quality::test_runner::evidence_relative_root;

pub const SCANNER_PROVIDER: &str = "sonar-scanner";
pub const MCP_PROVIDER: &str = "sonar-mcp";

pub struct SonarScannerProvider {
    configuration: SonarScannerConfiguration,
    processes: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
}

impl SonarScannerProvider {
    pub fn new(
        configuration: SonarScannerConfiguration,
        processes: Arc<dyn ProcessRunner>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            configuration,
            processes,
            clock,
        }
    }
}

#[async_trait]
impl ReviewProvider for SonarScannerProvider {
    fn name(&self) -> &str {
        SCANNER_PROVIDER
    }

    fn required(&self) -> bool {
        self.configuration.enabled
    }

    fn advisory(&self) -> bool {
        false
    }

    async fn available(&self) -> ApplicationResult<bool> {
        if !self.configuration.enabled {
            return Ok(false);
        }
        Ok(self
            .processes
            .probe_executable(&self.configuration.program)
            .await?
            .is_some())
    }

    async fn review(&self, context: &GateContext) -> ApplicationResult<ReviewGateOutput> {
        let started_at = self.clock.now();
        let mut args = self.configuration.arguments.clone();
        args.push(format!("-Dsonar.host.url={}", self.configuration.host_url));
        if let Some(project_key) = &self.configuration.project_key {
            args.push(format!("-Dsonar.projectKey={project_key}"));
        }
        if self.configuration.wait_for_quality_gate {
            args.push("-Dsonar.qualitygate.wait=true".to_string());
        }
        args.push(format!(
            "-Dsonar.projectBaseDir={}",
            context.worktree.display()
        ));

        let mut environment = Vec::new();
        if let Some(name) = &self.configuration.token_environment_variable {
            if let Ok(value) = std::env::var(name) {
                environment.push(("SONAR_TOKEN".to_string(), value));
            }
        }

        let request = ProcessRequest {
            program: self.configuration.program.clone(),
            args,
            working_directory: context.worktree.clone(),
            environment,
            timeout_seconds: self.configuration.timeout.get(),
            max_output_bytes: context.configuration.budgets.max_output_bytes_per_stream,
            label: SCANNER_PROVIDER.to_string(),
        };
        let outcome = self
            .processes
            .run(request, context.cancellation.clone())
            .await?;
        let base = evidence_relative_root(context);
        let artifacts = vec![
            GateArtifact {
                label: format!("{SCANNER_PROVIDER}-stdout"),
                relative_path: format!("{base}/sonar-scanner-stdout.log"),
                media_type: "text/plain".to_string(),
                bytes: outcome.stdout.clone(),
                truncated: outcome.stdout_truncated,
            },
            GateArtifact {
                label: format!("{SCANNER_PROVIDER}-stderr"),
                relative_path: format!("{base}/sonar-scanner-stderr.log"),
                media_type: "text/plain".to_string(),
                bytes: outcome.stderr.clone(),
                truncated: outcome.stderr_truncated,
            },
        ];

        let passed = outcome.succeeded();
        let mut issues = Vec::new();
        if !passed {
            let message = format!(
                "the SonarQube scanner reported a failing quality gate with status {}",
                outcome
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            issues.push(ReviewIssue {
                provider: SCANNER_PROVIDER.to_string(),
                fingerprint: ReviewIssue::compute_fingerprint(
                    SCANNER_PROVIDER,
                    "quality-gate",
                    None,
                    &message,
                ),
                rule_id: "quality-gate".to_string(),
                category: IssueCategory::Maintainability,
                severity: IssueSeverity::Blocker,
                file: None,
                line: None,
                column: None,
                message,
                help_reference: Some(self.configuration.host_url.clone()),
                is_new: true,
            });
        }

        let finished_at = self.clock.now();
        Ok(ReviewGateOutput {
            report: ReviewReport {
                schema_version: REVIEW_REPORT_SCHEMA_VERSION,
                provider: SCANNER_PROVIDER.to_string(),
                required: true,
                advisory: false,
                passed,
                quality_gate: if passed {
                    QualityGateOutcome::Passed
                } else {
                    QualityGateOutcome::Failed
                },
                issues,
                metrics: ReviewMetrics::default(),
                artifacts: Vec::new(),
                started_at,
                finished_at,
                failure_summary: (!passed).then(|| outcome.stderr_text().trim().to_string()),
            },
            artifacts,
        })
    }
}

pub struct SonarMcpProvider {
    configuration: SonarMcpConfiguration,
    processes: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
}

impl SonarMcpProvider {
    pub fn new(
        configuration: SonarMcpConfiguration,
        processes: Arc<dyn ProcessRunner>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            configuration,
            processes,
            clock,
        }
    }
}

#[derive(Debug, Deserialize)]
struct McpToolEvidence {
    #[serde(default)]
    tool_calls: Vec<String>,
    #[serde(default)]
    quality_gate: Option<String>,
    #[serde(default)]
    issues: Vec<McpIssue>,
}

#[derive(Debug, Deserialize)]
struct McpIssue {
    rule: String,
    severity: String,
    message: String,
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    line: Option<u32>,
}

#[async_trait]
impl ReviewProvider for SonarMcpProvider {
    fn name(&self) -> &str {
        MCP_PROVIDER
    }

    fn required(&self) -> bool {
        self.configuration.enabled
    }

    fn advisory(&self) -> bool {
        false
    }

    async fn available(&self) -> ApplicationResult<bool> {
        if !self.configuration.enabled {
            return Ok(false);
        }
        Ok(self
            .processes
            .probe_executable(&self.configuration.program)
            .await?
            .is_some())
    }

    async fn review(&self, context: &GateContext) -> ApplicationResult<ReviewGateOutput> {
        let started_at = self.clock.now();
        let mut args = self.configuration.arguments.clone();
        if let Some(project_key) = &self.configuration.project_key {
            args.push("--project".to_string());
            args.push(project_key.clone());
        }
        let mut environment = Vec::new();
        if let Some(name) = &self.configuration.token_environment_variable {
            if let Ok(value) = std::env::var(name) {
                environment.push(("SONAR_TOKEN".to_string(), value));
            }
        }
        let request = ProcessRequest {
            program: self.configuration.program.clone(),
            args,
            working_directory: context.worktree.clone(),
            environment,
            timeout_seconds: self.configuration.timeout.get(),
            max_output_bytes: context.configuration.budgets.max_output_bytes_per_stream,
            label: MCP_PROVIDER.to_string(),
        };
        let outcome = self
            .processes
            .run(request, context.cancellation.clone())
            .await?;
        let base = evidence_relative_root(context);
        let artifacts = vec![GateArtifact {
            label: format!("{MCP_PROVIDER}-stdout"),
            relative_path: format!("{base}/sonar-mcp-stdout.json"),
            media_type: "application/json".to_string(),
            bytes: outcome.stdout.clone(),
            truncated: outcome.stdout_truncated,
        }];

        let evidence: Option<McpToolEvidence> = serde_json::from_str(outcome.stdout_text().trim()).ok();
        let Some(evidence) = evidence else {
            return Err(ApplicationError::QualityProvider(
                "the SonarQube MCP adapter produced no structured tool evidence".to_string(),
            ));
        };
        let required_tools = ["quality_gate", "issues", "security_hotspots"];
        let missing: Vec<&str> = required_tools
            .into_iter()
            .filter(|tool| !evidence.tool_calls.iter().any(|call| call.contains(tool)))
            .collect();
        if !missing.is_empty() {
            return Err(ApplicationError::QualityProvider(format!(
                "the SonarQube MCP adapter did not record the required tool calls: {}",
                missing.join(", ")
            )));
        }

        let passed = evidence
            .quality_gate
            .as_deref()
            .map(|gate| gate.eq_ignore_ascii_case("ok") || gate.eq_ignore_ascii_case("passed"))
            .unwrap_or(false);

        let issues: Vec<ReviewIssue> = evidence
            .issues
            .into_iter()
            .map(|issue| {
                let severity = map_severity(&issue.severity);
                ReviewIssue {
                    provider: MCP_PROVIDER.to_string(),
                    fingerprint: ReviewIssue::compute_fingerprint(
                        MCP_PROVIDER,
                        &issue.rule,
                        issue.component.as_deref(),
                        &issue.message,
                    ),
                    rule_id: issue.rule,
                    category: IssueCategory::Maintainability,
                    severity,
                    file: issue.component,
                    line: issue.line,
                    column: None,
                    message: issue.message,
                    help_reference: None,
                    is_new: true,
                }
            })
            .collect();

        let finished_at = self.clock.now();
        Ok(ReviewGateOutput {
            report: ReviewReport {
                schema_version: REVIEW_REPORT_SCHEMA_VERSION,
                provider: MCP_PROVIDER.to_string(),
                required: true,
                advisory: false,
                passed,
                quality_gate: if passed {
                    QualityGateOutcome::Passed
                } else {
                    QualityGateOutcome::Failed
                },
                issues,
                metrics: ReviewMetrics::default(),
                artifacts: Vec::new(),
                started_at,
                finished_at,
                failure_summary: (!passed).then(|| {
                    format!(
                        "the SonarQube quality gate reported `{}`",
                        evidence.quality_gate.unwrap_or_else(|| "unknown".to_string())
                    )
                }),
            },
            artifacts,
        })
    }
}

fn map_severity(value: &str) -> IssueSeverity {
    match value.to_ascii_uppercase().as_str() {
        "BLOCKER" => IssueSeverity::Blocker,
        "CRITICAL" => IssueSeverity::Critical,
        "MAJOR" => IssueSeverity::High,
        "MINOR" => IssueSeverity::Medium,
        "INFO" => IssueSeverity::Info,
        _ => IssueSeverity::Medium,
    }
}

pub fn value_as_string(value: &Value) -> String {
    value.as_str().map(str::to_string).unwrap_or_default()
}
