use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::error::ApplicationResult;
use heikas_application::ports::process::{ProcessOutcome, ProcessRunner};
use heikas_application::ports::quality::{
    GateArtifact, GateContext, TestGateOutput, TestGateRunner,
};
use heikas_domain::command::{CommandKind, CommandSpecification, ReportFormat};
use heikas_domain::identity::ContentDigest;
use heikas_domain::test_evidence::{
    CommandExecutionRecord, CommandOutcome, TestEvidence, TestFailureDetail,
};

use crate::quality::reports::{
    parse_cargo_test_json, parse_junit_xml, parse_lcov_coverage, TestSummary,
};

pub struct CommandTestGateRunner {
    processes: Arc<dyn ProcessRunner>,
}

impl CommandTestGateRunner {
    pub fn new(processes: Arc<dyn ProcessRunner>) -> Self {
        Self { processes }
    }
}

#[async_trait]
impl TestGateRunner for CommandTestGateRunner {
    async fn run_tests(
        &self,
        context: &GateContext,
        commands: &[CommandSpecification],
    ) -> ApplicationResult<TestGateOutput> {
        let mut evidence = TestEvidence::default();
        let mut artifacts = Vec::new();

        for specification in commands {
            if *context.cancellation.borrow() {
                evidence.commands.push(CommandExecutionRecord {
                    command_id: specification.id.clone(),
                    required: specification.required,
                    outcome: CommandOutcome::Cancelled,
                    exit_code: None,
                    duration: heikas_domain::clock::DurationMs::ZERO,
                    stdout_artifact: None,
                    stderr_artifact: None,
                    stdout_truncated: false,
                    stderr_truncated: false,
                    tests_total: None,
                    tests_failed: None,
                    tests_skipped: None,
                    failures: Vec::new(),
                    line_coverage_percent: None,
                    detail: Some("the run was cancelled before this command started".to_string()),
                });
                continue;
            }
            let request = crate::process::request_for_command(
                specification,
                &context.worktree,
                context.configuration.budgets.max_output_bytes_per_stream,
            )?;
            let outcome = self
                .processes
                .run(request, context.cancellation.clone())
                .await?;
            let (record, produced) =
                build_record(context, specification, &outcome, &mut artifacts)?;
            let _ = produced;
            evidence.commands.push(record);
        }

        evidence.recompute_totals();
        Ok(TestGateOutput {
            evidence,
            artifacts,
        })
    }
}

pub fn build_record(
    context: &GateContext,
    specification: &CommandSpecification,
    outcome: &ProcessOutcome,
    artifacts: &mut Vec<GateArtifact>,
) -> ApplicationResult<(CommandExecutionRecord, bool)> {
    let scope = context
        .candidate_id
        .as_ref()
        .map(|candidate| candidate.to_string())
        .unwrap_or_else(|| "integration".to_string());
    let stdout_label = format!("{}-{}-stdout", scope, specification.id);
    let stderr_label = format!("{}-{}-stderr", scope, specification.id);
    let base = evidence_relative_root(context);

    artifacts.push(GateArtifact {
        label: stdout_label,
        relative_path: format!("{base}/{}-stdout.log", specification.id),
        media_type: "text/plain".to_string(),
        bytes: outcome.stdout.clone(),
        truncated: outcome.stdout_truncated,
    });
    artifacts.push(GateArtifact {
        label: stderr_label,
        relative_path: format!("{base}/{}-stderr.log", specification.id),
        media_type: "text/plain".to_string(),
        bytes: outcome.stderr.clone(),
        truncated: outcome.stderr_truncated,
    });

    let mut summary = TestSummary::default();
    let mut coverage = None;
    let mut report_missing = false;

    match specification.report_format {
        ReportFormat::None => {}
        ReportFormat::CargoTestJson => {
            summary = parse_cargo_test_json(&outcome.stdout_text());
        }
        ReportFormat::JUnitXml => match read_report(&context.worktree, specification)? {
            Some(contents) => {
                summary = parse_junit_xml(&contents)?;
                artifacts.push(GateArtifact {
                    label: format!("{scope}-{}-junit", specification.id),
                    relative_path: format!("{base}/{}-junit.xml", specification.id),
                    media_type: "application/xml".to_string(),
                    bytes: contents.into_bytes(),
                    truncated: false,
                });
            }
            None => report_missing = true,
        },
        ReportFormat::Lcov => match read_report(&context.worktree, specification)? {
            Some(contents) => {
                coverage = parse_lcov_coverage(&contents);
                artifacts.push(GateArtifact {
                    label: format!("{scope}-{}-lcov", specification.id),
                    relative_path: format!("{base}/{}-coverage.info", specification.id),
                    media_type: "text/plain".to_string(),
                    bytes: contents.into_bytes(),
                    truncated: false,
                });
            }
            None => report_missing = true,
        },
        ReportFormat::Sarif | ReportFormat::Text => {
            match read_report(&context.worktree, specification)? {
                Some(contents) => {
                    artifacts.push(GateArtifact {
                        label: format!("{scope}-{}-report", specification.id),
                        relative_path: format!("{base}/{}-report", specification.id),
                        media_type: if specification.report_format == ReportFormat::Sarif {
                            "application/sarif+json".to_string()
                        } else {
                            "text/plain".to_string()
                        },
                        bytes: contents.into_bytes(),
                        truncated: false,
                    });
                }
                None => report_missing = true,
            }
        }
    }

    if specification.kind == CommandKind::Coverage && coverage.is_none() {
        coverage = parse_lcov_coverage(&outcome.stdout_text());
    }

    let reports_executed_tests = specification.report_format != ReportFormat::None
        && specification.kind == CommandKind::Test;
    let executed_nothing = reports_executed_tests && summary.total == 0;

    let command_outcome = if outcome.cancelled {
        CommandOutcome::Cancelled
    } else if outcome.timed_out {
        CommandOutcome::TimedOut
    } else if specification.required && (report_missing || executed_nothing) {
        CommandOutcome::ReportMissing
    } else if specification.is_success(outcome.exit_code) && summary.failed == 0 {
        CommandOutcome::Passed
    } else {
        CommandOutcome::Failed
    };

    let detail = if command_outcome == CommandOutcome::Failed {
        Some(failure_detail(outcome, &summary))
    } else if command_outcome == CommandOutcome::ReportMissing {
        Some(if executed_nothing {
            "the required test command reported no executed tests, so it is no evidence that the change is correct".to_string()
        } else {
            format!(
                "the required report `{}` was not produced",
                specification.report_path.clone().unwrap_or_default()
            )
        })
    } else {
        None
    };

    Ok((
        CommandExecutionRecord {
            command_id: specification.id.clone(),
            required: specification.required,
            outcome: command_outcome,
            exit_code: outcome.exit_code,
            duration: outcome.duration,
            stdout_artifact: Some(ContentDigest::of_bytes(&outcome.stdout)),
            stderr_artifact: Some(ContentDigest::of_bytes(&outcome.stderr)),
            stdout_truncated: outcome.stdout_truncated,
            stderr_truncated: outcome.stderr_truncated,
            tests_total: if summary.total > 0 {
                Some(summary.total)
            } else {
                None
            },
            tests_failed: if summary.total > 0 {
                Some(summary.failed)
            } else {
                None
            },
            tests_skipped: if summary.total > 0 {
                Some(summary.skipped)
            } else {
                None
            },
            failures: limit_failures(summary.failures),
            line_coverage_percent: coverage,
            detail,
        },
        !report_missing,
    ))
}

fn limit_failures(failures: Vec<TestFailureDetail>) -> Vec<TestFailureDetail> {
    failures.into_iter().take(50).collect()
}

fn failure_detail(outcome: &ProcessOutcome, summary: &TestSummary) -> String {
    if summary.failed > 0 {
        return format!("{} of {} tests failed", summary.failed, summary.total);
    }
    let stderr = outcome.stderr_text();
    let tail: String = stderr
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    if tail.trim().is_empty() {
        let stdout = outcome.stdout_text();
        let stdout_tail: String = stdout
            .lines()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        if stdout_tail.trim().is_empty() {
            return match outcome.exit_code {
                Some(code) => format!("the command exited with status {code}"),
                None => "the command failed without an exit status".to_string(),
            };
        }
        return stdout_tail;
    }
    tail
}

pub fn evidence_relative_root(context: &GateContext) -> String {
    match &context.candidate_id {
        Some(candidate) => format!("candidates/{candidate}/reports"),
        None => "integration/reports".to_string(),
    }
}

fn read_report(
    worktree: &Path,
    specification: &CommandSpecification,
) -> ApplicationResult<Option<String>> {
    let Some(relative) = specification.report_path.as_deref() else {
        return Ok(None);
    };
    let root = crate::paths::confined_working_directory(
        worktree,
        specification.working_subdirectory.as_deref(),
    )?;
    let Some(bytes) = crate::paths::read_confined_file(
        &root,
        relative,
        crate::paths::MAXIMUM_REPOSITORY_REPORT_BYTES,
    )?
    else {
        return Ok(None);
    };
    Ok(String::from_utf8(bytes).ok())
}
