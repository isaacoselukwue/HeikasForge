use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::node::StatePatch;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{approved_plan, baseline, candidate_worktree};
use crate::ports::agent::{AgentInvocation, AgentRole, ToolPolicy};
use crate::prompt::{strategy_facts, PromptFacts, PromptLibrary};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let candidate_id = context
        .candidate_id()
        .ok_or_else(|| {
            ApplicationError::Internal("the implementation node requires a candidate".to_string())
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
    let worktree = candidate_worktree(context, &candidate_id).await?;
    let baseline_commit = baseline(context)?;
    let (plan_markdown, plan_hash) = approved_plan(context).await?;
    let expected_files = heikas_domain::plan::validate_plan_document(&plan_markdown).expected_files;
    let (strategy, emphasis) = strategy_facts(record.strategy);

    let facts = PromptFacts {
        task_title: context.run.task_title(),
        task_body: context.run.task_markdown.clone(),
        repository_summary: format!(
            "Candidate worktree baseline: {}\nCandidate identifier: {}\nRepair budget: {}\n",
            baseline_commit, candidate_id, record.repair_budget
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
        previous_evidence: Vec::new(),
        expected_files,
        attempt: context.attempt.get(),
    };

    let prompt = PromptLibrary::render(AgentRole::Implementer, &facts)?;
    let prompt_hash = prompt.template_hash.clone();
    let input = json!({
        "role": AgentRole::Implementer.as_str(),
        "candidate": candidate_id.as_str(),
        "strategy": record.strategy.as_str(),
        "attempt": context.attempt.get(),
        "approved_plan_hash": plan_hash.as_str(),
        "prompt_template_hash": prompt_hash.as_str(),
    });

    let invocation = AgentInvocation {
        run_id: context.run.run_id,
        candidate_id: Some(candidate_id.clone()),
        role: AgentRole::Implementer,
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

    let outcome = services.agent.invoke(invocation).await?;
    let evidence = AttemptEvidence::with_input(input)
        .with_invocation(json!({
            "driver": outcome.driver.as_str(),
            "model": outcome.model_identity,
            "exit_reason": outcome.exit_reason.as_str(),
            "tool_calls": outcome.tool_calls,
            "usage": outcome.usage,
            "prompt_template_hash": prompt_hash.as_str(),
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

    let diff_event = EventPayload::CandidateDiffRecorded {
        candidate_id: candidate_id.clone(),
        diff_digest: digest.clone(),
        changed_files: summary.changed_files,
        changed_lines: summary.changed_lines(),
    };

    if !outcome.completed() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "implementation_incomplete",
                format!(
                    "the implementation agent stopped with reason `{}`",
                    outcome.exit_reason.as_str()
                ),
            ),
            None,
        )
        .with_event(diff_event)
        .with_patch(StatePatch {
            diff_digest: Some(digest),
            changed_files: Some(summary.changed_files),
            changed_lines: Some(summary.changed_lines()),
            ..StatePatch::default()
        })
        .with_evidence(evidence));
    }

    let mut output = NodeOutput::succeeded(Some(NodeId::TestCandidate))
        .with_event(diff_event)
        .with_patch(StatePatch {
            diff_digest: Some(digest),
            changed_files: Some(summary.changed_files),
            changed_lines: Some(summary.changed_lines()),
            ..StatePatch::default()
        })
        .with_metrics(json!({
            "changed_files": summary.changed_files,
            "added_lines": summary.added_lines,
            "removed_lines": summary.removed_lines,
            "tool_calls": outcome.tool_calls.len(),
            "model": outcome.model_identity,
        }))
        .with_evidence(evidence);

    if summary.is_empty {
        output = output.with_warning("the candidate produced no change against the baseline");
    }
    Ok(output)
}
