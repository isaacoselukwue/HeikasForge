use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::identity::ContentDigest;
use heikas_domain::node::StatePatch;
use heikas_domain::plan::{validate_plan_document, PlanAuthor};
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::ApplicationResult;
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::truncate_for_prompt;
use crate::ports::agent::{AgentInvocation, AgentRole, ToolPolicy};
use crate::prompt::{PromptFacts, PromptLibrary};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let repository = configuration.repository_path.clone();

    let paths = services.git.list_paths(&repository, 400).await?;
    let repository_summary = build_repository_summary(context, &paths);

    let revision_note = context
        .projection
        .plan
        .approval
        .as_ref()
        .filter(|approval| {
            approval.decision == heikas_domain::plan::ApprovalDecision::RevisionRequested
        })
        .and_then(|approval| approval.note.clone());

    let mut previous_evidence = Vec::new();
    if let Some(note) = &revision_note {
        previous_evidence.push(format!("Requested revision: {note}"));
    }
    if let Some(current) = context.projection.plan.current() {
        let previous = services
            .store
            .read_version(context.run.run_id, current.version)
            .await
            .unwrap_or_default();
        previous_evidence.push(format!(
            "Previous plan version {} follows:\n{}",
            current.version,
            truncate_for_prompt(&previous, 6_000)
        ));
    }

    let facts = PromptFacts {
        task_title: context.run.task_title(),
        task_body: context.run.task_markdown.clone(),
        repository_summary,
        approved_plan_hash: None,
        approved_plan: None,
        strategy: None,
        strategy_emphasis: None,
        allowed_commands: configuration
            .commands
            .commands
            .iter()
            .map(|command| format!("{} ({})", command.id, command.kind.label()))
            .collect(),
        protected_paths: configuration.path_policy.protected_patterns.clone(),
        previous_evidence,
        expected_files: Vec::new(),
        attempt: context.attempt.get(),
    };

    let prompt = PromptLibrary::render(AgentRole::Planner, &facts)?;
    let prompt_hash = prompt.template_hash.clone();
    let input = json!({
        "role": AgentRole::Planner.as_str(),
        "attempt": context.attempt.get(),
        "prompt_template_id": prompt.template_id,
        "prompt_template_version": prompt.template_version,
        "prompt_template_hash": prompt_hash.as_str(),
        "revision_note": revision_note,
    });

    let invocation = AgentInvocation {
        run_id: context.run.run_id,
        candidate_id: None,
        role: AgentRole::Planner,
        strategy: None,
        worktree: repository.clone(),
        prompt: prompt.clone(),
        tool_policy: ToolPolicy::read_only(
            configuration.path_policy.clone(),
            configuration.budgets.max_agent_turns.saturating_mul(4),
        ),
        commands: Vec::new(),
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

    if !outcome.completed() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "planning_incomplete",
                format!(
                    "the planning agent stopped with reason `{}`",
                    outcome.exit_reason.as_str()
                ),
            )
            .with_remedy("Review the agent diagnostics and retry planning."),
            None,
        )
        .with_evidence(evidence));
    }

    if !outcome.changed_paths.is_empty() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::PolicyViolation,
                "plan_node_modified_files",
                format!(
                    "the planning node is read-only but {} paths changed",
                    outcome.changed_paths.len()
                ),
            ),
            None,
        )
        .with_evidence(evidence));
    }

    let Some(response) = outcome.structured_response.as_ref() else {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "planning_response_missing",
                "the planning agent returned no structured completion",
            ),
            None,
        )
        .with_evidence(evidence));
    };

    let markdown = response
        .get("plan_markdown")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    let validation = validate_plan_document(&markdown);
    if !validation.is_acceptable() {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::TaskFailure,
                "plan_missing_required_headings",
                format!(
                    "the plan is missing the required headings: {}",
                    validation.missing_headings.join(", ")
                ),
            )
            .with_remedy("Ask the planning agent for a revision that includes every required heading."),
            None,
        )
        .with_evidence(evidence));
    }

    let version_number = context.projection.plan.next_version_number();
    let recorded_at = services.clock.now();
    let version = services
        .store
        .write_version(
            context.run.run_id,
            version_number,
            &markdown,
            PlanAuthor::Agent,
            revision_note.clone(),
            recorded_at,
        )
        .await?;

    let mut warnings = Vec::new();
    for section in &validation.empty_sections {
        warnings.push(format!("the plan section `{section}` is empty"));
    }

    let mut output = NodeOutput::succeeded(Some(NodeId::Approval))
        .with_patch(StatePatch {
            plan_version: Some(version.version),
            ..StatePatch::default()
        })
        .with_event(EventPayload::PlanVersionWritten {
            version: version.version,
            plan_hash: version.hash.clone(),
            author: PlanAuthor::Agent,
            revision_note,
            byte_length: version.byte_length,
        })
        .with_metrics(json!({
            "expected_files": validation.expected_files,
            "plan_bytes": version.byte_length,
            "model": outcome.model_identity,
            "tool_calls": outcome.tool_calls.len(),
            "prompt_template_hash": prompt_hash.as_str(),
        }))
        .with_evidence(evidence);
    for warning in warnings {
        output = output.with_warning(warning);
    }
    Ok(output)
}

fn build_repository_summary(context: &NodeContext<'_>, paths: &[String]) -> String {
    let mut summary = String::new();
    summary.push_str(&format!(
        "Repository root: {}\n",
        context.configuration().repository_path.display()
    ));
    if let Some(baseline) = &context.projection.baseline_commit {
        summary.push_str(&format!("Baseline commit: {}\n", baseline));
    }
    if let Some(branch) = &context.projection.default_branch {
        summary.push_str(&format!("Default branch: {branch}\n"));
    }
    summary.push_str(&format!("Tracked paths sampled: {}\n", paths.len()));
    summary.push_str("Paths:\n");
    for path in paths.iter().take(300) {
        summary.push_str(&format!("- {path}\n"));
    }
    summary
}

pub fn plan_hash_of(markdown: &str) -> ContentDigest {
    ContentDigest::of_str(markdown)
}
