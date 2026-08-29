use std::path::PathBuf;

use heikas_domain::identity::{CandidateId, CommitHash, ContentDigest};
use heikas_domain::node::ArtifactReference;
use heikas_domain::path_policy::WorktreeRole;

use crate::engine::context::NodeContext;
use crate::error::{ApplicationError, ApplicationResult};
use crate::ports::git::DirtySnapshot;

pub const DIRTY_TRACKED_LABEL: &str = "baseline-dirty-tracked-patch";
pub const DIRTY_UNTRACKED_LABEL: &str = "baseline-dirty-untracked-archive";

pub async fn heikas_home(context: &NodeContext<'_>) -> ApplicationResult<PathBuf> {
    Ok(context.services().host.facts().await?.heikas_home)
}

pub async fn resolve_worktree(
    context: &NodeContext<'_>,
    relative_path: &str,
) -> ApplicationResult<PathBuf> {
    Ok(heikas_home(context).await?.join(relative_path))
}

pub async fn candidate_worktree(
    context: &NodeContext<'_>,
    candidate: &CandidateId,
) -> ApplicationResult<PathBuf> {
    let relative = context
        .projection
        .candidate(candidate)
        .map(|record| record.worktree_relative_path.clone())
        .ok_or_else(|| ApplicationError::CandidateNotFound {
            run: context.run.run_id,
            candidate: candidate.clone(),
        })?;
    resolve_worktree(context, &relative).await
}

pub fn candidate_worktree_relative(run_id: heikas_domain::identity::RunId, candidate: &CandidateId) -> String {
    format!("worktrees/{run_id}/{candidate}")
}

pub fn integration_worktree_relative(run_id: heikas_domain::identity::RunId) -> String {
    format!("worktrees/{run_id}/integration")
}

pub async fn integration_worktree(context: &NodeContext<'_>) -> ApplicationResult<PathBuf> {
    resolve_worktree(context, &integration_worktree_relative(context.run.run_id)).await
}

pub fn baseline(context: &NodeContext<'_>) -> ApplicationResult<CommitHash> {
    context
        .projection
        .baseline_commit
        .clone()
        .ok_or_else(|| ApplicationError::Internal("the run has no resolved baseline commit".to_string()))
}

pub async fn approved_plan(context: &NodeContext<'_>) -> ApplicationResult<(String, ContentDigest)> {
    let current = context
        .projection
        .plan
        .current()
        .ok_or_else(|| ApplicationError::ApprovalRequired("no plan version exists".to_string()))?;
    let hash = context
        .projection
        .plan
        .approved_hash()
        .ok_or_else(|| {
            ApplicationError::ApprovalRequired("the current plan version is not approved".to_string())
        })?
        .clone();
    let markdown = context
        .services()
        .store
        .read_version(context.run.run_id, current.version)
        .await?;
    Ok((markdown, hash))
}

pub async fn load_dirty_snapshot(
    context: &NodeContext<'_>,
) -> ApplicationResult<Option<DirtySnapshot>> {
    if !context.projection.dirty_snapshot {
        return Ok(None);
    }
    let key = crate::model::attempt::AttemptKey::new(
        heikas_domain::graph::NodeId::Prepare,
        None,
        heikas_domain::identity::AttemptNumber::FIRST,
    );
    let result = context
        .services()
        .store
        .read_attempt_result(context.run.run_id, &key)
        .await?
        .ok_or_else(|| {
            ApplicationError::Internal("the prepare attempt result is missing".to_string())
        })?;
    let tracked = find_artifact(&result.artifacts, DIRTY_TRACKED_LABEL);
    let untracked = find_artifact(&result.artifacts, DIRTY_UNTRACKED_LABEL);
    let Some(tracked) = tracked else {
        return Ok(None);
    };
    let tracked_patch = context
        .services()
        .store
        .read_artifact(context.run.run_id, &tracked.id)
        .await?;
    let untracked_archive = match untracked {
        Some(reference) => {
            context
                .services()
                .store
                .read_artifact(context.run.run_id, &reference.id)
                .await?
        }
        None => Vec::new(),
    };
    Ok(Some(DirtySnapshot {
        tracked_patch,
        untracked_archive,
        untracked_paths: Vec::new(),
    }))
}

fn find_artifact<'a>(
    artifacts: &'a [ArtifactReference],
    label: &str,
) -> Option<&'a ArtifactReference> {
    artifacts.iter().find(|artifact| artifact.label == label)
}

pub fn worktree_role_branch(
    context: &NodeContext<'_>,
    role: WorktreeRole,
    candidate: Option<&CandidateId>,
) -> String {
    let short = context.run.run_id.short();
    match (role, candidate) {
        (WorktreeRole::Candidate, Some(candidate)) => {
            format!("heikas/work-{}/{}", &short[..8], candidate)
        }
        (WorktreeRole::Integration, _) => format!("heikas/integration-{}", &short[..8]),
        _ => format!("heikas/source-{}", &short[..8]),
    }
}

pub async fn plan_expected_files(context: &NodeContext<'_>) -> ApplicationResult<Vec<String>> {
    let Some(current) = context.projection.plan.current() else {
        return Ok(Vec::new());
    };
    let markdown = context
        .services()
        .store
        .read_version(context.run.run_id, current.version)
        .await
        .unwrap_or_default();
    Ok(heikas_domain::plan::validate_plan_document(&markdown).expected_files)
}

pub fn truncate_for_prompt(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(limit).collect();
    truncated.push_str("\n[evidence truncated]");
    truncated
}
