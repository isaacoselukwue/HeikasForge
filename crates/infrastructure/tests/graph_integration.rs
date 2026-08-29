use heikas_application::engine::DispatchOutcome;
use heikas_domain::candidate::CandidateStatus;
use heikas_domain::graph::NodeId;
use heikas_domain::run::RunStatus;
use heikas_domain::state::NodeAttemptStatus;
use heikas_fixture_harness::{
    approve_plan, build_scenario, correct_invoice, implementer_step, implementer_writing,
    planner_step, repairer_step, script, weakened_tests, wrong_invoice,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_single_candidate_completes_the_happy_path_and_commits() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &correct_invoice()),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;

    let paused = scenario.dispatch(run).await;
    assert_eq!(
        paused,
        DispatchOutcome::Paused(RunStatus::AwaitingPlanApproval)
    );
    let projection = scenario.projection(run).await;
    assert!(
        projection.candidates.is_empty(),
        "no candidate worktree may exist before approval"
    );

    approve_plan(&scenario, run).await;
    let after_candidates = scenario.dispatch(run).await;
    assert_eq!(
        after_candidates,
        DispatchOutcome::Paused(RunStatus::AwaitingCommitApproval)
    );

    let projection = scenario.projection(run).await;
    assert_eq!(projection.candidates.len(), 1);
    assert_eq!(projection.candidates[0].status, CandidateStatus::Eligible);
    assert_eq!(
        projection.winner,
        projection.candidates.first().map(|c| c.id.clone())
    );
    assert_eq!(projection.integration.final_tests_passed, Some(true));
    assert_eq!(projection.integration.final_review_passed, Some(true));

    scenario
        .service()
        .approve_commit(run, None)
        .await
        .expect("the commit approves");
    let finished = scenario.dispatch(run).await;
    assert_eq!(finished, DispatchOutcome::Completed(RunStatus::Succeeded));

    let projection = scenario.projection(run).await;
    let commit = projection.commit.expect("a commit was created");
    assert_eq!(commit.author_name, heikas_fixture_harness::AUTHOR);
    assert_eq!(commit.committer_name, heikas_fixture_harness::AUTHOR);
    assert!(commit.branch.as_str().starts_with("heikas/run-"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failing_test_routes_through_the_repair_loop_and_recovers() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &wrong_invoice()),
            repairer_step(1, 1, vec![("src/invoice.py", correct_invoice())]),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;
    approve_plan(&scenario, run).await;
    scenario.dispatch(run).await;

    let projection = scenario.projection(run).await;
    let candidate = &projection.candidates[0];
    assert_eq!(candidate.status, CandidateStatus::Eligible);
    assert_eq!(
        candidate.repairs_used, 1,
        "exactly one repair must be recorded"
    );

    let test_attempts: Vec<_> = projection
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::TestCandidate)
        .collect();
    assert_eq!(test_attempts.len(), 2, "the test node must run twice");
    assert_eq!(test_attempts[0].status, NodeAttemptStatus::Failed);
    assert_eq!(test_attempts[1].status, NodeAttemptStatus::Succeeded);
    assert_eq!(test_attempts[0].next, Some(NodeId::RepairCandidate));
    assert!(projection.metrics.repair_loops >= 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_identical_failures_exhaust_the_repair_budget() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &wrong_invoice()),
            repairer_step(1, 1, vec![("src/invoice.py", wrong_invoice())]),
            repairer_step(1, 2, vec![("src/invoice.py", wrong_invoice())]),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;
    approve_plan(&scenario, run).await;
    let outcome = scenario.dispatch(run).await;

    assert_eq!(outcome, DispatchOutcome::Completed(RunStatus::Exhausted));
    let projection = scenario.projection(run).await;
    assert_eq!(projection.candidates[0].status, CandidateStatus::Ineligible);
    assert_eq!(projection.candidates[0].repairs_used, 2);
    assert!(projection.winner.is_none());
    assert!(projection.commit.is_none());
    let ranking = projection.ranking.expect("a ranking is recorded");
    assert!(ranking.winner.is_none());
    assert!(!ranking.entries[0].exclusion_reasons.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_quality_failure_makes_a_candidate_ineligible_with_a_recorded_reason() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_writing(
                1,
                vec![
                    ("src/invoice.py", correct_invoice()),
                    ("tests/test_invoice.py", weakened_tests()),
                ],
            ),
            repairer_step(
                1,
                1,
                vec![
                    ("src/invoice.py", correct_invoice()),
                    ("tests/test_invoice.py", weakened_tests()),
                ],
            ),
            repairer_step(
                1,
                2,
                vec![
                    ("src/invoice.py", correct_invoice()),
                    ("tests/test_invoice.py", weakened_tests()),
                ],
            ),
        ]),
        2,
        1,
    );
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;
    approve_plan(&scenario, run).await;
    let outcome = scenario.dispatch(run).await;

    assert_eq!(outcome, DispatchOutcome::Completed(RunStatus::Exhausted));
    let projection = scenario.projection(run).await;
    let candidate = &projection.candidates[0];
    assert_eq!(candidate.status, CandidateStatus::Ineligible);
    let summaries: Vec<String> = candidate
        .exclusion_reasons
        .iter()
        .map(heikas_domain::score::ExclusionReason::summary)
        .collect();
    assert!(
        summaries
            .iter()
            .any(|reason| reason.contains("declared 3 tests")),
        "the removed test must be reported: {summaries:?}"
    );

    let review_attempts: Vec<_> = projection
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::ReviewCandidate)
        .collect();
    assert!(
        review_attempts.len() >= 2,
        "the review must run again after each repair"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn three_candidates_run_concurrently_without_interfering() {
    let scenario = build_scenario(
        script(vec![
            planner_step(),
            implementer_step(1, &correct_invoice()),
            implementer_step(2, &wrong_invoice()),
            repairer_step(2, 1, vec![("src/invoice.py", correct_invoice())]),
            implementer_writing(
                3,
                vec![
                    ("src/invoice.py", correct_invoice()),
                    ("tests/test_invoice.py", weakened_tests()),
                ],
            ),
            repairer_step(
                3,
                1,
                vec![
                    ("src/invoice.py", correct_invoice()),
                    ("tests/test_invoice.py", weakened_tests()),
                ],
            ),
            repairer_step(
                3,
                2,
                vec![
                    ("src/invoice.py", correct_invoice()),
                    ("tests/test_invoice.py", weakened_tests()),
                ],
            ),
        ]),
        2,
        3,
    );
    let run = scenario.create_run(3).await;
    scenario.dispatch(run).await;
    approve_plan(&scenario, run).await;
    scenario.dispatch(run).await;

    let projection = scenario.projection(run).await;
    assert_eq!(projection.candidates.len(), 3);

    let eligible: Vec<_> = projection
        .candidates
        .iter()
        .filter(|candidate| candidate.status == CandidateStatus::Eligible)
        .collect();
    let ineligible: Vec<_> = projection
        .candidates
        .iter()
        .filter(|candidate| candidate.status == CandidateStatus::Ineligible)
        .collect();
    assert_eq!(eligible.len(), 2, "two candidates must satisfy every gate");
    assert_eq!(ineligible.len(), 1, "one candidate must be excluded");

    let winner = projection.winner.clone().expect("a winner is selected");
    assert_eq!(
        winner, projection.candidates[0].id,
        "the smallest clean change must win the deterministic tuple"
    );

    let mut worktrees: Vec<String> = projection
        .candidates
        .iter()
        .map(|candidate| candidate.worktree_relative_path.clone())
        .collect();
    worktrees.sort();
    worktrees.dedup();
    assert_eq!(
        worktrees.len(),
        3,
        "every candidate must own a distinct worktree"
    );

    for candidate in &projection.candidates {
        let path = scenario.home.path().join(&candidate.worktree_relative_path);
        assert!(
            path.join(".git").exists(),
            "each candidate worktree must exist"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn the_winner_is_identical_across_repeated_join_evaluation() {
    let steps = vec![
        planner_step(),
        implementer_step(1, &correct_invoice()),
        implementer_writing(
            2,
            vec![
                ("src/invoice.py", correct_invoice()),
                ("src/extra.py", "VALUE = 1\n".to_string()),
            ],
        ),
    ];
    let first = build_scenario(script(steps.clone()), 2, 2);
    let run = first.create_run(2).await;
    first.dispatch(run).await;
    approve_plan(&first, run).await;
    first.dispatch(run).await;
    let first_projection = first.projection(run).await;

    let second = build_scenario(script(steps), 2, 2);
    let other = second.create_run(2).await;
    second.dispatch(other).await;
    approve_plan(&second, other).await;
    second.dispatch(other).await;
    let second_projection = second.projection(other).await;

    let first_winner = first_projection.winner.expect("a winner");
    let second_winner = second_projection.winner.expect("a winner");
    assert_eq!(
        first_winner.ordinal(),
        second_winner.ordinal(),
        "the same evidence must select the same candidate ordinal"
    );

    let first_ranking = first_projection.ranking.expect("a ranking");
    let second_ranking = second_projection.ranking.expect("a ranking");
    let first_order: Vec<Option<u32>> = first_ranking
        .entries
        .iter()
        .map(|entry| entry.rank)
        .collect();
    let second_order: Vec<Option<u32>> = second_ranking
        .entries
        .iter()
        .map(|entry| entry.rank)
        .collect();
    assert_eq!(first_order, second_order);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_paused_run_survives_a_dispatcher_restart_and_resumes_from_files() {
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

    let paused = scenario.projection(run).await;
    assert_eq!(paused.status, RunStatus::AwaitingPlanApproval);
    let plan_attempts = paused
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::Plan)
        .count();
    assert_eq!(plan_attempts, 1);

    let rebuilt = heikas_infrastructure::build_runtime(heikas_infrastructure::StoreLayout::new(
        scenario.home.path().to_path_buf(),
    ))
    .expect("a fresh runtime opens the same store");
    let reloaded = rebuilt
        .service
        .projection(run)
        .await
        .expect("the projection loads from files alone");
    assert_eq!(reloaded.status, RunStatus::AwaitingPlanApproval);
    assert_eq!(reloaded.last_event_sequence, paused.last_event_sequence);
    assert_eq!(reloaded.last_event_hash, paused.last_event_hash);

    rebuilt
        .service
        .approve_plan(run, None, None)
        .await
        .expect("the plan approves after the restart");
    let outcome = rebuilt
        .service
        .dispatch(run)
        .await
        .expect("the dispatch runs");
    assert_eq!(
        outcome,
        DispatchOutcome::Paused(RunStatus::AwaitingCommitApproval)
    );

    let final_projection = rebuilt
        .service
        .projection(run)
        .await
        .expect("the projection loads");
    let plan_attempts_after = final_projection
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::Plan)
        .count();
    assert_eq!(
        plan_attempts_after, 1,
        "a completed node must never rerun because the dispatcher restarted"
    );
    let prepare_attempts = final_projection
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::Prepare)
        .count();
    assert_eq!(prepare_attempts, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejecting_a_plan_cancels_the_run_before_any_candidate_exists() {
    let scenario = build_scenario(script(vec![planner_step()]), 2, 1);
    let run = scenario.create_run(1).await;
    scenario.dispatch(run).await;

    scenario
        .service()
        .reject_plan(run, Some("the task description was wrong".to_string()))
        .await
        .expect("the plan rejects");
    let outcome = scenario.dispatch(run).await;
    assert_eq!(outcome, DispatchOutcome::Completed(RunStatus::Cancelled));

    let projection = scenario.projection(run).await;
    assert!(projection.candidates.is_empty());
    assert!(projection.commit.is_none());
    assert!(!scenario
        .home
        .path()
        .join("worktrees")
        .join(run.to_string())
        .exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requesting_a_revision_produces_a_second_plan_version() {
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

    scenario
        .service()
        .revise_plan(run, "Cover the negative tie case explicitly.".to_string())
        .await
        .expect("the revision is requested");
    let outcome = scenario.dispatch(run).await;
    assert_eq!(
        outcome,
        DispatchOutcome::Paused(RunStatus::AwaitingPlanApproval)
    );

    let projection = scenario.projection(run).await;
    assert_eq!(projection.plan.versions.len(), 2);
    assert!(!projection.plan.is_approved());
    let plan_attempts = projection
        .attempts
        .iter()
        .filter(|attempt| attempt.node_id == NodeId::Plan)
        .count();
    assert_eq!(plan_attempts, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn editing_the_plan_invalidates_an_existing_approval() {
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
    assert!(scenario.projection(run).await.plan.is_approved());

    scenario
        .service()
        .update_plan(
            run,
            &format!(
                "{}\n\nAn extra clarification.\n",
                heikas_fixture_harness::plan_document()
            ),
        )
        .await
        .expect("the plan updates");
    let projection = scenario.projection(run).await;
    assert!(
        !projection.plan.is_approved(),
        "a plan edit must invalidate the approval automatically"
    );
    assert_eq!(projection.plan.versions.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_a_run_terminates_it_and_records_the_reason() {
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

    scenario
        .service()
        .cancel(run, Some("the operator stopped the run".to_string()))
        .await
        .expect("the cancellation records");

    let projection = scenario.projection(run).await;
    assert_eq!(projection.status, RunStatus::Cancelled);
    assert!(projection.cancellation_requested);
    assert!(projection.commit.is_none());

    scenario
        .service()
        .cancel(run, None)
        .await
        .expect("cancellation is idempotent");
    assert_eq!(scenario.projection(run).await.status, RunStatus::Cancelled);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_event_log_remains_verifiable_after_a_complete_run() {
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
    scenario.dispatch(run).await;
    scenario
        .service()
        .approve_commit(run, None)
        .await
        .expect("the commit approves");
    scenario.dispatch(run).await;

    let verification = scenario
        .runtime
        .store
        .verify_chain(run)
        .await
        .expect("the chain verifies");
    assert!(!verification.quarantined_partial_record);
    assert!(verification.events_verified > 20);

    let projection = scenario.projection(run).await;
    assert_eq!(projection.last_event_sequence, verification.last_sequence);
    assert_eq!(projection.last_event_hash, verification.last_hash);

    let replayed = heikas_domain::state::replay(
        run,
        projection.created_at,
        &scenario
            .runtime
            .store
            .read_after(run, 0)
            .await
            .expect("the events read"),
    )
    .expect("a full replay succeeds");
    assert_eq!(
        serde_json::to_string(&replayed).expect("encodes"),
        serde_json::to_string(&projection).expect("encodes"),
        "the stored projection must equal a full replay"
    );
}
