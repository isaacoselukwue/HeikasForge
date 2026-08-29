use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::node::StatePatch;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{approved_plan, baseline, candidate_worktree, truncate_for_prompt};
use crate::ports::agent::{AgentInvocation, AgentRole, ToolPolicy};
use crate::prompt::{strategy_facts, PromptFacts, PromptLibrary};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let candidate_id = context
        .candidate_id()
        .ok_or_else(|| {
            ApplicationError::Internal("the repair node requires a candidate".to_string())
        })?
        .clone();
    let record = context
        .projection
        .candidate(&candidate_id)
        .ok_or_else(|| ApplicationError::CandidateNotFound {
            run: context.run.run_id,
            candidate: candidate_id.clone(),
        })?
        .clone();

    if !record.has_repair_budget() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "repair_budget_exhausted",
                format!(
                    "candidate {candidate_id} used {} of {} repair attempts",
                    record.repairs_used, record.repair_budget
                ),
            ),
            Some(NodeId::Join),
        )
        .with_patch(StatePatch {
            candidate_status: Some(heikas_domain::candidate::CandidateStatus::Ineligible),
            exclusion_reasons: Some(vec![
                heikas_domain::score::ExclusionReason::RepairBudgetExhausted {
                    used: record.repairs_used,
                    budget: record.repair_budget,
                },
            ]),
            ..StatePatch::default()
        }));
    }

    let worktree = candidate_worktree(context, &candidate_id).await?;
    let baseline_commit = baseline(context)?;
    let (plan_markdown, plan_hash) = approved_plan(context).await?;
    let (strategy, emphasis) = strategy_facts(record.strategy);

    let evidence_lines = collect_evidence(context, &candidate_id).await?;
    let fingerprint = latest_failure_fingerprint(context, &candidate_id);
    let repairs_used = record.repairs_used.saturating_add(1);

    let facts = PromptFacts {
        task_title: context.run.task_title(),
        task_body: context.run.task_markdown.clone(),
        repository_summary: format!(
            "Candidate identifier: {candidate_id}\nRepair attempt {repairs_used} of {}\n",
            record.repair_budget
        ),
        approved_plan_hash: Some(plan_hash.to_string()),
        approved_plan: Some(plan_markdown),
        strategy: Some(strategy),
        strategy_emphasis: Some(emphasis),
        allowed_commands: configuration
            .commands
            .commands
            .iter()
            .map(|command| format!("{} ({})", command.id, command.kind.label()))
            .collect(),
        protected_paths: configuration.path_policy.protected_patterns.clone(),
        previous_evidence: evidence_lines,
        expected_files: Vec::new(),
        attempt: repairs_used,
    };

    let prompt = PromptLibrary::render(AgentRole::Repairer, &facts)?;
    let prompt_hash = prompt.template_hash.clone();
    let input = json!({
        "role": AgentRole::Repairer.as_str(),
        "candidate": candidate_id.as_str(),
        "repairs_used": repairs_used,
        "repair_budget": record.repair_budget,
        "failure_fingerprint": fingerprint,
        "prompt_template_hash": prompt_hash.as_str(),
    });

    let invocation = AgentInvocation {
        run_id: context.run.run_id,
        candidate_id: Some(candidate_id.clone()),
        role: AgentRole::Repairer,
        strategy: Some(record.strategy),
        worktree: worktree.clone(),
        prompt,
        tool_policy: ToolPolicy::editing(
            configuration.path_policy.clone(),
            configuration
                .commands
                .commands
                .iter()
                .map(|command| command.id.clone())
                .collect(),
            configuration.budgets.max_agent_turns.saturating_mul(8),
        ),
        commands: configuration.commands.commands.clone(),
        environment_allowlist: configuration.environment_allowlist.clone(),
        network: configuration.agent.network,
        time_budget_seconds: configuration.agent.timeout.get(),
        turn_budget: configuration.budgets.max_agent_turns,
        output_budget_bytes: configuration.budgets.max_output_bytes_per_stream,
        cancellation: context.run.cancellation.clone(),
    };

    let repair_event = EventPayload::CandidateRepairStarted {
        candidate_id: candidate_id.clone(),
        repairs_used,
        repair_budget: record.repair_budget,
        failure_fingerprint: fingerprint.clone(),
    };

    let outcome = services.agent.invoke(invocation).await?;
    let attempt_evidence = AttemptEvidence::with_input(input)
        .with_invocation(json!({
            "driver": outcome.driver.as_str(),
            "model": outcome.model_identity,
            "exit_reason": outcome.exit_reason.as_str(),
            "tool_calls": outcome.tool_calls,
            "usage": outcome.usage,
        }))
        .with_streams(
            services.redactor.redact_text(&outcome.stdout).into_bytes(),
            services.redactor.redact_text(&outcome.stderr).into_bytes(),
        );

    let (patch, summary) = services
        .git
        .diff_against_baseline(&worktree, &baseline_commit)
        .await?;
    let digest = services
        .store
        .write_diff(context.run.run_id, &candidate_id, &patch)
        .await?;

    let events = vec![
        repair_event,
        EventPayload::CandidateDiffRecorded {
            candidate_id: candidate_id.clone(),
            diff_digest: digest.clone(),
            changed_files: summary.changed_files,
            changed_lines: summary.changed_lines(),
        },
    ];

    let state_patch = StatePatch {
        diff_digest: Some(digest),
        changed_files: Some(summary.changed_files),
        changed_lines: Some(summary.changed_lines()),
        repairs_used: Some(repairs_used),
        ..StatePatch::default()
    };

    if !outcome.completed() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "repair_incomplete",
                format!(
                    "the repair agent stopped with reason `{}`",
                    outcome.exit_reason.as_str()
                ),
            ),
            None,
        )
        .with_events(events)
        .with_patch(state_patch)
        .with_evidence(attempt_evidence));
    }

    Ok(NodeOutput::succeeded(Some(NodeId::TestCandidate))
        .with_events(events)
        .with_patch(state_patch)
        .with_metrics(json!({
            "repairs_used": repairs_used,
            "changed_files": summary.changed_files,
            "changed_lines": summary.changed_lines(),
            "tool_calls": outcome.tool_calls.len(),
        }))
        .with_evidence(attempt_evidence))
}

async fn collect_evidence(
    context: &NodeContext<'_>,
    candidate: &heikas_domain::identity::CandidateId,
) -> ApplicationResult<Vec<String>> {
    let mut lines = Vec::new();
    if let Some(tests) = context
        .services()
        .store
        .read_test_evidence(context.run.run_id, Some(candidate))
        .await?
    {
        for record in tests
            .commands
            .iter()
            .filter(|record| !record.outcome.is_pass())
        {
            lines.push(format!(
                "Command `{}` finished as {}: {}",
                record.command_id,
                record.outcome.as_str(),
                record.failure_summary()
            ));
            for failure in record.failures.iter().take(20) {
                lines.push(format!(
                    "Failing test {}::{} at {}:{} reported {}",
                    failure.suite,
                    failure.case,
                    failure
                        .file
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    failure.line.unwrap_or(0),
                    truncate_for_prompt(&failure.message, 600)
                ));
            }
        }
    }
    if let Some(review) = context
        .services()
        .store
        .read_review(context.run.run_id, Some(candidate))
        .await?
    {
        for report in review.reports.iter().filter(|report| !report.passed) {
            lines.push(format!(
                "Review provider `{}` failed: {}",
                report.provider,
                report
                    .failure_summary
                    .clone()
                    .unwrap_or_else(|| "the quality gate did not pass".to_string())
            ));
            for issue in report.issues.iter().take(30) {
                lines.push(format!(
                    "{} {} at {}:{} reported {}",
                    issue.severity,
                    issue.rule_id,
                    issue.file.clone().unwrap_or_else(|| "unknown".to_string()),
                    issue.line.unwrap_or(0),
                    truncate_for_prompt(&issue.message, 400)
                ));
            }
        }
    }
    if lines.is_empty() {
        lines
            .push("No structured gate evidence was recorded for the previous attempt.".to_string());
    }
    Ok(lines)
}

fn latest_failure_fingerprint(
    context: &NodeContext<'_>,
    candidate: &heikas_domain::identity::CandidateId,
) -> Option<String> {
    context
        .projection
        .attempts
        .iter()
        .filter(|attempt| attempt.candidate_id.as_ref() == Some(candidate))
        .filter(|attempt| attempt.status == heikas_domain::state::NodeAttemptStatus::Failed)
        .max_by_key(|attempt| attempt.sequence)
        .and_then(|attempt| attempt.failure_summary.clone())
        .map(|summary| {
            heikas_domain::identity::ContentDigest::of_str(&summary)
                .short()
                .to_string()
        })
}
