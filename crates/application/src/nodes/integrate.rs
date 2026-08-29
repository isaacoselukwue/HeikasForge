use std::str::FromStr;

use heikas_domain::event::EventPayload;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{BranchName, CandidateId};
use heikas_domain::node::StatePatch;
use heikas_domain::path_policy::WorktreeRole;
use heikas_domain::run::RunStatus;
use heikas_domain::score::ExclusionReason;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{
    baseline, integration_worktree_relative, load_dirty_snapshot, worktree_role_branch,
};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let baseline_commit = baseline(context)?;
    let winner = context
        .projection
        .winner
        .clone()
        .ok_or_else(|| ApplicationError::Internal("no winner is selected for integration".to_string()))?;

    let branch_text = worktree_role_branch(context, WorktreeRole::Integration, None);
    let branch = BranchName::from_str(&branch_text)?;
    let handle = services
        .git
        .create_worktree(
            &configuration.repository_path,
            context.run.run_id,
            None,
            WorktreeRole::Integration,
            &baseline_commit,
            &branch,
        )
        .await?;
    services
        .git
        .reset_worktree(&handle.path, &baseline_commit)
        .await?;
    if let Some(snapshot) = load_dirty_snapshot(context).await? {
        services.git.apply_snapshot(&handle.path, &snapshot).await?;
    }

    let patch = services
        .store
        .read_diff(context.run.run_id, &winner)
        .await
        .unwrap_or_default();

    let input = json!({
        "winner": winner.as_str(),
        "integration_worktree": integration_worktree_relative(context.run.run_id),
        "patch_bytes": patch.len(),
    });
    let evidence = AttemptEvidence::with_input(input);

    if patch.is_empty() {
        return promote_next(
            context,
            &winner,
            "the winning candidate produced an empty patch".to_string(),
            evidence,
        )
        .await;
    }

    match services.git.apply_patch(&handle.path, &patch).await {
        Ok(()) => {
            let digest = services
                .store
                .write_integration_diff(context.run.run_id, &patch)
                .await?;
            Ok(NodeOutput::succeeded(Some(NodeId::FinalTest))
                .with_event(EventPayload::IntegrationAttempted {
                    candidate_id: winner.clone(),
                    applied: true,
                    detail: None,
                })
                .with_metrics(json!({
                    "applied": true,
                    "integration_diff_digest": digest.as_str(),
                }))
                .with_evidence(evidence))
        }
        Err(error) => {
            promote_next(
                context,
                &winner,
                format!("the winning candidate patch did not apply: {error}"),
                evidence,
            )
            .await
        }
    }
}

async fn promote_next(
    context: &NodeContext<'_>,
    failed: &CandidateId,
    detail: String,
    evidence: AttemptEvidence,
) -> ApplicationResult<NodeOutput> {
    let next = next_promotable(context, failed);
    let mut events = vec![EventPayload::IntegrationAttempted {
        candidate_id: failed.clone(),
        applied: false,
        detail: Some(detail.clone()),
    }];
    events.push(EventPayload::CandidateExcluded {
        candidate_id: failed.clone(),
        reasons: vec![ExclusionReason::IntegrationFailed {
            detail: detail.clone(),
        }],
    });
    events.push(EventPayload::CandidatePromotionRequested {
        previous_candidate_id: failed.clone(),
        next_candidate_id: next.clone(),
        reason: detail.clone(),
    });

    let metrics = json!({
        "applied": false,
        "detail": detail,
        "next_candidate": next.as_ref().map(CandidateId::to_string),
    });

    match next {
        Some(_) => Ok(NodeOutput::succeeded(Some(NodeId::IntegrateWinner))
            .with_events(events)
            .with_metrics(metrics)
            .with_evidence(evidence)),
        None => Ok(NodeOutput::succeeded(None)
            .with_events(events)
            .with_patch(StatePatch {
                run_status: Some(RunStatus::Exhausted),
                ..StatePatch::default()
            })
            .with_metrics(metrics)
            .with_evidence(evidence)
            .with_warning("no further candidate could be promoted")),
    }
}

pub fn next_promotable(context: &NodeContext<'_>, failed: &CandidateId) -> Option<CandidateId> {
    let ranking = context.projection.ranking.as_ref()?;
    ranking
        .entries
        .iter()
        .filter(|entry| entry.eligible && entry.rank.is_some())
        .filter(|entry| &entry.candidate_id != failed)
        .filter(|entry| {
            context
                .projection
                .candidate(&entry.candidate_id)
                .map(|record| record.promotable && !record.integration_attempted)
                .unwrap_or(false)
        })
        .min_by_key(|entry| entry.rank.unwrap_or(u32::MAX))
        .map(|entry| entry.candidate_id.clone())
}
