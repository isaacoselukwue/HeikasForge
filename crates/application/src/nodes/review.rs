use heikas_domain::candidate::CandidateStatus;
use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::node::StatePatch;
use heikas_domain::review::{AggregatedReview, IssueSeverity};
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{baseline, candidate_worktree};
use crate::nodes::test::store_artifacts;
use crate::ports::quality::GateContext;

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let candidate_id = context
        .candidate_id()
        .ok_or_else(|| ApplicationError::Internal("the review node requires a candidate".to_string()))?
        .clone();
    let worktree = candidate_worktree(context, &candidate_id).await?;
    let baseline_commit = baseline(context)?;
    let changed_paths = services
        .git
        .changed_paths(&worktree, &baseline_commit)
        .await
        .unwrap_or_default();

    let plan_expected_files = crate::nodes::support::plan_expected_files(context).await?;
    let gate_context = GateContext {
        run_id: context.run.run_id,
        candidate_id: Some(candidate_id.clone()),
        worktree,
        repository: configuration.repository_path.clone(),
        baseline: baseline_commit,
        changed_paths,
        plan_expected_files: plan_expected_files.clone(),
        configuration: configuration.clone(),
        cancellation: context.run.cancellation.clone(),
    };

    let (review, artifacts, duration) = run_providers(context, &gate_context).await?;
    let stored_artifacts = store_artifacts(context, artifacts).await?;

    services
        .store
        .write_review(
            context.run.run_id,
            Some(&candidate_id),
            context.attempt,
            &review,
        )
        .await?;

    let passed = review.required_reports_passed() && review.has_required_provider();
    let failed_providers: Vec<String> = review
        .failed_required_providers()
        .into_iter()
        .map(str::to_string)
        .collect();

    let event = EventPayload::ReviewEvidenceRecorded {
        candidate_id: Some(candidate_id.clone()),
        node_id: NodeId::ReviewCandidate,
        passed,
        providers: review
            .reports
            .iter()
            .map(|report| report.provider.clone())
            .collect(),
        failed_providers: failed_providers.clone(),
        blocker_issues: review.issue_count(IssueSeverity::Blocker),
        duration,
    };

    let input = json!({
        "candidate": candidate_id.as_str(),
        "attempt": context.attempt.get(),
        "providers": configuration.review_provider_names(),
    });
    let attempt_evidence = AttemptEvidence::with_input(input).with_streams(
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
        "critical_issues": review.issue_count(IssueSeverity::Critical),
        "high_issues": review.issue_count(IssueSeverity::High),
        "duration_ms": duration.millis(),
    });

    if passed {
        return Ok(NodeOutput::succeeded(Some(NodeId::Join))
            .with_event(event)
            .with_artifacts(stored_artifacts)
            .with_patch(StatePatch {
                candidate_status: Some(CandidateStatus::Eligible),
                ..StatePatch::default()
            })
            .with_metrics(metrics)
            .with_evidence(attempt_evidence));
    }

    let fingerprint = review_fingerprint(&review);
    let failure = NodeFailure::new(
        FailureClass::TaskFailure,
        "required_review_failed",
        format!(
            "required review providers failed: {}",
            if failed_providers.is_empty() {
                "no required provider produced a report".to_string()
            } else {
                failed_providers.join(", ")
            }
        ),
    )
    .with_fingerprint(fingerprint);

    Ok(NodeOutput::failed(failure, None)
        .with_event(event)
        .with_artifacts(stored_artifacts)
        .with_metrics(metrics)
        .with_evidence(attempt_evidence))
}

pub async fn run_providers(
    context: &NodeContext<'_>,
    gate_context: &GateContext,
) -> ApplicationResult<(
    AggregatedReview,
    Vec<crate::ports::quality::GateArtifact>,
    heikas_domain::clock::DurationMs,
)> {
    let mut review = AggregatedReview::default();
    let mut artifacts = Vec::new();
    let mut duration = heikas_domain::clock::DurationMs::ZERO;
    for provider in &context.services().reviews {
        if !provider.available().await? {
            if provider.required() {
                return Err(ApplicationError::QualityProvider(format!(
                    "the required review provider `{}` is not available",
                    provider.name()
                )));
            }
            continue;
        }
        let output = provider.review(gate_context).await?;
        output.report.validate()?;
        duration = duration.saturating_add(
            output
                .report
                .finished_at
                .duration_since(output.report.started_at),
        );
        review.reports.push(output.report);
        artifacts.extend(output.artifacts);
    }
    Ok((review, artifacts, duration))
}

fn review_fingerprint(review: &AggregatedReview) -> String {
    let mut material = String::new();
    for report in review.reports.iter().filter(|report| !report.passed) {
        material.push_str(&report.provider);
        for issue in &report.issues {
            material.push('\u{1f}');
            material.push_str(&issue.fingerprint);
        }
        material.push('\n');
    }
    heikas_domain::identity::ContentDigest::of_str(&material)
        .short()
        .to_string()
}
