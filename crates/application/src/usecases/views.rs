use std::collections::BTreeMap;

use heikas_domain::event::{DurableEvent, EventPayload};
use heikas_domain::graph::{graph_edges, NodeId};
use heikas_domain::state::{NodeAttemptStatus, RunProjection};

use crate::model::detail::{GraphEdgeView, GraphNodeState, GraphNodeView, GraphView};
use crate::model::run_summary::{CandidateView, TimelineEntry, TimelineLevel};

pub fn candidate_views(projection: &RunProjection) -> Vec<CandidateView> {
    projection
        .candidates
        .iter()
        .map(|record| {
            let rank = projection.ranking.as_ref().and_then(|ranking| {
                ranking
                    .entries
                    .iter()
                    .find(|entry| entry.candidate_id == record.id)
                    .and_then(|entry| entry.rank)
            });
            let tests_passed = latest_test_outcome(projection, &record.id);
            let review_passed = latest_review_outcome(projection, &record.id);
            CandidateView {
                candidate_id: record.id.clone(),
                ordinal: record.ordinal.get(),
                strategy: record.strategy.as_str().to_string(),
                strategy_label: record.strategy.label().to_string(),
                status: record.status,
                status_label: record.status.label().to_string(),
                branch: record.branch.clone(),
                repairs_used: record.repairs_used,
                repair_budget: record.repair_budget,
                changed_files: record.changed_files,
                changed_lines: record.changed_lines,
                gate_duration: record.gate_duration,
                score: record.score.clone(),
                score_components: record
                    .score
                    .as_ref()
                    .map(|score| score.components())
                    .unwrap_or_default(),
                exclusion_reasons: record.exclusion_reasons.clone(),
                exclusion_summaries: record
                    .exclusion_reasons
                    .iter()
                    .map(heikas_domain::score::ExclusionReason::summary)
                    .collect(),
                rank,
                is_winner: projection.winner.as_ref() == Some(&record.id),
                promotable: record.promotable,
                tests_passed,
                review_passed,
                line_coverage_percent: record
                    .score
                    .as_ref()
                    .and_then(|score| score.coverage_rank.percent()),
            }
        })
        .collect()
}

fn latest_test_outcome(
    projection: &RunProjection,
    candidate: &heikas_domain::identity::CandidateId,
) -> Option<bool> {
    projection
        .attempts
        .iter()
        .filter(|attempt| {
            attempt.node_id == NodeId::TestCandidate
                && attempt.candidate_id.as_ref() == Some(candidate)
        })
        .max_by_key(|attempt| attempt.sequence)
        .map(|attempt| attempt.status == NodeAttemptStatus::Succeeded)
}

fn latest_review_outcome(
    projection: &RunProjection,
    candidate: &heikas_domain::identity::CandidateId,
) -> Option<bool> {
    projection
        .attempts
        .iter()
        .filter(|attempt| {
            attempt.node_id == NodeId::ReviewCandidate
                && attempt.candidate_id.as_ref() == Some(candidate)
        })
        .max_by_key(|attempt| attempt.sequence)
        .map(|attempt| attempt.status == NodeAttemptStatus::Succeeded)
}

pub fn graph_view(projection: &RunProjection) -> GraphView {
    let mut states: BTreeMap<NodeId, (GraphNodeState, u32, u64)> = BTreeMap::new();
    for node in NodeId::ALL {
        states.insert(node, (GraphNodeState::Pending, 0, 0));
    }
    for attempt in &projection.attempts {
        let entry = states
            .entry(attempt.node_id)
            .or_insert((GraphNodeState::Pending, 0, 0));
        entry.1 += 1;
        entry.2 = entry.2.saturating_add(attempt.duration.millis());
        entry.0 = match attempt.status {
            NodeAttemptStatus::Started => GraphNodeState::Active,
            NodeAttemptStatus::Succeeded => match entry.0 {
                GraphNodeState::Active => GraphNodeState::Active,
                _ => GraphNodeState::Succeeded,
            },
            NodeAttemptStatus::Failed => GraphNodeState::Failed,
            NodeAttemptStatus::Paused => GraphNodeState::Paused,
            NodeAttemptStatus::Cancelled | NodeAttemptStatus::Interrupted => {
                GraphNodeState::Skipped
            }
        };
    }

    if projection.plan.current().is_some() {
        let approval_state = if projection.plan.is_approved() {
            GraphNodeState::Succeeded
        } else {
            GraphNodeState::Paused
        };
        states.insert(NodeId::Approval, (approval_state, 1, 0));
    }
    if projection.commit_approved || projection.commit.is_some() {
        states.insert(NodeId::CommitApproval, (GraphNodeState::Succeeded, 1, 0));
    } else if projection.status == heikas_domain::run::RunStatus::AwaitingCommitApproval {
        states.insert(NodeId::CommitApproval, (GraphNodeState::Paused, 1, 0));
    }

    let nodes = NodeId::ALL
        .into_iter()
        .map(|node| {
            let (state, attempts, duration) =
                states
                    .get(&node)
                    .copied()
                    .unwrap_or((GraphNodeState::Pending, 0, 0));
            GraphNodeView {
                id: node.as_str().to_string(),
                label: node.label().to_string(),
                scope: match node.scope() {
                    heikas_domain::graph::NodeScope::Run => "run".to_string(),
                    heikas_domain::graph::NodeScope::Candidate => "candidate".to_string(),
                },
                class: format!("{:?}", node.class()).to_lowercase(),
                state,
                attempts,
                total_duration_ms: duration,
            }
        })
        .collect();

    let traversed: Vec<(NodeId, NodeId)> = projection
        .attempts
        .iter()
        .filter_map(|attempt| attempt.next.map(|next| (attempt.node_id, next)))
        .collect();
    let edges = graph_edges()
        .iter()
        .map(|edge| {
            let was_traversed = traversed
                .iter()
                .any(|(from, to)| *from == edge.from && *to == edge.to)
                || implicit_edge(projection, edge.from, edge.to);
            GraphEdgeView::from_edge(edge, was_traversed)
        })
        .collect();

    GraphView { nodes, edges }
}

fn implicit_edge(projection: &RunProjection, from: NodeId, to: NodeId) -> bool {
    match (from, to) {
        (NodeId::Approval, NodeId::FanOut) => projection.plan.is_approved(),
        (NodeId::Plan, NodeId::Approval) => projection.plan.current().is_some(),
        (NodeId::CommitApproval, NodeId::Commit) => projection.commit.is_some(),
        (NodeId::FinalReview, NodeId::CommitApproval) => {
            projection.integration.final_review_passed == Some(true)
        }
        _ => false,
    }
}

pub fn timeline(events: &[DurableEvent]) -> Vec<TimelineEntry> {
    events
        .iter()
        .map(|event| TimelineEntry {
            sequence: event.sequence,
            recorded_at: event.recorded_at,
            node_id: event.node_id,
            node_label: event.node_id.map(|node| node.label().to_string()),
            candidate_id: event.candidate_id.clone(),
            attempt: event.attempt.map(|attempt| attempt.get()),
            event_type: event.event_type.clone(),
            summary: event.payload.human_summary(),
            duration: duration_of(&event.payload),
            level: level_of(&event.payload),
        })
        .collect()
}

fn duration_of(payload: &EventPayload) -> Option<heikas_domain::clock::DurationMs> {
    match payload {
        EventPayload::NodeSucceeded { duration, .. }
        | EventPayload::NodeFailed { duration, .. }
        | EventPayload::TestEvidenceRecorded { duration, .. }
        | EventPayload::ReviewEvidenceRecorded { duration, .. } => Some(*duration),
        _ => None,
    }
}

fn level_of(payload: &EventPayload) -> TimelineLevel {
    match payload {
        EventPayload::NodeFailed { .. }
        | EventPayload::CandidateExcluded { .. }
        | EventPayload::NodeInterrupted { .. } => TimelineLevel::Failure,
        EventPayload::NodeSucceeded { .. }
        | EventPayload::CommitCreated { .. }
        | EventPayload::WinnerSelected { .. } => TimelineLevel::Success,
        EventPayload::NodePaused { .. }
        | EventPayload::NodeRetryScheduled { .. }
        | EventPayload::CandidateRepairStarted { .. }
        | EventPayload::CandidatePromotionRequested { .. }
        | EventPayload::RecoveryStarted { .. } => TimelineLevel::Warning,
        EventPayload::TestEvidenceRecorded { passed, .. }
        | EventPayload::ReviewEvidenceRecorded { passed, .. } => {
            if *passed {
                TimelineLevel::Success
            } else {
                TimelineLevel::Failure
            }
        }
        EventPayload::DiagnosticRecorded { level, .. } => match level {
            heikas_domain::event::DiagnosticLevel::Error => TimelineLevel::Failure,
            heikas_domain::event::DiagnosticLevel::Warning => TimelineLevel::Warning,
            heikas_domain::event::DiagnosticLevel::Info => TimelineLevel::Information,
        },
        _ => TimelineLevel::Information,
    }
}
