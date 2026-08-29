use heikas_domain::event::EventPayload;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::CandidateId;
use heikas_domain::node::StatePatch;
use heikas_domain::review::IssueSeverity;
use heikas_domain::run::RunStatus;
use heikas_domain::score::ExclusionReason;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::integrate::next_promotable;
use crate::nodes::review::run_providers;
use crate::nodes::support::{baseline, integration_worktree};
use crate::nodes::test::store_artifacts;
use crate::ports::quality::GateContext;

pub async fn final_test(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let worktree = integration_worktree(context).await?;
    let baseline_commit = baseline(context)?;
    let winner = winner_of(context)?;

    let commands: Vec<_> = configuration
        .commands
        .test_phase()
        .into_iter()
        .cloned()
        .collect();
    let changed_paths = services
        .git
        .changed_paths(&worktree, &baseline_commit)
        .await
        .unwrap_or_default();
    let plan_expected_files = crate::nodes::support::plan_expected_files(context).await?;
    let gate_context = GateContext {
        run_id: context.run.run_id,
        candidate_id: None,
        worktree,
        repository: configuration.repository_path.clone(),
        baseline: baseline_commit,
        changed_paths,
        plan_expected_files: plan_expected_files.clone(),
        configuration: configuration.clone(),
        cancellation: context.run.cancellation.clone(),
    };

    let output = services.tests.run_tests(&gate_context, &commands).await?;
    let mut evidence_bundle = output.evidence;
    evidence_bundle.recompute_totals();
    services
        .store
        .write_test_evidence(context.run.run_id, None, context.attempt, &evidence_bundle)
        .await?;
    let artifacts = store_artifacts(context, output.artifacts).await?;

    let passed = evidence_bundle.all_required_passed();
    let failed_commands: Vec<String> = evidence_bundle
        .failed_required()
        .into_iter()
        .map(|(command, _)| command)
        .collect();

    let mut events = vec![EventPayload::TestEvidenceRecorded {
        candidate_id: None,
        node_id: NodeId::FinalTest,
        passed,
        commands: commands
            .iter()
            .map(|command| command.id.to_string())
            .collect(),
        failed_commands: failed_commands.clone(),
        line_coverage_percent: evidence_bundle.line_coverage_percent,
        duration: evidence_bundle.total_duration,
    }];

    let attempt_evidence = AttemptEvidence::with_input(json!({
        "phase": "final_test",
        "winner": winner.as_str(),
        "commands": commands.iter().map(|command| command.id.to_string()).collect::<Vec<_>>(),
    }))
    .with_streams(
        services
            .redactor
            .redact_text(&serde_json::to_string_pretty(&evidence_bundle)?)
            .into_bytes(),
        Vec::new(),
    );

    let metrics = json!({
        "passed": passed,
        "failed_commands": failed_commands,
        "line_coverage_percent": evidence_bundle.line_coverage_percent,
    });

    if passed {
        return Ok(NodeOutput::succeeded(Some(NodeId::FinalReview))
            .with_events(events)
            .with_artifacts(artifacts)
            .with_metrics(metrics)
            .with_evidence(attempt_evidence));
    }

    let detail = format!(
        "the final tests failed in the integration worktree: {}",
        failed_commands.join(", ")
    );
    events.extend(promotion_events(context, &winner, &detail));
    finish_with_promotion(
        context,
        &winner,
        events,
        artifacts,
        metrics,
        attempt_evidence,
    )
}

pub async fn final_review(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let worktree = integration_worktree(context).await?;
    let baseline_commit = baseline(context)?;
    let winner = winner_of(context)?;
    let changed_paths = services
        .git
        .changed_paths(&worktree, &baseline_commit)
        .await
        .unwrap_or_default();

    let plan_expected_files = crate::nodes::support::plan_expected_files(context).await?;
    let gate_context = GateContext {
        run_id: context.run.run_id,
        candidate_id: None,
        worktree,
        repository: configuration.repository_path.clone(),
        baseline: baseline_commit,
        changed_paths,
        plan_expected_files: plan_expected_files.clone(),
        configuration: configuration.clone(),
        cancellation: context.run.cancellation.clone(),
    };

    let (review, artifacts, duration) = run_providers(context, &gate_context).await?;
    let stored = store_artifacts(context, artifacts).await?;
    services
        .store
        .write_review(context.run.run_id, None, context.attempt, &review)
        .await?;

    let passed = review.required_reports_passed() && review.has_required_provider();
    let failed_providers: Vec<String> = review
        .failed_required_providers()
        .into_iter()
        .map(str::to_string)
        .collect();

    let mut events = vec![EventPayload::ReviewEvidenceRecorded {
        candidate_id: None,
        node_id: NodeId::FinalReview,
        passed,
        providers: review
            .reports
            .iter()
            .map(|report| report.provider.clone())
            .collect(),
        failed_providers: failed_providers.clone(),
        blocker_issues: review.issue_count(IssueSeverity::Blocker),
        duration,
    }];

    let attempt_evidence = AttemptEvidence::with_input(json!({
        "phase": "final_review",
        "winner": winner.as_str(),
        "providers": configuration.review_provider_names(),
    }))
    .with_streams(
        services
            .redactor
            .redact_text(&serde_json::to_string_pretty(&review)?)
            .into_bytes(),
        Vec::new(),
    );

    let metrics = json!({
        "passed": passed,
        "failed_providers": failed_providers,
        "blocker_issues": review.issue_count(IssueSeverity::Blocker),
    });

    if passed {
        return Ok(NodeOutput::succeeded(Some(NodeId::CommitApproval))
            .with_events(events)
            .with_artifacts(stored)
            .with_metrics(metrics)
            .with_evidence(attempt_evidence));
    }

    let detail = format!(
        "the final review failed in the integration worktree: {}",
        failed_providers.join(", ")
    );
    events.extend(promotion_events(context, &winner, &detail));
    finish_with_promotion(context, &winner, events, stored, metrics, attempt_evidence)
}

fn winner_of(context: &NodeContext<'_>) -> ApplicationResult<CandidateId> {
    context.projection.winner.clone().ok_or_else(|| {
        ApplicationError::Internal("no winner is selected for the final gates".to_string())
    })
}

fn promotion_events(
    context: &NodeContext<'_>,
    winner: &CandidateId,
    detail: &str,
) -> Vec<EventPayload> {
    let next = next_promotable(context, winner);
    vec![
        EventPayload::CandidateExcluded {
            candidate_id: winner.clone(),
            reasons: vec![ExclusionReason::IntegrationFailed {
                detail: detail.to_string(),
            }],
        },
        EventPayload::CandidatePromotionRequested {
            previous_candidate_id: winner.clone(),
            next_candidate_id: next,
            reason: detail.to_string(),
        },
    ]
}

fn finish_with_promotion(
    context: &NodeContext<'_>,
    winner: &CandidateId,
    events: Vec<EventPayload>,
    artifacts: Vec<heikas_domain::node::ArtifactReference>,
    metrics: serde_json::Value,
    evidence: AttemptEvidence,
) -> ApplicationResult<NodeOutput> {
    match next_promotable(context, winner) {
        Some(_) => Ok(NodeOutput::succeeded(Some(NodeId::IntegrateWinner))
            .with_events(events)
            .with_artifacts(artifacts)
            .with_metrics(metrics)
            .with_evidence(evidence)),
        None => Ok(NodeOutput::succeeded(None)
            .with_events(events)
            .with_artifacts(artifacts)
            .with_patch(StatePatch {
                run_status: Some(RunStatus::Exhausted),
                ..StatePatch::default()
            })
            .with_metrics(metrics)
            .with_evidence(evidence)
            .with_warning("no further candidate could be promoted after a final gate failure")),
    }
}
