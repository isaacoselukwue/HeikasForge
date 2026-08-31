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

fn python_command() -> CommandSpecification {
    CommandSpecification {
        id: CommandId::from_str("python-test").expect("a command identifier"),
        kind: CommandKind::Test,
        program: "python3".to_string(),
        args: vec!["-m".to_string(), "pytest".to_string(), "-q".to_string()],
        working_subdirectory: None,
        timeout: TimeoutSeconds::clamped(300, 600),
        required: true,
        report_format: ReportFormat::PytestText,
        report_path: None,
        environment: Vec::new(),
        success_exit_codes: vec![0],
    }
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
    std::fs::write(
        directory.path().join("app.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .expect("the module writes");

    let record = run_gate(directory.path(), python_command()).await;
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
async fn a_suite_that_runs_tests_passes_and_reports_the_count() {
    let directory = TempDir::new().expect("a temporary worktree");
    std::fs::create_dir_all(directory.path().join("tests")).expect("the directory creates");
    std::fs::write(
        directory.path().join("tests").join("test_app.py"),
        "def test_one():\n    assert 1 == 1\n\n\ndef test_two():\n    assert 2 == 2\n",
    )
    .expect("the suite writes");

    let record = run_gate(directory.path(), python_command()).await;
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
    std::fs::create_dir_all(directory.path().join("tests")).expect("the directory creates");
    std::fs::write(
        directory.path().join("tests").join("test_app.py"),
        "def test_one():\n    assert 1 == 2\n",
    )
    .expect("the suite writes");

    let record = run_gate(directory.path(), python_command()).await;
    assert_eq!(record.outcome, CommandOutcome::Failed);
    assert_eq!(record.tests_total, Some(1));
    assert_eq!(record.tests_failed, Some(1));
}

#[tokio::test]
async fn a_command_that_declares_no_report_still_passes_on_its_exit_status() {
    let directory = TempDir::new().expect("a temporary worktree");
    let mut command = python_command();
    command.report_format = ReportFormat::None;
    command.args = vec!["-c".to_string(), "print('done')".to_string()];

    let record = run_gate(directory.path(), command).await;
    assert_eq!(
        record.outcome,
        CommandOutcome::Passed,
        "the new rule must only apply where an executed count is actually observable"
    );
}

#[tokio::test]
async fn a_suite_that_fails_to_build_is_reported_as_a_failure_not_as_an_empty_suite() {
    let directory = TempDir::new().expect("a temporary worktree");
    std::fs::create_dir_all(directory.path().join("tests")).expect("the directory creates");
    std::fs::write(
        directory.path().join("tests").join("test_app.py"),
        "def test_one(:\n    this is not python\n",
    )
    .expect("the broken suite writes");

    let record = run_gate(directory.path(), python_command()).await;
    assert_eq!(
        record.outcome,
        CommandOutcome::Failed,
        "a suite that could not be collected must read as a failure, not as an empty suite"
    );
    let detail = record.detail.clone().unwrap_or_default();
    assert!(
        !detail.contains("executed no tests"),
        "a broken suite must not be described as having run nothing, detail: `{detail}`"
    );
}

#[tokio::test]
async fn a_suite_in_which_every_test_is_skipped_does_not_pass() {
    let directory = TempDir::new().expect("a temporary worktree");
    std::fs::create_dir_all(directory.path().join("tests")).expect("the directory creates");
    std::fs::write(
        directory.path().join("tests").join("test_app.py"),
        "import pytest\n\n\n@pytest.mark.skip\ndef test_one():\n    assert False\n\n\n@pytest.mark.skip\ndef test_two():\n    assert False\n",
    )
    .expect("the suite writes");

    let record = run_gate(directory.path(), python_command()).await;
    assert_ne!(
        record.outcome,
        CommandOutcome::Passed,
        "skipping every test leaves the change unvalidated, record: {record:?}"
    );
}
