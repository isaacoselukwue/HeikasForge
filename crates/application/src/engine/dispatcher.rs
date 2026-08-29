use std::sync::Arc;

use heikas_domain::candidate::CandidateStatus;
use heikas_domain::clock::Timestamp;
use heikas_domain::event::{DurableEvent, EventPayload};
use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{AttemptNumber, CandidateId, RunId};
use heikas_domain::node::{NodeResult, NodeStatus};
use heikas_domain::retry::{classify_retry, RetryDecision};
use heikas_domain::run::RunStatus;
use heikas_domain::state::{RunManifest, RunProjection};
use tokio::sync::{watch, Mutex, Semaphore};
use tracing::{debug, info, instrument, warn};

use crate::configuration::EffectiveConfiguration;
use crate::engine::context::{NodeContext, NodeOutput, RunContext};
use crate::engine::recovery;
use crate::engine::scheduler::{
    active_candidates, candidate_status_for_node, next_candidate_step, next_run_step,
    run_status_for_node, CandidateStep, RunStep,
};
use crate::engine::services::EngineServices;
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::attempt::AttemptKey;
use crate::nodes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Completed(RunStatus),
    Paused(RunStatus),
    Blocked(String),
}

impl DispatchOutcome {
    pub fn status(&self) -> Option<RunStatus> {
        match self {
            DispatchOutcome::Completed(status) | DispatchOutcome::Paused(status) => Some(*status),
            DispatchOutcome::Blocked(_) => None,
        }
    }
}

pub struct Dispatcher {
    context: RunContext,
    projection: Mutex<RunProjection>,
}

impl Dispatcher {
    pub async fn dispatch(
        services: EngineServices,
        run_id: RunId,
        cancellation: watch::Receiver<bool>,
    ) -> ApplicationResult<DispatchOutcome> {
        let guard = services.locks.acquire(run_id).await?;
        let outcome = Self::dispatch_locked(services, run_id, cancellation).await;
        guard.release();
        outcome
    }

    async fn dispatch_locked(
        services: EngineServices,
        run_id: RunId,
        cancellation: watch::Receiver<bool>,
    ) -> ApplicationResult<DispatchOutcome> {
        if !services.store.exists(run_id).await? {
            return Err(ApplicationError::RunNotFound(run_id));
        }
        let configuration = services.store.configuration(run_id).await?;
        let task_markdown = services.store.task_markdown(run_id).await?;
        let projection = recovery::recover(&services, run_id).await?;
        let context = RunContext {
            run_id,
            repository: configuration.repository_path.clone(),
            configuration: Arc::new(configuration),
            task_markdown,
            services,
            cancellation,
        };
        let dispatcher = Arc::new(Self {
            context,
            projection: Mutex::new(projection),
        });
        dispatcher.run_loop().await
    }

    pub async fn snapshot(&self) -> RunProjection {
        self.projection.lock().await.clone()
    }

    #[instrument(skip(self), fields(run_id = %self.context.run_id))]
    async fn run_loop(self: &Arc<Self>) -> ApplicationResult<DispatchOutcome> {
        loop {
            let snapshot = self.snapshot().await;
            let step = next_run_step(&snapshot);
            debug!(step = ?step, "scheduler selected the next run step");
            match step {
                RunStep::Finish(status) => {
                    return Ok(DispatchOutcome::Completed(status));
                }
                RunStep::Blocked(reason) => {
                    return Ok(DispatchOutcome::Blocked(reason));
                }
                RunStep::AwaitPlanApproval => {
                    self.ensure_run_status(RunStatus::AwaitingPlanApproval, None)
                        .await?;
                    return Ok(DispatchOutcome::Paused(RunStatus::AwaitingPlanApproval));
                }
                RunStep::AwaitCommitApproval => {
                    self.ensure_run_status(RunStatus::AwaitingCommitApproval, None)
                        .await?;
                    return Ok(DispatchOutcome::Paused(RunStatus::AwaitingCommitApproval));
                }
                RunStep::Cancel => {
                    self.finish_cancelled().await?;
                    return Ok(DispatchOutcome::Completed(RunStatus::Cancelled));
                }
                RunStep::RunCandidates => {
                    self.run_candidate_subgraphs().await?;
                }
                RunStep::RunNode(node) => {
                    let progressed = self.execute_run_node(node).await?;
                    if !progressed {
                        let snapshot = self.snapshot().await;
                        return Ok(DispatchOutcome::Blocked(format!(
                            "node {} could not make progress from status {}",
                            node.as_str(),
                            snapshot.status
                        )));
                    }
                }
            }
        }
    }

    async fn finish_cancelled(self: &Arc<Self>) -> ApplicationResult<()> {
        let snapshot = self.snapshot().await;
        for candidate in snapshot.candidates.iter() {
            if !candidate.status.is_terminal() {
                self.set_candidate_status(
                    &candidate.id,
                    CandidateStatus::Cancelled,
                    Some("the run was cancelled".to_string()),
                )
                .await?;
            }
        }
        self.ensure_run_status(
            RunStatus::Cancelled,
            Some("the run was cancelled".to_string()),
        )
        .await?;
        Ok(())
    }

    async fn run_candidate_subgraphs(self: &Arc<Self>) -> ApplicationResult<()> {
        let snapshot = self.snapshot().await;
        let candidates = active_candidates(&snapshot);
        if candidates.is_empty() {
            return Ok(());
        }
        let parallelism =
            self.context.configuration.budgets.effective_parallelism(
                self.context.services.host.facts().await?.logical_processors,
            );
        let permits = Arc::new(Semaphore::new(usize::from(parallelism.max(1))));
        let mut tasks = Vec::new();
        for candidate in candidates {
            let dispatcher = Arc::clone(self);
            let permits = Arc::clone(&permits);
            tasks.push(tokio::spawn(async move {
                let permit = permits
                    .acquire_owned()
                    .await
                    .map_err(|error| ApplicationError::Internal(error.to_string()))?;
                let outcome = dispatcher.drive_candidate(candidate).await;
                drop(permit);
                outcome
            }));
        }
        let mut first_error = None;
        for task in tasks {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    warn!(error = %error, "a candidate subgraph failed");
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    warn!(error = %error, "a candidate task could not be joined");
                    if first_error.is_none() {
                        first_error = Some(ApplicationError::Internal(error.to_string()));
                    }
                }
            }
        }
        match first_error {
            Some(ApplicationError::Cancelled) => Ok(()),
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    #[instrument(skip(self), fields(run_id = %self.context.run_id, candidate = %candidate))]
    async fn drive_candidate(self: Arc<Self>, candidate: CandidateId) -> ApplicationResult<()> {
        loop {
            let snapshot = self.snapshot().await;
            match next_candidate_step(&snapshot, &candidate) {
                CandidateStep::Finish => return Ok(()),
                CandidateStep::Blocked(reason) => {
                    warn!(reason = %reason, "candidate progress is blocked");
                    return Ok(());
                }
                CandidateStep::RunNode(node) => {
                    let progressed = self.execute_candidate_node(node, candidate.clone()).await?;
                    if !progressed {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn execute_run_node(self: &Arc<Self>, node: NodeId) -> ApplicationResult<bool> {
        let snapshot = self.snapshot().await;
        if let Some(target) = desired_run_status(snapshot.status, node) {
            if snapshot.status.transition_to(target).is_ok() {
                self.ensure_run_status(target, None).await?;
            }
        }
        self.execute_node(node, None).await
    }

    async fn execute_candidate_node(
        self: &Arc<Self>,
        node: NodeId,
        candidate: CandidateId,
    ) -> ApplicationResult<bool> {
        if let Some(target) = candidate_status_for_node(node) {
            self.set_candidate_status(&candidate, target, None).await?;
        }
        self.execute_node(node, Some(candidate)).await
    }

    #[instrument(skip(self), fields(run_id = %self.context.run_id, node = %node))]
    async fn execute_node(
        self: &Arc<Self>,
        node: NodeId,
        candidate: Option<CandidateId>,
    ) -> ApplicationResult<bool> {
        if self.context.cancelled() {
            self.finish_cancelled().await?;
            return Ok(false);
        }
        let snapshot = self.snapshot().await;
        let attempt = snapshot.next_attempt_number(node, candidate.as_ref());
        let started_at = self.context.services.clock.now();

        self.commit_event(EventPayload::NodeStarted {
            node_id: node,
            candidate_id: candidate.clone(),
            attempt,
            prompt_template_hash: None,
        })
        .await?;

        let node_context = NodeContext {
            run: &self.context,
            node,
            candidate: candidate.clone(),
            attempt,
            projection: self.snapshot().await,
        };

        let timeout = self.context.configuration.timeouts.for_node(node);
        let execution = tokio::time::timeout(timeout, nodes::execute(&node_context)).await;
        let output = match execution {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                warn!(error = %error, "node execution returned an error");
                NodeOutput::failed(error.to_node_failure(), None)
            }
            Err(_) => NodeOutput::failed(
                NodeFailure::new(
                    FailureClass::TransientInfrastructure,
                    "node_timeout",
                    format!(
                        "node {} exceeded its {} second budget",
                        node.as_str(),
                        timeout.as_secs()
                    ),
                )
                .with_remedy(
                    "Increase the node timeout in the configuration or reduce the task scope.",
                ),
                None,
            ),
        };

        self.finalise_attempt(node, candidate, attempt, started_at, output)
            .await
    }

    async fn finalise_attempt(
        self: &Arc<Self>,
        node: NodeId,
        candidate: Option<CandidateId>,
        attempt: AttemptNumber,
        started_at: Timestamp,
        mut output: NodeOutput,
    ) -> ApplicationResult<bool> {
        let finished_at = self.context.services.clock.now();
        let run_id = self.context.run_id;

        if output.status == NodeStatus::Failed {
            if let Some(failure) = output.failure.clone() {
                let decision = self
                    .decide_retry(node, candidate.as_ref(), attempt, &failure)
                    .await;
                output = self
                    .apply_retry_decision(node, candidate.as_ref(), decision, output)
                    .await?;
            }
        }

        for event in std::mem::take(&mut output.events) {
            self.commit_event(event).await?;
        }

        let mut builder = NodeResult::builder(run_id, node, attempt, started_at);
        if let Some(candidate_id) = candidate.clone() {
            builder = builder.candidate(candidate_id);
        }
        builder = builder
            .patch(output.state_patch.clone())
            .artifacts(output.artifacts.clone())
            .metrics(output.metrics.clone());
        for warning in &output.warnings {
            builder = builder.warning(warning.clone());
        }
        let result = match output.status {
            NodeStatus::Succeeded => builder.succeeded(finished_at, output.next),
            NodeStatus::Failed => builder.failed(
                finished_at,
                output.failure.clone().unwrap_or_else(|| {
                    NodeFailure::new(
                        FailureClass::InternalInvariant,
                        "missing_failure",
                        "a failed node produced no failure record",
                    )
                }),
                output.next,
            ),
            NodeStatus::Paused => builder.paused(finished_at),
            NodeStatus::Cancelled => builder.cancelled(finished_at),
            NodeStatus::Interrupted => builder.cancelled(finished_at),
        };
        result.validate()?;

        let key = AttemptKey::new(node, candidate.clone(), attempt);
        let evidence = std::mem::take(&mut output.evidence);
        self.context
            .services
            .store
            .commit_attempt(run_id, &result, evidence)
            .await?;

        let result_digest =
            heikas_domain::identity::ContentDigest::of_bytes(&serde_json::to_vec(&result)?);
        let duration = result.duration_ms;
        let terminal = match output.status {
            NodeStatus::Succeeded => EventPayload::NodeSucceeded {
                node_id: node,
                candidate_id: candidate.clone(),
                attempt,
                duration,
                next: output.next,
                result_digest,
            },
            NodeStatus::Failed => EventPayload::NodeFailed {
                node_id: node,
                candidate_id: candidate.clone(),
                attempt,
                duration,
                failure: result.failure.clone().unwrap_or_else(|| {
                    NodeFailure::new(
                        FailureClass::InternalInvariant,
                        "missing_failure",
                        "a failed node produced no failure record",
                    )
                }),
                next: output.next,
                result_digest,
            },
            NodeStatus::Paused => EventPayload::NodePaused {
                node_id: node,
                candidate_id: candidate.clone(),
                attempt,
                reason: output
                    .warnings
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "the node paused for user action".to_string()),
                result_digest,
            },
            NodeStatus::Cancelled | NodeStatus::Interrupted => EventPayload::NodeCancelled {
                node_id: node,
                candidate_id: candidate.clone(),
                attempt,
                result_digest,
            },
        };
        self.commit_event(terminal).await?;
        self.apply_state_patch(node, candidate.as_ref(), &output)
            .await?;
        self.update_manifest(&key).await?;

        Ok(matches!(
            output.status,
            NodeStatus::Succeeded | NodeStatus::Failed
        ))
    }

    async fn decide_retry(
        &self,
        node: NodeId,
        candidate: Option<&CandidateId>,
        attempt: AttemptNumber,
        failure: &NodeFailure,
    ) -> RetryDecision {
        let snapshot = self.projection.lock().await;
        let repair_budget_remaining = candidate
            .and_then(|id| snapshot.candidate(id))
            .map(|record| record.has_repair_budget())
            .unwrap_or(false);
        classify_retry(
            node,
            failure.class,
            attempt.get(),
            self.context.configuration.retry,
            repair_budget_remaining,
        )
    }

    async fn apply_retry_decision(
        self: &Arc<Self>,
        node: NodeId,
        candidate: Option<&CandidateId>,
        decision: RetryDecision,
        mut output: NodeOutput,
    ) -> ApplicationResult<NodeOutput> {
        match decision {
            RetryDecision::RetrySameNode => {
                let attempt = {
                    let snapshot = self.projection.lock().await;
                    snapshot.next_attempt_number(node, candidate).get()
                };
                let delay = self.context.configuration.retry.delay_with_jitter(
                    attempt,
                    self.context.services.identifiers.jitter_fraction(),
                );
                self.commit_event(EventPayload::NodeRetryScheduled {
                    node_id: node,
                    candidate_id: candidate.cloned(),
                    attempt: AttemptNumber::new(attempt).unwrap_or(AttemptNumber::FIRST),
                    delay: heikas_domain::clock::DurationMs::from_millis(delay.as_millis() as u64),
                    reason: "a transient infrastructure failure was detected".to_string(),
                })
                .await?;
                tokio::time::sleep(delay).await;
                output.next = Some(node);
            }
            RetryDecision::RouteToRepair => {
                output.next = Some(NodeId::RepairCandidate);
            }
            RetryDecision::FailCandidate => {
                output.next = Some(NodeId::Join);
                output.state_patch.candidate_status = Some(CandidateStatus::Ineligible);
            }
            RetryDecision::PauseForUser => {
                output.status = NodeStatus::Paused;
                output.next = None;
            }
            RetryDecision::Cancel => {
                output.status = NodeStatus::Cancelled;
                output.next = None;
            }
            RetryDecision::FailRun => {
                output.next = None;
            }
        }
        Ok(output)
    }

    async fn apply_state_patch(
        self: &Arc<Self>,
        node: NodeId,
        candidate: Option<&CandidateId>,
        output: &NodeOutput,
    ) -> ApplicationResult<()> {
        if let Some(status) = output.state_patch.candidate_status {
            if let Some(candidate_id) = candidate {
                self.set_candidate_status(candidate_id, status, None)
                    .await?;
            }
        }
        if let Some(status) = output.state_patch.run_status {
            self.ensure_run_status(status, None).await?;
        }
        if output.status == NodeStatus::Failed
            && output.next.is_none()
            && node.scope() == heikas_domain::graph::NodeScope::Run
        {
            let reason = output
                .failure
                .as_ref()
                .map(|failure| failure.message.clone())
                .unwrap_or_else(|| format!("node {} failed", node.as_str()));
            self.ensure_run_status(RunStatus::Failed, Some(reason))
                .await?;
        }
        if output.status == NodeStatus::Paused
            && node.scope() == heikas_domain::graph::NodeScope::Run
        {
            self.ensure_run_status(
                RunStatus::RecoveryRequired,
                Some(
                    output
                        .warnings
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "the run requires user action".to_string()),
                ),
            )
            .await?;
        }
        Ok(())
    }

    async fn update_manifest(&self, key: &AttemptKey) -> ApplicationResult<()> {
        let snapshot = self.projection.lock().await;
        let mut manifest = self
            .context
            .services
            .store
            .load_manifest(self.context.run_id)
            .await?
            .unwrap_or_else(|| RunManifest::empty(self.context.run_id));
        manifest.last_event_sequence = snapshot.last_event_sequence;
        manifest.last_event_hash = snapshot.last_event_hash.clone();
        let path = format!("nodes/{}", key.directory_segments().join("/"));
        if !manifest.node_evidence_paths.contains(&path) {
            manifest.node_evidence_paths.push(path);
        }
        manifest.candidate_paths = snapshot
            .candidates
            .iter()
            .map(|candidate| format!("candidates/{}", candidate.id))
            .collect();
        manifest.artifact_count = snapshot.metrics.artifacts_stored;
        self.context.services.store.store_manifest(&manifest).await
    }

    pub async fn ensure_run_status(
        &self,
        target: RunStatus,
        reason: Option<String>,
    ) -> ApplicationResult<()> {
        let current = {
            let snapshot = self.projection.lock().await;
            snapshot.status
        };
        if current == target {
            return Ok(());
        }
        current.transition_to(target)?;
        self.commit_event(EventPayload::RunStatusChanged {
            from: current,
            to: target,
            reason,
        })
        .await?;
        Ok(())
    }

    pub async fn set_candidate_status(
        &self,
        candidate: &CandidateId,
        target: CandidateStatus,
        reason: Option<String>,
    ) -> ApplicationResult<()> {
        let current = {
            let snapshot = self.projection.lock().await;
            snapshot
                .candidate(candidate)
                .map(|record| record.status)
                .ok_or_else(|| ApplicationError::CandidateNotFound {
                    run: self.context.run_id,
                    candidate: candidate.clone(),
                })?
        };
        if current == target {
            return Ok(());
        }
        current.transition_to(target)?;
        self.commit_event(EventPayload::CandidateStatusChanged {
            candidate_id: candidate.clone(),
            from: current,
            to: target,
            reason,
        })
        .await?;
        Ok(())
    }

    pub async fn commit_event(&self, payload: EventPayload) -> ApplicationResult<DurableEvent> {
        let mut projection = self.projection.lock().await;
        let event = self
            .context
            .services
            .store
            .append(self.context.run_id, payload)
            .await?;
        projection.apply(&event)?;
        self.context.services.store.store(&projection).await?;
        self.context
            .services
            .store
            .store_metrics(self.context.run_id, &projection)
            .await?;
        drop(projection);
        self.context.services.publisher.publish(&event).await?;
        Ok(event)
    }

    pub fn configuration(&self) -> &EffectiveConfiguration {
        &self.context.configuration
    }
}

fn desired_run_status(current: RunStatus, node: NodeId) -> Option<RunStatus> {
    let target = run_status_for_node(node);
    if current == target {
        return None;
    }
    if node == NodeId::Commit && current == RunStatus::AwaitingCommitApproval {
        return None;
    }
    Some(target)
}

pub async fn dispatch_run(
    services: EngineServices,
    run_id: RunId,
    cancellation: watch::Receiver<bool>,
) -> ApplicationResult<DispatchOutcome> {
    info!(run_id = %run_id, "dispatching run");
    Dispatcher::dispatch(services, run_id, cancellation).await
}
