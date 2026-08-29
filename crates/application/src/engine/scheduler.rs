use heikas_domain::candidate::CandidateStatus;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::CandidateId;
use heikas_domain::plan::ApprovalDecision;
use heikas_domain::run::{CommitPolicy, RunStatus};
use heikas_domain::state::{NodeAttemptStatus, RunProjection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStep {
    RunNode(NodeId),
    RunCandidates,
    AwaitPlanApproval,
    AwaitCommitApproval,
    Cancel,
    Finish(RunStatus),
    Blocked(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateStep {
    RunNode(NodeId),
    Finish,
    Blocked(String),
}

pub fn next_run_step(projection: &RunProjection) -> RunStep {
    if projection.status.is_terminal() {
        return RunStep::Finish(projection.status);
    }
    if projection.status == RunStatus::RecoveryRequired {
        return RunStep::Blocked(
            projection
                .recovery_reason
                .clone()
                .unwrap_or_else(|| "the run requires manual recovery".to_string()),
        );
    }
    if projection.cancellation_requested {
        return RunStep::Cancel;
    }

    if !projection.completed_successfully(NodeId::Prepare, None) {
        return RunStep::RunNode(NodeId::Prepare);
    }

    match plan_gate(projection) {
        PlanGate::WritePlan => return RunStep::RunNode(NodeId::Plan),
        PlanGate::Await => return RunStep::AwaitPlanApproval,
        PlanGate::Rejected => return RunStep::Cancel,
        PlanGate::Approved => {}
    }

    if projection.candidates.is_empty() {
        return RunStep::RunNode(NodeId::FanOut);
    }

    if !projection.all_candidates_terminal() {
        return RunStep::RunCandidates;
    }

    if projection.ranking.is_none() {
        return RunStep::RunNode(NodeId::Join);
    }

    let Some(winner) = projection.winner.as_ref() else {
        return RunStep::Finish(RunStatus::Exhausted);
    };

    if projection.commit.is_some() {
        return RunStep::Finish(RunStatus::Succeeded);
    }

    if projection.integration.applied_candidate.as_ref() != Some(winner) {
        return RunStep::RunNode(NodeId::IntegrateWinner);
    }

    match projection.integration.final_tests_passed {
        Some(true) => {}
        _ => return RunStep::RunNode(NodeId::FinalTest),
    }

    match projection.integration.final_review_passed {
        Some(true) => {}
        _ => return RunStep::RunNode(NodeId::FinalReview),
    }

    match projection.commit_policy {
        CommitPolicy::None => RunStep::Finish(RunStatus::Succeeded),
        CommitPolicy::Automatic => RunStep::RunNode(NodeId::Commit),
        CommitPolicy::Manual => {
            if projection.commit_approved {
                RunStep::RunNode(NodeId::Commit)
            } else {
                RunStep::AwaitCommitApproval
            }
        }
    }
}

enum PlanGate {
    WritePlan,
    Await,
    Approved,
    Rejected,
}

fn plan_gate(projection: &RunProjection) -> PlanGate {
    let Some(current) = projection.plan.current() else {
        return PlanGate::WritePlan;
    };
    match projection.plan.approval.as_ref() {
        None => PlanGate::Await,
        Some(approval) => match approval.decision {
            ApprovalDecision::Rejected => PlanGate::Rejected,
            ApprovalDecision::RevisionRequested => {
                if approval.plan_version >= current.version {
                    PlanGate::WritePlan
                } else {
                    PlanGate::Await
                }
            }
            ApprovalDecision::Approved => {
                if approval.plan_hash == current.hash {
                    PlanGate::Approved
                } else {
                    PlanGate::Await
                }
            }
        },
    }
}

pub fn next_candidate_step(projection: &RunProjection, candidate: &CandidateId) -> CandidateStep {
    let Some(record) = projection.candidate(candidate) else {
        return CandidateStep::Blocked(format!("candidate {candidate} is not registered"));
    };
    if record.status.is_terminal() {
        return CandidateStep::Finish;
    }
    if projection.cancellation_requested {
        return CandidateStep::Finish;
    }

    let last = projection
        .attempts
        .iter()
        .filter(|attempt| attempt.candidate_id.as_ref() == Some(candidate))
        .max_by_key(|attempt| attempt.sequence);

    match last {
        None => CandidateStep::RunNode(NodeId::ImplementCandidate),
        Some(attempt) => match attempt.status {
            NodeAttemptStatus::Started | NodeAttemptStatus::Interrupted => {
                CandidateStep::RunNode(attempt.node_id)
            }
            NodeAttemptStatus::Succeeded | NodeAttemptStatus::Failed => match attempt.next {
                Some(NodeId::Join) | None => CandidateStep::Finish,
                Some(next) => CandidateStep::RunNode(next),
            },
            NodeAttemptStatus::Cancelled => CandidateStep::Finish,
            NodeAttemptStatus::Paused => CandidateStep::Blocked(
                attempt
                    .failure_summary
                    .clone()
                    .unwrap_or_else(|| "the candidate is paused".to_string()),
            ),
        },
    }
}

pub fn active_candidates(projection: &RunProjection) -> Vec<CandidateId> {
    projection
        .candidates
        .iter()
        .filter(|candidate| !candidate.status.is_terminal())
        .map(|candidate| candidate.id.clone())
        .collect()
}

pub fn run_status_for_node(node: NodeId) -> RunStatus {
    match node {
        NodeId::Prepare => RunStatus::Validating,
        NodeId::Plan => RunStatus::Planning,
        NodeId::Approval => RunStatus::AwaitingPlanApproval,
        NodeId::FanOut
        | NodeId::ImplementCandidate
        | NodeId::TestCandidate
        | NodeId::ReviewCandidate
        | NodeId::RepairCandidate => RunStatus::RunningCandidates,
        NodeId::Join => RunStatus::Joining,
        NodeId::IntegrateWinner | NodeId::FinalTest | NodeId::FinalReview => RunStatus::Integrating,
        NodeId::CommitApproval => RunStatus::AwaitingCommitApproval,
        NodeId::Commit => RunStatus::Integrating,
    }
}

pub fn candidate_status_for_node(node: NodeId) -> Option<CandidateStatus> {
    match node {
        NodeId::ImplementCandidate => Some(CandidateStatus::Implementing),
        NodeId::TestCandidate => Some(CandidateStatus::Testing),
        NodeId::ReviewCandidate => Some(CandidateStatus::Reviewing),
        NodeId::RepairCandidate => Some(CandidateStatus::Repairing),
        _ => None,
    }
}
