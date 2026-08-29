use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::error::ApplicationResult;
use heikas_application::ports::clock::Clock;
use heikas_application::ports::git::GitService;
use heikas_application::ports::process::{ProcessRequest, ProcessRunner};
use heikas_application::ports::quality::{
    GateArtifact, GateContext, ReviewGateOutput, ReviewProvider,
};
use heikas_domain::command::{CommandSpecification, ReportFormat};
use heikas_domain::review::{
    IssueCategory, IssueSeverity, QualityGateOutcome, ReviewArtifactReference, ReviewIssue,
    ReviewMetrics, ReviewReport, REVIEW_REPORT_SCHEMA_VERSION,
};

use crate::quality::integrity;
use crate::quality::reports::parse_sarif;
use crate::quality::test_runner::evidence_relative_root;

pub const PROVIDER_NAME: &str = "local";

pub struct LocalQualityProvider {
    processes: Arc<dyn ProcessRunner>,
    git: Arc<dyn GitService>,
    clock: Arc<dyn Clock>,
}

impl LocalQualityProvider {
    pub fn new(
        processes: Arc<dyn ProcessRunner>,
        git: Arc<dyn GitService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            processes,
            git,
            clock,
        }
    }

    async fn run_command(
        &self,
        context: &GateContext,
        specification: &CommandSpecification,
        issues: &mut Vec<ReviewIssue>,
        artifacts: &mut Vec<GateArtifact>,
    ) -> ApplicationResult<bool> {
        let request = ProcessRequest::from_specification(
            specification,
            &context.worktree,
            context.configuration.budgets.max_output_bytes_per_stream,
        );
        let outcome = self
            .processes
            .run(request, context.cancellation.clone())
            .await?;
        let base = evidence_relative_root(context);
        artifacts.push(GateArtifact {
            label: format!("{}-{}-stdout", PROVIDER_NAME, specification.id),
            relative_path: format!("{base}/review-{}-stdout.log", specification.id),
            media_type: "text/plain".to_string(),
            bytes: outcome.stdout.clone(),
            truncated: outcome.stdout_truncated,
        });
        artifacts.push(GateArtifact {
            label: format!("{}-{}-stderr", PROVIDER_NAME, specification.id),
            relative_path: format!("{base}/review-{}-stderr.log", specification.id),
            media_type: "text/plain".to_string(),
            bytes: outcome.stderr.clone(),
            truncated: outcome.stderr_truncated,
        });

        let mut report_missing = false;
        if specification.report_format == ReportFormat::Sarif {
            match specification
                .report_path
                .as_ref()
                .and_then(|relative| std::fs::read_to_string(context.worktree.join(relative)).ok())
            {
                Some(contents) => {
                    let parsed = parse_sarif(&contents, &specification.id.to_string())?;
                    issues.extend(parsed.into_iter().map(|mut issue| {
                        issue.category = specification.kind.default_issue_category();
                        issue
                    }));
                    artifacts.push(GateArtifact {
                        label: format!("{}-{}-sarif", PROVIDER_NAME, specification.id),
                        relative_path: format!("{base}/review-{}.sarif", specification.id),
                        media_type: "application/sarif+json".to_string(),
                        bytes: contents.into_bytes(),
                        truncated: false,
                    });
                }
                None => report_missing = true,
            }
        }

        let succeeded = specification.is_success(outcome.exit_code)
            && !outcome.timed_out
            && !outcome.cancelled
            && !(report_missing && specification.required);

        if !succeeded {
            let severity = if specification.required {
                IssueSeverity::Blocker
            } else {
                IssueSeverity::Medium
            };
            let message = if report_missing {
                format!(
                    "the command `{}` did not produce the required report `{}`",
                    specification.id,
                    specification.report_path.clone().unwrap_or_default()
                )
            } else if outcome.timed_out {
                format!("the command `{}` exceeded its timeout", specification.id)
            } else {
                format!(
                    "the command `{}` failed with status {}",
                    specification.id,
                    outcome
                        .exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                )
            };
            let rule_id = format!("command.{}", specification.id);
            issues.push(ReviewIssue {
                provider: PROVIDER_NAME.to_string(),
                fingerprint: ReviewIssue::compute_fingerprint(
                    PROVIDER_NAME,
                    &rule_id,
                    None,
                    &message,
                ),
                rule_id,
                category: specification.kind.default_issue_category(),
                severity,
                file: None,
                line: None,
                column: None,
                message,
                help_reference: None,
                is_new: true,
            });
        }
        Ok(succeeded)
    }
}

#[async_trait]
impl ReviewProvider for LocalQualityProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    fn required(&self) -> bool {
        true
    }

    fn advisory(&self) -> bool {
        false
    }

    async fn available(&self) -> ApplicationResult<bool> {
        Ok(true)
    }

    async fn review(&self, context: &GateContext) -> ApplicationResult<ReviewGateOutput> {
        let started_at = self.clock.now();
        let mut issues = Vec::new();
        let mut artifacts = Vec::new();
        let mut all_required_passed = true;

        let commands: Vec<CommandSpecification> = context
            .configuration
            .commands
            .review_phase()
            .into_iter()
            .cloned()
            .collect();
        for specification in &commands {
            let passed = self
                .run_command(context, specification, &mut issues, &mut artifacts)
                .await?;
            if specification.required && !passed {
                all_required_passed = false;
            }
        }

        let integrity_issues = integrity::evaluate(context, &self.git).await?;
        let integrity_blocked = integrity_issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Blocker);
        issues.extend(integrity_issues);
        if integrity_blocked {
            all_required_passed = false;
        }

        let required_kinds = context.configuration.required_review_kinds();
        for kind in required_kinds {
            if !commands.iter().any(|command| command.kind == kind) {
                let rule_id = format!("missing.{}", kind.as_str());
                let message = format!(
                    "the {} quality profile requires a `{}` command but none is configured",
                    context.configuration.quality.profile.as_str(),
                    kind.as_str()
                );
                issues.push(ReviewIssue {
                    provider: PROVIDER_NAME.to_string(),
                    fingerprint: ReviewIssue::compute_fingerprint(
                        PROVIDER_NAME,
                        &rule_id,
                        None,
                        &message,
                    ),
                    rule_id,
                    category: IssueCategory::Policy,
                    severity: IssueSeverity::Blocker,
                    file: None,
                    line: None,
                    column: None,
                    message,
                    help_reference: None,
                    is_new: true,
                });
                all_required_passed = false;
            }
        }

        let finished_at = self.clock.now();
        let failure_summary = if all_required_passed {
            None
        } else {
            Some(
                issues
                    .iter()
                    .filter(|issue| issue.severity == IssueSeverity::Blocker)
                    .map(|issue| issue.message.clone())
                    .take(5)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        };

        let report = ReviewReport {
            schema_version: REVIEW_REPORT_SCHEMA_VERSION,
            provider: PROVIDER_NAME.to_string(),
            required: true,
            advisory: false,
            passed: all_required_passed,
            quality_gate: if all_required_passed {
                QualityGateOutcome::Passed
            } else {
                QualityGateOutcome::Failed
            },
            issues,
            metrics: ReviewMetrics {
                line_coverage_percent: None,
                branch_coverage_percent: None,
                changed_lines: None,
                changed_files: Some(context.changed_paths.len() as u32),
                analysed_files: Some(context.changed_paths.len() as u32),
                duplicated_lines: None,
            },
            artifacts: artifacts
                .iter()
                .map(|artifact| ReviewArtifactReference {
                    label: artifact.label.clone(),
                    relative_path: artifact.relative_path.clone(),
                    media_type: artifact.media_type.clone(),
                    digest: heikas_domain::identity::ContentDigest::of_bytes(&artifact.bytes),
                    byte_length: artifact.bytes.len() as u64,
                })
                .collect(),
            started_at,
            finished_at,
            failure_summary,
        };

        Ok(ReviewGateOutput { report, artifacts })
    }
}

pub fn coverage_issue(measured: Option<f64>, required: Option<f64>) -> Option<ReviewIssue> {
    let required = required?;
    let message = match measured {
        Some(value) if value + f64::EPSILON < required => format!(
            "line coverage {value:.2}% is below the required {required:.2}%"
        ),
        Some(_) => return None,
        None => format!("line coverage was not measured but {required:.2}% is required"),
    };
    Some(ReviewIssue {
        provider: PROVIDER_NAME.to_string(),
        fingerprint: ReviewIssue::compute_fingerprint(
            PROVIDER_NAME,
            "coverage.threshold",
            None,
            &message,
        ),
        rule_id: "coverage.threshold".to_string(),
        category: IssueCategory::Coverage,
        severity: IssueSeverity::Blocker,
        file: None,
        line: None,
        column: None,
        message,
        help_reference: None,
        is_new: true,
    })
}
