use heikas_domain::event::EventPayload;
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::node::StatePatch;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::ApplicationResult;
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{DIRTY_TRACKED_LABEL, DIRTY_UNTRACKED_LABEL};

const MINIMUM_FREE_BYTES_PER_CANDIDATE: u64 = 536_870_912;

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let repository = configuration.repository_path.clone();

    let input = json!({
        "repository_path": repository.display().to_string(),
        "candidate_count": configuration.budgets.candidates.get(),
        "quality_profile": configuration.quality.profile.as_str(),
        "commit_policy": configuration.commit_policy.as_str(),
        "agent_driver": configuration.agent.driver.as_str(),
    });
    let evidence = AttemptEvidence::with_input(input);

    if let Err(error) = configuration.validate() {
        return Ok(NodeOutput::failed(error.to_node_failure(), None).with_evidence(evidence));
    }

    let facts = match services.git.inspect(&repository).await {
        Ok(facts) => facts,
        Err(error) => {
            return Ok(NodeOutput::failed(error.to_node_failure(), None).with_evidence(evidence))
        }
    };

    if facts.has_submodules {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::PermanentConfiguration,
                "submodules_unsupported",
                "the repository declares submodules, which Heikas Forge does not orchestrate",
            )
            .with_remedy("Remove the submodule requirement or run Heikas Forge on a repository without submodules."),
            None,
        )
        .with_evidence(evidence));
    }

    let mut dirty_snapshot_taken = false;
    let mut artifacts = Vec::new();

    if !facts.is_clean {
        if configuration.git.require_clean_repository && !configuration.git.include_dirty {
            return Ok(NodeOutput::failed(
                NodeFailure::new(
                    FailureClass::UserActionRequired,
                    "repository_not_clean",
                    format!(
                        "the repository has {} staged, {} unstaged and {} untracked changes",
                        facts.staged_paths.len(),
                        facts.unstaged_paths.len(),
                        facts.untracked_paths.len()
                    ),
                )
                .with_remedy(
                    "Commit or stash the changes, or start the run with the include dirty option.",
                ),
                None,
            )
            .with_evidence(evidence));
        }
        let snapshot = services.git.capture_dirty_snapshot(&repository).await?;
        let tracked = services
            .store
            .store_artifact(
                context.run.run_id,
                DIRTY_TRACKED_LABEL,
                "artifacts/baseline-dirty.patch",
                &snapshot.tracked_patch,
                false,
            )
            .await?;
        artifacts.push(tracked.to_reference());
        if !snapshot.untracked_archive.is_empty() {
            let untracked = services
                .store
                .store_artifact(
                    context.run.run_id,
                    DIRTY_UNTRACKED_LABEL,
                    "artifacts/baseline-untracked.zip",
                    &snapshot.untracked_archive,
                    false,
                )
                .await?;
            artifacts.push(untracked.to_reference());
        }
        dirty_snapshot_taken = true;
    }

    let capabilities = services.agent.capabilities().await?;
    if !capabilities.available {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::PermanentConfiguration,
                "agent_unavailable",
                format!(
                    "the {} agent driver is not available: {}",
                    capabilities.driver.as_str(),
                    capabilities.diagnostics.join("; ")
                ),
            )
            .with_remedy("Run `heikas doctor` and configure an available agent driver."),
            None,
        )
        .with_evidence(evidence));
    }
    if !capabilities.supports_structured_tool_calls {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::PermanentConfiguration,
                "agent_lacks_structured_tool_calls",
                "the selected agent driver cannot make reliable structured tool calls",
            )
            .with_remedy("Select a model or driver that supports structured tool calling."),
            None,
        )
        .with_evidence(evidence));
    }

    for command in &configuration.commands.commands {
        if services
            .processes
            .probe_executable(&command.program)
            .await?
            .is_none()
        {
            return Ok(NodeOutput::failed(
                NodeFailure::new(
                    FailureClass::PermanentConfiguration,
                    "command_executable_missing",
                    format!(
                        "the executable `{}` for command `{}` was not found",
                        command.program, command.id
                    ),
                )
                .with_remedy("Install the executable or correct the command in the configuration."),
                None,
            )
            .with_evidence(evidence));
        }
    }

    for provider in &services.reviews {
        if provider.required() && !provider.available().await? {
            return Ok(NodeOutput::failed(
                NodeFailure::new(
                    FailureClass::PermanentConfiguration,
                    "required_review_provider_unavailable",
                    format!(
                        "the required review provider `{}` is not available",
                        provider.name()
                    ),
                )
                .with_remedy("Install the provider or change the quality profile."),
                None,
            )
            .with_evidence(evidence));
        }
    }

    let host_facts = services.host.facts().await?;
    let disk = services.host.disk_space(&host_facts.heikas_home).await?;
    let required_bytes = MINIMUM_FREE_BYTES_PER_CANDIDATE
        .saturating_mul(u64::from(configuration.budgets.candidates.get()).saturating_add(1));
    if disk.available_bytes < required_bytes {
        return Ok(NodeOutput::failed(
            NodeFailure::new(
                FailureClass::PermanentConfiguration,
                "insufficient_disk_space",
                format!(
                    "{} bytes are available but {} bytes are required for {} candidate worktrees",
                    disk.available_bytes,
                    required_bytes,
                    configuration.budgets.candidates.get()
                ),
            )
            .with_remedy("Free disk space or reduce the candidate count."),
            None,
        )
        .with_evidence(evidence));
    }

    let configuration_digest = configuration.digest()?;
    let command_ids: Vec<String> = configuration
        .commands
        .commands
        .iter()
        .map(|command| command.id.to_string())
        .collect();
    let required_command_ids: Vec<String> = configuration
        .required_commands()
        .iter()
        .map(|command| command.id.to_string())
        .collect();

    let events = vec![
        EventPayload::BaselineResolved {
            baseline_commit: facts.head_commit.clone(),
            default_branch: facts.default_branch.clone(),
            dirty_snapshot: dirty_snapshot_taken,
        },
        EventPayload::ConfigurationSnapshotted {
            digest: configuration_digest,
            command_ids,
            required_command_ids,
            review_providers: services
                .reviews
                .iter()
                .map(|provider| provider.name().to_string())
                .collect(),
        },
    ];

    let metrics = json!({
        "available_disk_bytes": disk.available_bytes,
        "logical_processors": host_facts.logical_processors,
        "agent_isolation": capabilities.isolation.as_str(),
        "agent_model": capabilities.model_identity,
        "dirty_snapshot": dirty_snapshot_taken,
    });

    Ok(NodeOutput::succeeded(Some(NodeId::Plan))
        .with_patch(StatePatch {
            baseline_commit: Some(facts.head_commit.clone()),
            ..StatePatch::default()
        })
        .with_events(events)
        .with_artifacts(artifacts)
        .with_metrics(metrics)
        .with_evidence(evidence))
}
