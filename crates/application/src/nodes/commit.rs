use std::str::FromStr;

use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::identity::BranchName;
use heikas_domain::node::StatePatch;
use heikas_domain::path_policy::{
    evaluate_path, GlobPatternMatcher, PathAccess, RelativeWorkspacePath,
};
use heikas_domain::run::RunStatus;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{baseline, integration_worktree};
use crate::ports::git::CommitRequest;

const MAXIMUM_SUBJECT_LENGTH: usize = 72;

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let worktree = integration_worktree(context).await?;
    let baseline_commit = baseline(context)?;
    let winner = context.projection.winner.clone().ok_or_else(|| {
        ApplicationError::Internal("no winner is selected for the commit".to_string())
    })?;

    let facts = services.git.inspect(&configuration.repository_path).await?;
    let evidence = AttemptEvidence::with_input(json!({
        "winner": winner.as_str(),
        "commit_policy": configuration.commit_policy.as_str(),
        "author_name": configuration.git.author_name,
    }));

    let Some(email) = facts.configured_user_email.clone() else {
        return Ok(NodeOutput::paused().with_evidence(evidence).with_warning(
            "the repository has no configured Git email, so no commit identity can be derived",
        ));
    };
    if email.trim().is_empty() {
        return Ok(NodeOutput::paused().with_evidence(evidence).with_warning(
            "the configured Git email is empty, so no commit identity can be derived",
        ));
    }

    let changed_paths = services
        .git
        .changed_paths(&worktree, &baseline_commit)
        .await?;
    if changed_paths.is_empty() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "empty_integration_diff",
                "the integration worktree contains no change to commit",
            ),
            None,
        )
        .with_evidence(evidence));
    }

    for path in &changed_paths {
        let relative = RelativeWorkspacePath::parse(path)?;
        if let Err(violation) = evaluate_path(
            &configuration.path_policy,
            &GlobPatternMatcher,
            &relative,
            PathAccess::Write,
        ) {
            return Ok(NodeOutput::failed(
                NodeFailure::new(
                    FailureClass::PolicyViolation,
                    "protected_path_in_commit",
                    format!("the integration diff may not include `{path}`: {violation}"),
                ),
                None,
            )
            .with_evidence(evidence));
        }
    }

    let branch_text = format!(
        "{}{}",
        configuration.git.branch_prefix,
        &context.run.run_id.short()[..8]
    );
    let branch = BranchName::from_str(&branch_text)?;
    let subject = commit_subject(&context.run.task_title());
    let body = commit_body(context, &changed_paths);

    let request = CommitRequest {
        worktree: worktree.clone(),
        branch: branch.clone(),
        paths: changed_paths.clone(),
        subject,
        body,
        author_name: configuration.git.author_name.clone(),
        committer_name: configuration.git.author_name.clone(),
    };

    let outcome = match services.git.create_commit(&request).await {
        Ok(outcome) => outcome,
        Err(ApplicationError::UserActionRequired(detail)) => {
            return Ok(NodeOutput::paused()
                .with_evidence(evidence)
                .with_warning(detail));
        }
        Err(error) => return Err(error),
    };

    Ok(NodeOutput::succeeded(None)
        .with_event(EventPayload::CommitCreated {
            branch: outcome.branch.clone(),
            commit_hash: outcome.commit_hash.clone(),
            author_name: configuration.git.author_name.clone(),
            committer_name: configuration.git.author_name.clone(),
            changed_files: outcome.changed_files,
            signed: outcome.signed,
        })
        .with_patch(StatePatch {
            run_status: Some(RunStatus::Succeeded),
            commit_hash: Some(outcome.commit_hash.clone()),
            branch: Some(outcome.branch.clone()),
            ..StatePatch::default()
        })
        .with_metrics(json!({
            "commit": outcome.commit_hash.as_str(),
            "branch": outcome.branch.as_str(),
            "changed_files": outcome.changed_files,
            "signed": outcome.signed,
        }))
        .with_evidence(evidence))
}

pub fn commit_subject(task_title: &str) -> String {
    let cleaned = task_title
        .trim()
        .trim_end_matches('.')
        .replace(['\n', '\r'], " ");
    let imperative = to_imperative(&cleaned);
    let mut subject: String = imperative.chars().take(MAXIMUM_SUBJECT_LENGTH).collect();
    if subject.is_empty() {
        subject.push_str("Apply the approved implementation plan");
    }
    subject
}

fn to_imperative(title: &str) -> String {
    let lowered = title.to_lowercase();
    let stripped = lowered
        .strip_prefix("please ")
        .or_else(|| lowered.strip_prefix("can you "))
        .or_else(|| lowered.strip_prefix("we need to "))
        .or_else(|| lowered.strip_prefix("i want to "))
        .unwrap_or(&lowered);
    let mut characters = stripped.chars();
    match characters.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), characters.as_str()),
        None => String::new(),
    }
}

fn commit_body(context: &NodeContext<'_>, changed_paths: &[String]) -> String {
    let mut body = String::new();
    body.push_str("Validated changes:\n");
    for path in changed_paths.iter().take(50) {
        body.push_str(&format!("- {path}\n"));
    }
    if changed_paths.len() > 50 {
        body.push_str(&format!(
            "- and {} further paths\n",
            changed_paths.len() - 50
        ));
    }
    body.push_str("\nGates satisfied:\n");
    for command in context.configuration().required_commands() {
        body.push_str(&format!("- {} ({})\n", command.id, command.kind.label()));
    }
    for provider in context.configuration().review_provider_names() {
        body.push_str(&format!("- review provider {provider}\n"));
    }
    if let Some(plan) = context.projection.plan.current() {
        body.push_str(&format!("\nApproved plan version {}.\n", plan.version));
    }
    body
}
