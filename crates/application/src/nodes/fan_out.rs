use heikas_domain::candidate::CandidateStatus;
use heikas_domain::event::EventPayload;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{BranchName, CandidateId, CandidateOrdinal};
use heikas_domain::path_policy::WorktreeRole;
use heikas_domain::run::CandidateStrategy;
use serde_json::json;
use std::str::FromStr;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{
    candidate_worktree_relative, heikas_home, load_dirty_snapshot, worktree_role_branch,
};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let baseline = crate::nodes::support::baseline(context)?;
    let count = configuration.budgets.candidates.get();

    let input = json!({
        "candidate_count": count,
        "baseline_commit": baseline.as_str(),
        "max_parallel_candidates": configuration.budgets.max_parallel_candidates,
        "repair_budget": configuration.budgets.max_repairs_per_candidate,
    });
    let evidence = AttemptEvidence::with_input(input);

    let snapshot = load_dirty_snapshot(context).await?;
    let home = heikas_home(context).await?;
    let mut events = Vec::new();

    for ordinal_value in 1..=count {
        let ordinal = CandidateOrdinal::new(ordinal_value)?;
        let candidate_id = CandidateId::derive(context.run.run_id, ordinal);
        let strategy = CandidateStrategy::for_ordinal(ordinal_value);
        let relative = candidate_worktree_relative(context.run.run_id, &candidate_id);
        let branch_text = worktree_role_branch(context, WorktreeRole::Candidate, Some(&candidate_id));
        let branch = BranchName::from_str(&branch_text)?;

        let handle = services
            .git
            .create_worktree(
                &configuration.repository_path,
                context.run.run_id,
                Some(&candidate_id),
                WorktreeRole::Candidate,
                &baseline,
                &branch,
            )
            .await?;

        if handle.path != home.join(&relative) {
            return Err(ApplicationError::Internal(format!(
                "the worktree service created `{}` but the run store expects `{}`",
                handle.path.display(),
                home.join(&relative).display()
            )));
        }

        if let Some(snapshot) = &snapshot {
            services.git.apply_snapshot(&handle.path, snapshot).await?;
        }

        events.push(EventPayload::CandidateRegistered {
            candidate_id: candidate_id.clone(),
            ordinal,
            strategy,
            branch: branch.to_string(),
            worktree_relative_path: relative,
            repair_budget: configuration.budgets.max_repairs_per_candidate,
        });
        events.push(EventPayload::CandidateStatusChanged {
            candidate_id,
            from: CandidateStatus::Pending,
            to: CandidateStatus::Preparing,
            reason: Some("the candidate worktree was created from the baseline".to_string()),
        });
    }

    Ok(NodeOutput::succeeded(Some(NodeId::ImplementCandidate))
        .with_events(events)
        .with_metrics(json!({
            "candidates_created": count,
            "dirty_snapshot_applied": snapshot.is_some(),
        }))
        .with_evidence(evidence))
}
