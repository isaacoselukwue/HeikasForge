use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use heikas_application::configuration::{
    AgentConfiguration, EffectiveConfiguration, GitConfiguration, QualityConfiguration,
    RedactionConfiguration, CONFIGURATION_SCHEMA_VERSION,
};
use heikas_application::ports::process::ProcessRunner;
use heikas_application::ports::quality::{GateContext, TestGateRunner};
use heikas_domain::budget::RunBudgets;
use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{
    CommandCatalogue, CommandId, CommandKind, CommandSpecification, ReportFormat,
};
use heikas_domain::identity::CommitHash;
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;
use heikas_domain::test_evidence::CommandOutcome;
use heikas_infrastructure::process::SupervisedProcessRunner;
use heikas_infrastructure::quality::CommandTestGateRunner;
use heikas_infrastructure::system::UuidIdentifierFactory;
use tempfile::TempDir;
use tokio::sync::watch;

fn configuration(repository: &Path) -> EffectiveConfiguration {
    EffectiveConfiguration {
        schema_version: CONFIGURATION_SCHEMA_VERSION,
        repository_path: repository.to_path_buf(),
        budgets: RunBudgets::default(),
        commit_policy: CommitPolicy::Manual,
        agent: AgentConfiguration::default(),
        quality: QualityConfiguration::default(),
        git: GitConfiguration::default(),
        commands: CommandCatalogue::default(),
        path_policy: PathPolicy::default(),
        redaction: RedactionConfiguration::default(),
        retry: RetryPolicy::default(),
        timeouts: NodeTimeouts::default(),
        environment_allowlist: Vec::new(),
        demonstration_mode: true,
        repository_trust: Default::default(),
        command_source: Default::default(),
        detection_notes: Vec::new(),
    }
}

fn reporting_command(script: &str, report_format: ReportFormat) -> CommandSpecification {
    CommandSpecification {
        id: CommandId::from_str("suite").expect("a command identifier"),
        kind: CommandKind::Test,
        program: python_interpreter(),
        args: vec!["-c".to_string(), script.to_string()],
        working_subdirectory: None,
        timeout: TimeoutSeconds::clamped(300, 600),
        required: true,
        report_format,
        report_path: None,
        environment: Vec::new(),
        success_exit_codes: vec![0],
    }
}

fn emitting(lines: &[&str], exit_code: i32) -> String {
    let body = lines
        .iter()
        .map(|line| format!("print({:?})", line))
        .collect::<Vec<_>>()
        .join("; ");
    format!("import sys; {body}; sys.exit({exit_code})")
}

fn python_interpreter() -> String {
    for candidate in ["python3", "python"] {
        if std::process::Command::new(candidate)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return candidate.to_string();
        }
    }
    panic!("no Python interpreter was found on the executable search path");
}

async fn run_gate(
    worktree: &Path,
    command: CommandSpecification,
) -> heikas_domain::test_evidence::CommandExecutionRecord {
    let (_sender, cancellation) = watch::channel(false);
    let processes: Arc<dyn ProcessRunner> = Arc::new(SupervisedProcessRunner::new(vec![
        "PATH".to_string(),
        "HOME".to_string(),
    ]));
    let runner = CommandTestGateRunner::new(processes);
    let context = GateContext {
        run_id: {
            use heikas_application::ports::clock::IdentifierFactory;
            UuidIdentifierFactory.new_run_id()
        },
        candidate_id: None,
        worktree: worktree.to_path_buf(),
        repository: worktree.to_path_buf(),
        baseline: CommitHash::from_str("0000000000000000000000000000000000000000")
            .expect("a commit hash"),
        changed_paths: Vec::new(),
        plan_expected_files: Vec::new(),
        configuration: configuration(worktree),
        cancellation,
    };
    let output = runner
        .run_tests(&context, std::slice::from_ref(&command))
        .await
        .expect("the gate runs");
    output
        .evidence
        .commands
        .into_iter()
        .next()
        .expect("one command record")
}

#[tokio::test]
async fn a_suite_that_runs_no_tests_is_not_recorded_as_passing() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(
        &emitting(&["", "no tests ran in 0.00s"], 0),
        ReportFormat::PytestText,
    );

    let record = run_gate(directory.path(), command).await;
    assert_ne!(
        record.outcome,
        CommandOutcome::Passed,
        "a gate that executed nothing must never pass, record: {record:?}"
    );
    assert_eq!(record.tests_total, Some(0));
    let detail = record.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("executed no tests"),
        "the record must say why, detail: `{detail}`"
    );
}

#[tokio::test]
async fn a_suite_in_which_every_test_is_skipped_does_not_pass() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(
        &emitting(&["ss", "2 skipped in 0.01s"], 0),
        ReportFormat::PytestText,
    );

    let record = run_gate(directory.path(), command).await;
    assert_ne!(
        record.outcome,
        CommandOutcome::Passed,
        "skipping every test leaves the change unvalidated, record: {record:?}"
    );
    assert_eq!(record.tests_total, Some(2));
    assert_eq!(record.tests_skipped, Some(2));
}

#[tokio::test]
async fn an_ignored_cargo_suite_does_not_pass() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(
        &emitting(
            &["test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s"],
            0,
        ),
        ReportFormat::CargoTestText,
    );

    let record = run_gate(directory.path(), command).await;
    assert_ne!(
        record.outcome,
        CommandOutcome::Passed,
        "ignoring every test leaves the change unvalidated, record: {record:?}"
    );
}

#[tokio::test]
async fn a_suite_that_runs_tests_passes_and_reports_the_count() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(
        &emitting(&["..", "2 passed in 0.05s"], 0),
        ReportFormat::PytestText,
    );

    let record = run_gate(directory.path(), command).await;
    assert_eq!(
        record.outcome,
        CommandOutcome::Passed,
        "a real suite must still pass, record: {record:?}"
    );
    assert_eq!(record.tests_total, Some(2));
    assert_eq!(record.tests_failed, Some(0));
}

#[tokio::test]
async fn a_failing_suite_reports_the_failure_and_the_count() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(
        &emitting(&["F", "1 failed in 0.05s"], 1),
        ReportFormat::PytestText,
    );

    let record = run_gate(directory.path(), command).await;
    assert_eq!(record.outcome, CommandOutcome::Failed);
    assert_eq!(record.tests_total, Some(1));
    assert_eq!(record.tests_failed, Some(1));
}

#[tokio::test]
async fn a_suite_that_fails_to_build_is_reported_as_a_failure_not_as_an_empty_suite() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(
        &emitting(&["error: could not compile `example`"], 101),
        ReportFormat::CargoTestText,
    );

    let record = run_gate(directory.path(), command).await;
    assert_eq!(
        record.outcome,
        CommandOutcome::Failed,
        "a suite that could not be built must read as a failure, record: {record:?}"
    );
}

#[tokio::test]
async fn a_command_that_declares_no_report_still_passes_on_its_exit_status() {
    let directory = TempDir::new().expect("a temporary worktree");
    let command = reporting_command(&emitting(&["done"], 0), ReportFormat::None);

    let record = run_gate(directory.path(), command).await;
    assert_eq!(
        record.outcome,
        CommandOutcome::Passed,
        "the new rule must only apply where an executed count is actually observable"
    );
}
