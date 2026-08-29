use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::node::ArtifactReference;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{baseline, candidate_worktree};
use crate::ports::quality::{GateArtifact, GateContext};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let candidate_id = context
        .candidate_id()
        .ok_or_else(|| ApplicationError::Internal("the test node requires a candidate".to_string()))?
        .clone();
    let worktree = candidate_worktree(context, &candidate_id).await?;
    let baseline_commit = baseline(context)?;

    let commands: Vec<_> = configuration
        .commands
        .test_phase()
        .into_iter()
        .cloned()
        .collect();

    let input = json!({
        "candidate": candidate_id.as_str(),
        "attempt": context.attempt.get(),
        "commands": commands.iter().map(|command| command.id.to_string()).collect::<Vec<_>>(),
    });

    let changed_paths = services
        .git
        .changed_paths(&worktree, &baseline_commit)
        .await
        .unwrap_or_default();

    let plan_expected_files = crate::nodes::support::plan_expected_files(context).await?;
    let gate_context = GateContext {
        run_id: context.run.run_id,
        candidate_id: Some(candidate_id.clone()),
        worktree: worktree.clone(),
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
        .write_test_evidence(
            context.run.run_id,
            Some(&candidate_id),
            context.attempt,
            &evidence_bundle,
        )
        .await?;

    let artifacts = store_artifacts(context, output.artifacts).await?;
    let passed = evidence_bundle.all_required_passed();
    let failed_commands: Vec<String> = evidence_bundle
        .failed_required()
        .into_iter()
        .map(|(command, _)| command)
        .collect();

    let event = EventPayload::TestEvidenceRecorded {
        candidate_id: Some(candidate_id.clone()),
        node_id: NodeId::TestCandidate,
        passed,
        commands: commands.iter().map(|command| command.id.to_string()).collect(),
        failed_commands: failed_commands.clone(),
        line_coverage_percent: evidence_bundle.line_coverage_percent,
        duration: evidence_bundle.total_duration,
    };

    let attempt_evidence = AttemptEvidence::with_input(input).with_streams(
        services
            .redactor
            .redact_text(&serde_json::to_string_pretty(&evidence_bundle)?)
            .into_bytes(),
        Vec::new(),
    );

    let metrics = json!({
        "passed": passed,
        "commands": commands.len(),
        "failed_commands": failed_commands,
        "line_coverage_percent": evidence_bundle.line_coverage_percent,
        "duration_ms": evidence_bundle.total_duration.millis(),
    });

    if passed {
        return Ok(NodeOutput::succeeded(Some(NodeId::ReviewCandidate))
            .with_event(event)
            .with_artifacts(artifacts)
            .with_metrics(metrics)
            .with_evidence(attempt_evidence));
    }

    let fingerprint = evidence_bundle.failure_fingerprint();
    let mut failure = NodeFailure::new(
        FailureClass::TaskFailure,
        "required_tests_failed",
        format!(
            "required test commands failed: {}",
            failed_commands.join(", ")
        ),
    );
    if let Some(fingerprint) = fingerprint {
        failure = failure.with_fingerprint(fingerprint);
    }

    Ok(NodeOutput::failed(failure, None)
        .with_event(event)
        .with_artifacts(artifacts)
        .with_metrics(metrics)
        .with_evidence(attempt_evidence))
}

pub async fn store_artifacts(
    context: &NodeContext<'_>,
    artifacts: Vec<GateArtifact>,
) -> ApplicationResult<Vec<ArtifactReference>> {
    let mut references = Vec::new();
    for artifact in artifacts {
        let redacted = context.services().redactor.redact_bytes(&artifact.bytes);
        let stored = context
            .services()
            .store
            .store_artifact(
                context.run.run_id,
                &artifact.label,
                &artifact.relative_path,
                &redacted,
                artifact.truncated,
            )
            .await?;
        references.push(stored.to_reference());
    }
    Ok(references)
}
