use std::sync::Arc;
use std::time::Duration;

use heikas_application::ports::process::{ProcessRequest, ProcessRunner};
use heikas_infrastructure::process::SupervisedProcessRunner;
use tempfile::TempDir;
use tokio::sync::watch;

fn runner() -> SupervisedProcessRunner {
    SupervisedProcessRunner::new(Vec::new())
}

fn python() -> &'static str {
    heikas_fixture_harness::python_interpreter()
}

fn request(program: &str, arguments: &[&str], directory: &std::path::Path) -> ProcessRequest {
    ProcessRequest {
        program: program.to_string(),
        args: arguments.iter().map(|value| (*value).to_string()).collect(),
        working_directory: directory.to_path_buf(),
        environment: Vec::new(),
        timeout_seconds: 30,
        max_output_bytes: 65_536,
        label: "test".to_string(),
    }
}

#[tokio::test]
async fn a_successful_command_reports_its_streams_and_status() {
    let directory = TempDir::new().expect("a temporary directory");
    let (_sender, receiver) = watch::channel(false);
    let outcome = runner()
        .run(
            request(
                python(),
                &[
                    "-c",
                    "import sys; print('out'); print('err', file=sys.stderr)",
                ],
                directory.path(),
            ),
            receiver,
        )
        .await
        .expect("the command runs");
    assert!(outcome.succeeded());
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.stdout_text().contains("out"));
    assert!(outcome.stderr_text().contains("err"));
    assert!(!outcome.timed_out);
    assert!(!outcome.cancelled);
}

#[tokio::test]
async fn a_failing_command_reports_its_exit_status() {
    let directory = TempDir::new().expect("a temporary directory");
    let (_sender, receiver) = watch::channel(false);
    let outcome = runner()
        .run(
            request(
                python(),
                &["-c", "import sys; sys.exit(7)"],
                directory.path(),
            ),
            receiver,
        )
        .await
        .expect("the command runs");
    assert_eq!(outcome.exit_code, Some(7));
    assert!(!outcome.succeeded());
}

#[tokio::test]
async fn the_environment_is_not_inherited_beyond_the_allowlist() {
    let directory = TempDir::new().expect("a temporary directory");
    std::env::set_var("HEIKAS_TEST_SECRET_VALUE", "must-not-leak");
    let (_sender, receiver) = watch::channel(false);
    let outcome = runner()
        .run(
            request(
                python(),
                &[
                    "-c",
                    "import os; print(os.environ.get('HEIKAS_TEST_SECRET_VALUE', 'absent'))",
                ],
                directory.path(),
            ),
            receiver,
        )
        .await
        .expect("the command runs");
    assert!(
        outcome.stdout_text().contains("absent"),
        "the parent environment must not be inherited blindly"
    );
}

#[tokio::test]
async fn an_explicit_environment_entry_reaches_the_child() {
    let directory = TempDir::new().expect("a temporary directory");
    let (_sender, receiver) = watch::channel(false);
    let mut specification = request(
        python(),
        &[
            "-c",
            "import os; print(os.environ.get('HEIKAS_EXPLICIT', 'absent'))",
        ],
        directory.path(),
    );
    specification
        .environment
        .push(("HEIKAS_EXPLICIT".to_string(), "present".to_string()));
    let outcome = runner()
        .run(specification, receiver)
        .await
        .expect("the command runs");
    assert!(outcome.stdout_text().contains("present"));
}

#[tokio::test]
async fn a_command_that_exceeds_its_timeout_is_terminated() {
    let directory = TempDir::new().expect("a temporary directory");
    let (_sender, receiver) = watch::channel(false);
    let mut specification = request(
        python(),
        &["-c", "import time; time.sleep(60)"],
        directory.path(),
    );
    specification.timeout_seconds = 1;
    let outcome = runner()
        .run(specification, receiver)
        .await
        .expect("the command is supervised");
    assert!(
        outcome.timed_out,
        "the command must be recorded as timed out"
    );
    assert!(outcome.duration.millis() < 20_000);
}

#[tokio::test]
async fn cancellation_terminates_a_running_command() {
    let directory = TempDir::new().expect("a temporary directory");
    let (sender, receiver) = watch::channel(false);
    let specification = request(
        python(),
        &["-c", "import time; time.sleep(60)"],
        directory.path(),
    );
    let handle = tokio::spawn(async move { runner().run(specification, receiver).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    sender
        .send(true)
        .expect("the cancellation signal is delivered");
    let outcome = handle
        .await
        .expect("the task joins")
        .expect("the command is supervised");
    assert!(
        outcome.cancelled,
        "the command must be recorded as cancelled"
    );
}

#[tokio::test]
async fn no_child_process_survives_a_timeout() {
    let directory = TempDir::new().expect("a temporary directory");
    let marker = directory.path().join("child-alive.txt");
    let script = format!(
        "import os, subprocess, sys, time\n\
         child = subprocess.Popen([sys.executable, '-c', \"import time\\nwhile True:\\n    open(r'{}', 'a').write('x')\\n    time.sleep(0.1)\"])\n\
         time.sleep(60)\n",
        marker.display()
    );
    let (_sender, receiver) = watch::channel(false);
    let mut specification = request(python(), &["-c", &script], directory.path());
    specification.timeout_seconds = 2;
    let outcome = runner()
        .run(specification, receiver)
        .await
        .expect("the command is supervised");
    assert!(outcome.timed_out);

    let before = std::fs::metadata(&marker)
        .map(|data| data.len())
        .unwrap_or(0);
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    let after = std::fs::metadata(&marker)
        .map(|data| data.len())
        .unwrap_or(0);
    assert_eq!(
        before, after,
        "no descendant process may keep running after the process tree is terminated"
    );
}

#[tokio::test]
async fn output_beyond_the_limit_is_truncated_with_the_tail_preserved() {
    let directory = TempDir::new().expect("a temporary directory");
    let (_sender, receiver) = watch::channel(false);
    let mut specification = request(
        python(),
        &["-c", "print('a' * 60000); print('THE FINAL LINE')"],
        directory.path(),
    );
    specification.max_output_bytes = 8_192;
    let outcome = runner()
        .run(specification, receiver)
        .await
        .expect("the command runs");
    assert!(
        outcome.stdout_truncated,
        "the stream must be marked as truncated"
    );
    let text = outcome.stdout_text();
    assert!(text.contains("[output truncated"));
    assert!(
        text.contains("THE FINAL LINE"),
        "the tail must be preserved for diagnosis"
    );
}

#[tokio::test]
async fn a_missing_working_directory_is_reported_before_spawning() {
    let (_sender, receiver) = watch::channel(false);
    let outcome = runner()
        .run(
            request(
                python(),
                &["-c", "pass"],
                std::path::Path::new("/heikas/absent"),
            ),
            receiver,
        )
        .await;
    assert!(outcome.is_err());
}

#[tokio::test]
async fn probing_reports_a_present_and_an_absent_executable() {
    let runner = Arc::new(runner());
    let present = runner
        .probe_executable(python())
        .await
        .expect("the probe runs");
    assert!(present.is_some());
    let absent = runner
        .probe_executable("heikas-absent-executable")
        .await
        .expect("the probe runs");
    assert!(absent.is_none());
}

#[tokio::test]
async fn task_text_is_never_interpreted_by_a_shell() {
    let directory = TempDir::new().expect("a temporary directory");
    let sentinel = directory.path().join("injected.txt");
    let hostile = format!("; touch {}", sentinel.display());
    let (_sender, receiver) = watch::channel(false);
    let outcome = runner()
        .run(
            request(
                python(),
                &["-c", "import sys; print(sys.argv[1])", &hostile],
                directory.path(),
            ),
            receiver,
        )
        .await
        .expect("the command runs");
    assert!(outcome.stdout_text().contains("; touch"));
    assert!(
        !sentinel.exists(),
        "argument text must never reach a shell interpreter"
    );
}
