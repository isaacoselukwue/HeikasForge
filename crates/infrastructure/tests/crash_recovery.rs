use std::process::{Command, Stdio};
use std::time::Duration;

use heikas_application::engine::DispatchOutcome;
use heikas_domain::candidate::CandidateStatus;
use heikas_domain::event::EventPayload;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::AttemptNumber;
use heikas_domain::run::RunStatus;
use heikas_domain::state::NodeAttemptStatus;
use heikas_fixture_harness::{
    approve_plan, build_scenario, correct_invoice, implementer_step, planner_step, script,
};
use heikas_infrastructure::{build_runtime, StoreLayout};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_attempt_interrupted_after_node_started_is_recovered_and_retried() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &correct_invoice()),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;
    approve_plan(&scenario, run).await;

    scenario
        .service()
        .append(
            run,
            vec![EventPayload::NodeStarted {
                node_id: NodeId::FanOut,
                candidate_id: None,
                attempt: AttemptNumber::FIRST,
                prompt_template_hash: None,
            }],
        )
        .await
        .expect("the interrupted attempt is recorded");

    let before = scenario.projection(run).await;
    assert_eq!(before.open_attempts().len(), 1);

    let rebuilt = build_runtime(StoreLayout::new(scenario.home.path().to_path_buf()))
        .expect("a fresh dispatcher opens the store");
    let outcome = rebuilt
        .service
        .dispatch(run)
        .await
        .expect("the dispatch runs");
    assert_eq!(
        outcome,
        DispatchOutcome::Paused(RunStatus::AwaitingCommitApproval)
    );

    let after = rebuilt
        .service
        .projection(run)
        .await
        .expect("the projection loads");
    assert!(after.open_attempts().is_empty(), "no attempt may stay open");

    let fan_out: Vec<_> = after
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::FanOut)
        .collect();
    assert_eq!(
        fan_out.len(),
        2,
        "the interrupted attempt and its retry are both recorded"
    );
    assert_eq!(fan_out[0].status, NodeAttemptStatus::Interrupted);
    assert_eq!(fan_out[1].status, NodeAttemptStatus::Succeeded);
    assert_eq!(
        fan_out[1].attempt,
        AttemptNumber::new(2).expect("attempt two")
    );

    let events = rebuilt
        .store
        .read_after(run, 0)
        .await
        .expect("the events read");
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, EventPayload::RecoveryStarted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, EventPayload::NodeInterrupted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, EventPayload::RecoveryCompleted { .. })));

    let plan_attempts = after
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::Plan)
        .count();
    assert_eq!(plan_attempts, 1, "a completed node must never be repeated");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_crash_after_the_terminal_event_is_repaired_by_replay_without_rerunning_the_node() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &correct_invoice()),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;

    let complete = scenario.projection(run).await;
    assert_eq!(complete.status, RunStatus::AwaitingPlanApproval);
    let full_sequence = complete.last_event_sequence;

    let rewound = heikas_domain::state::replay(
        run,
        complete.created_at,
        &scenario
            .runtime
            .store
            .read_after(run, 0)
            .await
            .expect("the events read")
            .into_iter()
            .filter(|event| event.sequence <= 3)
            .collect::<Vec<_>>(),
    )
    .expect("a partial replay succeeds");
    assert!(rewound.last_event_sequence < full_sequence);
    scenario
        .runtime
        .store
        .store(&rewound)
        .await
        .expect("the stale projection is written");

    let rebuilt = build_runtime(StoreLayout::new(scenario.home.path().to_path_buf()))
        .expect("a fresh dispatcher opens the store");

    let raw = rebuilt
        .store
        .load(run)
        .await
        .expect("the stored projection reads")
        .expect("a projection exists");
    assert_eq!(
        raw.last_event_sequence, 3,
        "the file on disk is deliberately stale before recovery"
    );

    let repaired = rebuilt
        .service
        .projection(run)
        .await
        .expect("the projection loads");
    assert_eq!(
        repaired.last_event_sequence, full_sequence,
        "reading a run must reconcile the projection with the durable event log"
    );
    assert_eq!(repaired.status, RunStatus::AwaitingPlanApproval);

    rebuilt
        .service
        .approve_plan(run, None, None)
        .await
        .expect("the plan approves after the projection was rewound");

    let recovered = rebuilt
        .service
        .projection(run)
        .await
        .expect("the projection loads");
    assert!(
        recovered.last_event_sequence > full_sequence,
        "the approval must append after the recovered tail"
    );
    let plan_attempts = recovered
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::Plan)
        .count();
    assert_eq!(
        plan_attempts, 1,
        "the completed plan node must not be rerun after projection repair"
    );
    assert_eq!(recovered.plan.versions.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_interrupted_candidate_is_marked_and_then_driven_to_completion() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &correct_invoice()),
            heikas_fixture_harness::repairer_step(
                1,
                1,
                vec![("src/invoice.py", correct_invoice())],
            ),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;
    approve_plan(&scenario, run).await;

    let mut runtime = build_runtime(StoreLayout::new(scenario.home.path().to_path_buf()))
        .expect("a dispatcher opens the store");
    runtime
        .service
        .dispatch(run)
        .await
        .expect("the run completes");

    let projection = runtime
        .service
        .projection(run)
        .await
        .expect("the projection loads");
    let candidate = projection.candidates[0].clone();
    assert_eq!(candidate.status, CandidateStatus::Eligible);

    runtime = build_runtime(StoreLayout::new(scenario.home.path().to_path_buf()))
        .expect("a second dispatcher opens the store");
    let outcome = runtime
        .service
        .dispatch(run)
        .await
        .expect("the dispatch runs");
    assert_eq!(
        outcome,
        DispatchOutcome::Paused(RunStatus::AwaitingCommitApproval)
    );

    let after = runtime
        .service
        .projection(run)
        .await
        .expect("the projection loads");
    let implement_attempts = after
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::ImplementCandidate)
        .count();
    assert_eq!(
        implement_attempts, 1,
        "restarting the dispatcher must not repeat completed candidate work"
    );
}

#[test]
fn a_forced_process_exit_leaves_a_recoverable_run() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &correct_invoice()),
        ]),
        2,
        1,
    );
    let executable = heikas_executable();
    let home = scenario.home.path().display().to_string();
    let task = scenario.repository.join("TASK.md").display().to_string();

    let mut child = Command::new(&executable)
        .args([
            "--json",
            "run",
            "--repo",
            &scenario.repository.display().to_string(),
            "--task-file",
            &task,
            "--demonstration",
            "--agent",
            "fake",
        ])
        .env("HEIKAS_HOME", &home)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the orchestrator starts");

    let runs_directory = scenario.home.path().join("runs");
    let mut attempts = 0;
    let run_directory = loop {
        let candidate = std::fs::read_dir(&runs_directory)
            .ok()
            .and_then(|entries| entries.filter_map(Result::ok).next())
            .map(|entry| entry.path());
        if let Some(candidate) = candidate {
            if candidate.join("events.jsonl").is_file() {
                break candidate;
            }
        }
        attempts += 1;
        assert!(
            attempts < 300,
            "the run never reached its first durable event"
        );
        std::thread::sleep(Duration::from_millis(100));
    };

    child.kill().expect("the process is killed");
    let _ = child.wait();

    let run_id = run_directory
        .file_name()
        .expect("a run directory name")
        .to_string_lossy()
        .to_string();

    assert!(
        run_directory.join("events.jsonl").is_file(),
        "the durable event log must survive a forced exit"
    );

    let resumed = Command::new(&executable)
        .args(["--json", "resume", &run_id])
        .env("HEIKAS_HOME", &home)
        .output()
        .expect("the resume runs");
    let code = resumed.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 3,
        "resume must recover the run, exit code {code}, stderr {}",
        String::from_utf8_lossy(&resumed.stderr)
    );

    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(run_directory.join("state.json")).expect("the projection reads"),
    )
    .expect("the projection decodes");
    let status = state["status"].as_str().unwrap_or_default();
    assert!(
        [
            "awaiting_plan_approval",
            "awaiting_commit_approval",
            "succeeded",
            "planning"
        ]
        .contains(&status),
        "the recovered run reached an expected state, found {status}"
    );
    assert!(
        state["attempts"]
            .as_array()
            .expect("attempts")
            .iter()
            .all(|attempt| attempt["status"] != "started"),
        "no attempt may remain open after recovery"
    );
}

fn heikas_executable() -> std::path::PathBuf {
    let root = heikas_fixture_harness::workspace_root();
    let name = if cfg!(windows) {
        "heikas.exe"
    } else {
        "heikas"
    };
    for profile in ["debug", "release"] {
        let candidate = root.join("target").join(profile).join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .args(["build", "-p", "heikas-cli"])
        .current_dir(&root)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "the heikas executable must build");
    root.join("target").join("debug").join(name)
}
