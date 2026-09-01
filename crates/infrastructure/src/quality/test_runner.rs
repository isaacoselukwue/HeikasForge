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
    parse_cargo_test_json, parse_cargo_test_summary, parse_ctest_summary, parse_go_test_json,
    parse_junit_xml, parse_lcov_coverage, parse_node_test_summary, parse_pytest_summary,
    TestSummary,
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
        ReportFormat::CargoTestText => {
            summary = parse_cargo_test_summary(&outcome.stdout_text());
        }
        ReportFormat::GoTestJson => {
            summary = parse_go_test_json(&outcome.stdout_text());
        }
        ReportFormat::PytestText => {
            summary = parse_pytest_summary(&outcome.stdout_text());
        }
        ReportFormat::NodeTestText => {
            summary = parse_node_test_summary(&outcome.stdout_text());
        }
        ReportFormat::CTestText => {
            summary = parse_ctest_summary(&outcome.stdout_text());
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

    let counted = specification.report_format.counts_executed_tests() || summary.total > 0;
    let reports_executed_tests = specification.report_format.counts_executed_tests()
        && specification.kind == CommandKind::Test;
    let executed = summary.total.saturating_sub(summary.skipped);
    let executed_nothing = reports_executed_tests && executed == 0;

    let command_outcome = if outcome.cancelled {
        CommandOutcome::Cancelled
    } else if outcome.timed_out {
        CommandOutcome::TimedOut
    } else if !specification.is_success(outcome.exit_code) || summary.failed > 0 {
        CommandOutcome::Failed
    } else if specification.required && (report_missing || executed_nothing) {
        CommandOutcome::ReportMissing
    } else {
        CommandOutcome::Passed
    };

    const NO_EXECUTED_TESTS: &str = "the test command executed no tests, counting skipped and ignored tests as not executed, so it is no evidence that the change is correct";
    let detail = match command_outcome {
        CommandOutcome::Failed if executed_nothing => Some(format!(
            "{NO_EXECUTED_TESTS}. {}",
            failure_detail(outcome, &summary)
        )),
        CommandOutcome::Failed => Some(failure_detail(outcome, &summary)),
        CommandOutcome::ReportMissing if executed_nothing => Some(NO_EXECUTED_TESTS.to_string()),
        CommandOutcome::ReportMissing => Some(format!(
            "the required report `{}` was not produced",
            specification.report_path.clone().unwrap_or_default()
        )),
        _ => None,
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
            tests_total: counted.then_some(summary.total),
            tests_failed: counted.then_some(summary.failed),
            tests_skipped: counted.then_some(summary.skipped),
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
