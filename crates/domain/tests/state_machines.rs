use heikas_domain::candidate::CandidateStatus;
use heikas_domain::failure::FailureClass;
use heikas_domain::graph::{graph_edges, NodeId, NodeScope};
use heikas_domain::retry::{classify_retry, RetryDecision, RetryPolicy};
use heikas_domain::run::RunStatus;
use heikas_domain::DomainError;

#[test]
fn every_run_status_pair_is_either_permitted_or_rejected_explicitly() {
    for from in RunStatus::ALL {
        for to in RunStatus::ALL {
            let outcome = from.transition_to(to);
            if from == to {
                assert!(outcome.is_ok(), "a status must transition to itself");
                continue;
            }
            if from.allowed_next().contains(&to) {
                assert_eq!(outcome.expect("the transition is permitted"), to);
            } else {
                assert!(
                    matches!(outcome, Err(DomainError::IllegalRunTransition { .. })),
                    "{from} to {to} must be rejected"
                );
            }
        }
    }
}

#[test]
fn terminal_run_statuses_permit_no_further_transition() {
    for status in RunStatus::ALL.into_iter().filter(RunStatus::is_terminal) {
        assert!(
            status.allowed_next().is_empty(),
            "{status} is terminal and must not permit a successor"
        );
    }
}

#[test]
fn recovery_required_can_return_to_every_active_status() {
    let reachable = RunStatus::RecoveryRequired.allowed_next();
    for status in [
        RunStatus::Validating,
        RunStatus::Planning,
        RunStatus::AwaitingPlanApproval,
        RunStatus::RunningCandidates,
        RunStatus::Joining,
        RunStatus::Integrating,
        RunStatus::AwaitingCommitApproval,
    ] {
        assert!(reachable.contains(&status), "recovery must reach {status}");
    }
}

#[test]
fn every_candidate_status_pair_is_either_permitted_or_rejected_explicitly() {
    for from in CandidateStatus::ALL {
        for to in CandidateStatus::ALL {
            let outcome = from.transition_to(to);
            if from == to {
                assert!(outcome.is_ok());
                continue;
            }
            if from.allowed_next().contains(&to) {
                assert_eq!(outcome.expect("the transition is permitted"), to);
            } else {
                assert!(
                    matches!(outcome, Err(DomainError::IllegalCandidateTransition { .. })),
                    "{from} to {to} must be rejected"
                );
            }
        }
    }
}

#[test]
fn terminal_candidate_statuses_are_final_except_demotion_to_ineligible() {
    assert!(CandidateStatus::Ineligible.allowed_next().is_empty());
    assert!(CandidateStatus::Cancelled.allowed_next().is_empty());
    assert_eq!(
        CandidateStatus::Eligible.allowed_next(),
        &[CandidateStatus::Ineligible]
    );
}

#[test]
fn every_declared_graph_edge_is_an_allowed_transition() {
    for edge in graph_edges() {
        assert!(
            edge.from.accepts_successor(edge.to),
            "the declared edge {} to {} is not permitted by the node contract",
            edge.from,
            edge.to
        );
    }
}

#[test]
fn every_allowed_transition_is_a_declared_graph_edge() {
    let edges = graph_edges();
    for node in NodeId::ALL {
        for successor in node.allowed_successors() {
            assert!(
                edges
                    .iter()
                    .any(|edge| edge.from == node && edge.to == *successor),
                "the transition {node} to {successor} is missing from the declared graph"
            );
        }
    }
}

#[test]
fn only_the_commit_node_is_a_graph_sink() {
    for node in NodeId::ALL {
        if node == NodeId::Commit {
            assert!(node.allowed_successors().is_empty());
        } else {
            assert!(
                !node.allowed_successors().is_empty(),
                "{node} must declare at least one successor"
            );
        }
    }
}

#[test]
fn planning_and_decision_nodes_are_read_only() {
    assert!(NodeId::Plan.is_read_only());
    assert!(NodeId::Approval.is_read_only());
    assert!(NodeId::Join.is_read_only());
    assert!(!NodeId::ImplementCandidate.is_read_only());
    assert!(!NodeId::RepairCandidate.is_read_only());
}

#[test]
fn candidate_scoped_nodes_are_exactly_the_subgraph_nodes() {
    let candidate_scoped: Vec<NodeId> = NodeId::ALL
        .into_iter()
        .filter(|node| node.scope() == NodeScope::Candidate)
        .collect();
    assert_eq!(
        candidate_scoped,
        vec![
            NodeId::ImplementCandidate,
            NodeId::TestCandidate,
            NodeId::ReviewCandidate,
            NodeId::RepairCandidate
        ]
    );
}

#[test]
fn only_transient_infrastructure_failures_retry_the_same_node() {
    let policy = RetryPolicy::default();
    for class in FailureClass::ALL {
        let decision = classify_retry(NodeId::TestCandidate, class, 1, policy, true);
        if class == FailureClass::TransientInfrastructure {
            assert_eq!(decision, RetryDecision::RetrySameNode);
        } else {
            assert_ne!(
                decision,
                RetryDecision::RetrySameNode,
                "{class} must not retry the same node automatically"
            );
        }
    }
}

#[test]
fn task_failures_route_to_repair_while_budget_remains() {
    let policy = RetryPolicy::default();
    assert_eq!(
        classify_retry(
            NodeId::TestCandidate,
            FailureClass::TaskFailure,
            1,
            policy,
            true
        ),
        RetryDecision::RouteToRepair
    );
    assert_eq!(
        classify_retry(
            NodeId::TestCandidate,
            FailureClass::TaskFailure,
            1,
            policy,
            false
        ),
        RetryDecision::FailCandidate
    );
    assert_eq!(
        classify_retry(NodeId::Plan, FailureClass::TaskFailure, 1, policy, true),
        RetryDecision::FailRun
    );
}

#[test]
fn transient_failures_stop_retrying_once_the_attempt_budget_is_spent() {
    let policy = RetryPolicy::default();
    assert_eq!(
        classify_retry(
            NodeId::TestCandidate,
            FailureClass::TransientInfrastructure,
            policy.maximum_attempts,
            policy,
            true
        ),
        RetryDecision::FailCandidate
    );
    assert_eq!(
        classify_retry(
            NodeId::Plan,
            FailureClass::TransientInfrastructure,
            policy.maximum_attempts,
            policy,
            false
        ),
        RetryDecision::FailRun
    );
}

#[test]
fn user_action_and_cancellation_always_take_precedence() {
    let policy = RetryPolicy::default();
    assert_eq!(
        classify_retry(
            NodeId::Commit,
            FailureClass::UserActionRequired,
            1,
            policy,
            true
        ),
        RetryDecision::PauseForUser
    );
    assert_eq!(
        classify_retry(
            NodeId::TestCandidate,
            FailureClass::Cancelled,
            1,
            policy,
            true
        ),
        RetryDecision::Cancel
    );
}

#[test]
fn backoff_grows_and_is_capped() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.base_delay(1).as_millis(), 500);
    assert_eq!(policy.base_delay(2).as_millis(), 1_000);
    assert_eq!(policy.base_delay(3).as_millis(), 2_000);
    assert_eq!(
        policy.base_delay(20).as_millis(),
        u128::from(policy.maximum_delay_ms)
    );
    assert!(policy.delay_with_jitter(3, 0.0).as_millis() == 0);
    assert_eq!(policy.delay_with_jitter(3, 1.0), policy.base_delay(3));
}

#[test]
fn an_operator_written_pattern_is_matched_by_the_same_glob_engine_everywhere() {
    use heikas_domain::path_policy::{
        evaluate_path, GlobPatternMatcher, PathAccess, PathPolicy, RelativeWorkspacePath,
    };

    let policy = PathPolicy {
        protected_patterns: vec![
            ".github/**/*.yml".to_string(),
            "infra/*/secrets.tf".to_string(),
            "deploy/**/*.sh".to_string(),
        ],
        sensitive_patterns: Vec::new(),
        approved_protected_paths: Vec::new(),
        maximum_read_bytes: 1_048_576,
        maximum_write_bytes: 4_194_304,
    };

    for protected in [
        ".github/workflows/ci.yml",
        "infra/production/secrets.tf",
        "deploy/staging/release.sh",
    ] {
        let path = RelativeWorkspacePath::parse(protected).expect("a relative path");
        assert!(
            evaluate_path(&policy, &GlobPatternMatcher, &path, PathAccess::Write).is_err(),
            "`{protected}` must be refused for writing"
        );
    }

    let permitted = RelativeWorkspacePath::parse("src/main.rs").expect("a relative path");
    assert!(evaluate_path(&policy, &GlobPatternMatcher, &permitted, PathAccess::Write).is_ok());
}
