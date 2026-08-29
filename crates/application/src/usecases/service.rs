use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use heikas_domain::budget::{CandidateCount, RunBudgets};
use heikas_domain::clock::Timestamp;
use heikas_domain::event::{DurableEvent, EventPayload};
use heikas_domain::identity::{ContentDigest, RunId};
use heikas_domain::plan::{ApprovalDecision, PlanAuthor};
use heikas_domain::run::RunStatus;
use heikas_domain::state::RunProjection;
use tokio::sync::{watch, Mutex};
use tracing::{info, warn};

use crate::configuration::EffectiveConfiguration;
use crate::engine::context::task_title_of;
use crate::engine::{dispatch_run, DispatchOutcome, EngineServices};
use crate::error::{ApplicationError, ApplicationResult};
use crate::model::detail::{EventPage, RunDetail};
use crate::model::doctor::DoctorReport;
use crate::model::request::{CreateRunRequest, ExportRequest};
use crate::model::run_summary::{CandidateView, RunSummary, TimelineEntry};
use crate::ports::clock::{Clock, IdentifierFactory, LocalIdentity};
use crate::ports::environment::HostEnvironment;
use crate::ports::git::GitService;
use crate::ports::observability::{DomainEventPublisher, RunLogWriter};
use crate::ports::process::ProcessRunner;
use crate::ports::runtime::{ConfigurationResolver, EvidenceExporter, ExportOutcome, RuntimeFactory};
use crate::ports::store::{RunLockService, RunStore};
use crate::usecases::{diagnostics, views};

#[derive(Clone)]
pub struct BaseServices {
    pub store: Arc<dyn RunStore>,
    pub locks: Arc<dyn RunLockService>,
    pub clock: Arc<dyn Clock>,
    pub identifiers: Arc<dyn IdentifierFactory>,
    pub identity: Arc<dyn LocalIdentity>,
    pub git: Arc<dyn GitService>,
    pub processes: Arc<dyn ProcessRunner>,
    pub publisher: Arc<dyn DomainEventPublisher>,
    pub host: Arc<dyn HostEnvironment>,
    pub logs: Arc<dyn RunLogWriter>,
}

pub struct ApplicationService {
    base: BaseServices,
    factory: Arc<dyn RuntimeFactory>,
    configuration: Arc<dyn ConfigurationResolver>,
    exporter: Arc<dyn EvidenceExporter>,
    cancellations: Mutex<HashMap<RunId, watch::Sender<bool>>>,
}

impl ApplicationService {
    pub fn new(
        base: BaseServices,
        factory: Arc<dyn RuntimeFactory>,
        configuration: Arc<dyn ConfigurationResolver>,
        exporter: Arc<dyn EvidenceExporter>,
    ) -> Self {
        Self {
            base,
            factory,
            configuration,
            exporter,
            cancellations: Mutex::new(HashMap::new()),
        }
    }

    pub fn base(&self) -> &BaseServices {
        &self.base
    }

    pub fn configuration_resolver(&self) -> &Arc<dyn ConfigurationResolver> {
        &self.configuration
    }

    pub fn now(&self) -> Timestamp {
        self.base.clock.now()
    }

    pub async fn engine_services(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<EngineServices> {
        Ok(EngineServices {
            store: Arc::clone(&self.base.store),
            locks: Arc::clone(&self.base.locks),
            clock: Arc::clone(&self.base.clock),
            identifiers: Arc::clone(&self.base.identifiers),
            identity: Arc::clone(&self.base.identity),
            git: Arc::clone(&self.base.git),
            processes: Arc::clone(&self.base.processes),
            agent: self.factory.agent_driver(configuration).await?,
            tests: self.factory.test_runner(configuration).await?,
            reviews: self.factory.review_providers(configuration).await?,
            publisher: Arc::clone(&self.base.publisher),
            redactor: self.factory.redactor(configuration).await?,
            host: Arc::clone(&self.base.host),
            logs: Arc::clone(&self.base.logs),
        })
    }

    pub async fn create_run(&self, request: CreateRunRequest) -> ApplicationResult<RunId> {
        let mut configuration = self.configuration.resolve(&request).await?;
        apply_request_overrides(&mut configuration, &request)?;
        configuration.validate()?;

        let run_id = self.base.identifiers.new_run_id();
        self.base
            .store
            .initialise(run_id, &request.task_markdown, &configuration)
            .await?;

        let task_digest = ContentDigest::of_str(&request.task_markdown);
        let payload = EventPayload::RunCreated {
            repository_path: configuration.repository_path.display().to_string(),
            task_title: task_title_of(&request.task_markdown),
            task_digest,
            candidate_count: configuration.budgets.candidates.get(),
            commit_policy: configuration.commit_policy,
            agent_driver: configuration.agent.driver.as_str().to_string(),
            demonstration_mode: configuration.demonstration_mode,
        };
        self.append(run_id, vec![payload]).await?;
        info!(run_id = %run_id, "run created");
        Ok(run_id)
    }

    pub async fn dispatch(&self, run_id: RunId) -> ApplicationResult<DispatchOutcome> {
        let configuration = self.base.store.configuration(run_id).await?;
        let services = self.engine_services(&configuration).await?;
        let (sender, receiver) = watch::channel(false);
        {
            let mut guard = self.cancellations.lock().await;
            guard.insert(run_id, sender);
        }
        let outcome = dispatch_run(services, run_id, receiver).await;
        {
            let mut guard = self.cancellations.lock().await;
            guard.remove(&run_id);
        }
        outcome
    }

    pub async fn resume(&self, run_id: RunId) -> ApplicationResult<DispatchOutcome> {
        let projection = self.projection(run_id).await?;
        if projection.status.is_terminal() {
            return Ok(DispatchOutcome::Completed(projection.status));
        }
        self.dispatch(run_id).await
    }

    pub async fn cancel(&self, run_id: RunId, reason: Option<String>) -> ApplicationResult<()> {
        let projection = self.projection(run_id).await?;
        if projection.status.is_terminal() {
            return Ok(());
        }
        let requested_by = self.base.identity.user_name();
        let signalled = {
            let guard = self.cancellations.lock().await;
            match guard.get(&run_id) {
                Some(sender) => {
                    let _ = sender.send(true);
                    true
                }
                None => false,
            }
        };
        self.append(
            run_id,
            vec![EventPayload::CancellationRequested {
                requested_by,
                reason,
            }],
        )
        .await?;
        if !signalled {
            let projection = self.projection(run_id).await?;
            if !projection.status.is_terminal() {
                let configuration = self.base.store.configuration(run_id).await?;
                let services = self.engine_services(&configuration).await?;
                let (_sender, receiver) = watch::channel(true);
                if let Err(error) = dispatch_run(services, run_id, receiver).await {
                    warn!(error = %error, "cancellation finalisation failed");
                }
            }
        }
        Ok(())
    }

    pub async fn update_plan(
        &self,
        run_id: RunId,
        markdown: &str,
    ) -> ApplicationResult<u32> {
        let projection = self.projection(run_id).await?;
        self.ensure_plan_editable(&projection, "update_plan")?;
        let version_number = projection.plan.next_version_number();
        let recorded_at = self.base.clock.now();
        let version = self
            .base
            .store
            .write_version(
                run_id,
                version_number,
                markdown,
                PlanAuthor::Human,
                None,
                recorded_at,
            )
            .await?;
        let mut events = Vec::new();
        if let Some(previous) = projection.plan.current() {
            if projection.plan.approval.is_some() && previous.hash != version.hash {
                events.push(EventPayload::PlanApprovalInvalidated {
                    previous_plan_hash: previous.hash.clone(),
                    current_plan_hash: version.hash.clone(),
                });
            }
        }
        events.push(EventPayload::PlanVersionWritten {
            version: version.version,
            plan_hash: version.hash.clone(),
            author: PlanAuthor::Human,
            revision_note: None,
            byte_length: version.byte_length,
        });
        self.append(run_id, events).await?;
        Ok(version.version)
    }

    pub async fn approve_plan(
        &self,
        run_id: RunId,
        plan_markdown: Option<String>,
        note: Option<String>,
    ) -> ApplicationResult<()> {
        if let Some(markdown) = plan_markdown {
            self.update_plan(run_id, &markdown).await?;
        }
        let projection = self.projection(run_id).await?;
        self.ensure_plan_editable(&projection, "approve_plan")?;
        let current = projection.plan.current().ok_or_else(|| {
            ApplicationError::ApprovalRequired("no plan version exists yet".to_string())
        })?;
        self.append(
            run_id,
            vec![EventPayload::PlanDecisionRecorded {
                approval_id: self.base.identifiers.new_approval_id(),
                decision: ApprovalDecision::Approved,
                plan_version: current.version,
                plan_hash: current.hash.clone(),
                local_user: self.base.identity.user_name(),
                note,
            }],
        )
        .await
        .map(|_| ())
    }

    pub async fn revise_plan(&self, run_id: RunId, note: String) -> ApplicationResult<()> {
        let projection = self.projection(run_id).await?;
        self.ensure_plan_editable(&projection, "revise_plan")?;
        let current = projection.plan.current().ok_or_else(|| {
            ApplicationError::ApprovalRequired("no plan version exists yet".to_string())
        })?;
        self.append(
            run_id,
            vec![EventPayload::PlanDecisionRecorded {
                approval_id: self.base.identifiers.new_approval_id(),
                decision: ApprovalDecision::RevisionRequested,
                plan_version: current.version,
                plan_hash: current.hash.clone(),
                local_user: self.base.identity.user_name(),
                note: Some(note),
            }],
        )
        .await
        .map(|_| ())
    }

    pub async fn reject_plan(&self, run_id: RunId, reason: Option<String>) -> ApplicationResult<()> {
        let projection = self.projection(run_id).await?;
        self.ensure_plan_editable(&projection, "reject_plan")?;
        let current = projection.plan.current().ok_or_else(|| {
            ApplicationError::ApprovalRequired("no plan version exists yet".to_string())
        })?;
        self.append(
            run_id,
            vec![EventPayload::PlanDecisionRecorded {
                approval_id: self.base.identifiers.new_approval_id(),
                decision: ApprovalDecision::Rejected,
                plan_version: current.version,
                plan_hash: current.hash.clone(),
                local_user: self.base.identity.user_name(),
                note: reason,
            }],
        )
        .await
        .map(|_| ())
    }

    pub async fn approve_commit(&self, run_id: RunId, note: Option<String>) -> ApplicationResult<()> {
        let projection = self.projection(run_id).await?;
        if projection.status != RunStatus::AwaitingCommitApproval {
            return Err(ApplicationError::InvalidRunState {
                run: run_id,
                status: projection.status.to_string(),
                operation: "approve_commit",
            });
        }
        self.append(
            run_id,
            vec![EventPayload::CommitApprovalRecorded {
                approval_id: self.base.identifiers.new_approval_id(),
                local_user: self.base.identity.user_name(),
                note,
            }],
        )
        .await
        .map(|_| ())
    }

    pub async fn list_runs(&self) -> ApplicationResult<Vec<RunSummary>> {
        let headers = self.base.store.headers().await?;
        let now = self.base.clock.now();
        let mut summaries = Vec::new();
        for header in headers {
            match self.base.store.load(header.run_id).await? {
                Some(projection) => summaries.push(RunSummary::from_projection(&projection, now)),
                None => continue,
            }
        }
        summaries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(summaries)
    }

    pub async fn projection(&self, run_id: RunId) -> ApplicationResult<RunProjection> {
        self.base
            .store
            .load(run_id)
            .await?
            .ok_or(ApplicationError::RunNotFound(run_id))
    }

    pub async fn run_detail(&self, run_id: RunId) -> ApplicationResult<RunDetail> {
        let projection = self.projection(run_id).await?;
        let events = self.base.store.read_after(run_id, 0).await?;
        let now = self.base.clock.now();
        Ok(RunDetail {
            summary: RunSummary::from_projection(&projection, now),
            candidates: views::candidate_views(&projection),
            graph: views::graph_view(&projection),
            timeline: views::timeline(&events),
            metrics: projection.metrics.clone(),
            ranking_rationale: projection
                .ranking
                .as_ref()
                .map(|ranking| ranking.rationale.clone())
                .unwrap_or_default(),
            integration_detail: projection.integration.last_detail.clone(),
            projection,
        })
    }

    pub async fn candidates(&self, run_id: RunId) -> ApplicationResult<Vec<CandidateView>> {
        let projection = self.projection(run_id).await?;
        Ok(views::candidate_views(&projection))
    }

    pub async fn timeline(&self, run_id: RunId) -> ApplicationResult<Vec<TimelineEntry>> {
        let events = self.base.store.read_after(run_id, 0).await?;
        Ok(views::timeline(&events))
    }

    pub async fn events(
        &self,
        run_id: RunId,
        after_sequence: u64,
        limit: usize,
    ) -> ApplicationResult<EventPage> {
        let events = self
            .base
            .store
            .read_range(run_id, after_sequence, limit)
            .await?;
        let next_sequence = events
            .last()
            .map(|event| event.sequence)
            .unwrap_or(after_sequence);
        Ok(EventPage {
            run_id,
            complete: events.len() < limit,
            events,
            next_sequence,
        })
    }

    pub async fn plan_markdown(&self, run_id: RunId) -> ApplicationResult<Option<(u32, String)>> {
        self.base.store.read_current(run_id).await
    }

    pub async fn plan_version(&self, run_id: RunId, version: u32) -> ApplicationResult<String> {
        self.base.store.read_version(run_id, version).await
    }

    pub async fn candidate_diff(
        &self,
        run_id: RunId,
        candidate: &heikas_domain::identity::CandidateId,
    ) -> ApplicationResult<Vec<u8>> {
        self.base.store.read_diff(run_id, candidate).await
    }

    pub async fn integration_diff(&self, run_id: RunId) -> ApplicationResult<Vec<u8>> {
        self.base.store.read_integration_diff(run_id).await
    }

    pub async fn artifact(
        &self,
        run_id: RunId,
        artifact_id: &ContentDigest,
    ) -> ApplicationResult<Vec<u8>> {
        self.base.store.read_artifact(run_id, artifact_id).await
    }

    pub async fn artifact_range(
        &self,
        run_id: RunId,
        artifact_id: &ContentDigest,
        offset: u64,
        length: u64,
    ) -> ApplicationResult<Vec<u8>> {
        self.base
            .store
            .read_artifact_range(run_id, artifact_id, offset, length)
            .await
    }

    pub async fn export(
        &self,
        run_id: RunId,
        request: ExportRequest,
    ) -> ApplicationResult<ExportOutcome> {
        let outcome = self
            .exporter
            .export(run_id, &request.output_path, request.include_worktrees)
            .await?;
        self.append(
            run_id,
            vec![EventPayload::RunExported {
                archive_relative_path: outcome.archive_path.display().to_string(),
                byte_length: outcome.byte_length,
                redacted: outcome.redacted,
            }],
        )
        .await?;
        Ok(outcome)
    }

    pub async fn cleanup(&self, run_id: RunId) -> ApplicationResult<Vec<String>> {
        let projection = self.projection(run_id).await?;
        if !projection.status.is_terminal() {
            return Err(ApplicationError::InvalidRunState {
                run: run_id,
                status: projection.status.to_string(),
                operation: "cleanup",
            });
        }
        self.base.store.remove_worktrees(run_id).await
    }

    pub async fn diagnose(&self, repository: Option<&Path>) -> ApplicationResult<DoctorReport> {
        diagnostics::diagnose(self, repository).await
    }

    pub async fn resolve_run_reference(&self, reference: &str) -> ApplicationResult<RunId> {
        self.base.store.resolve_run_reference(reference).await
    }

    pub fn runtime_factory(&self) -> &Arc<dyn RuntimeFactory> {
        &self.factory
    }

    fn ensure_plan_editable(
        &self,
        projection: &RunProjection,
        operation: &'static str,
    ) -> ApplicationResult<()> {
        if projection.status.is_terminal() {
            return Err(ApplicationError::InvalidRunState {
                run: projection.run_id,
                status: projection.status.to_string(),
                operation,
            });
        }
        if projection.candidates.iter().any(|candidate| {
            candidate.status != heikas_domain::candidate::CandidateStatus::Pending
        }) {
            return Err(ApplicationError::InvalidRunState {
                run: projection.run_id,
                status: "candidate work has already started".to_string(),
                operation,
            });
        }
        Ok(())
    }

    pub async fn append(
        &self,
        run_id: RunId,
        payloads: Vec<EventPayload>,
    ) -> ApplicationResult<Vec<DurableEvent>> {
        let guard = self.base.locks.acquire(run_id).await?;
        let result = self.append_locked(run_id, payloads).await;
        guard.release();
        result
    }

    async fn append_locked(
        &self,
        run_id: RunId,
        payloads: Vec<EventPayload>,
    ) -> ApplicationResult<Vec<DurableEvent>> {
        let mut projection = match self.base.store.load(run_id).await? {
            Some(projection) => projection,
            None => RunProjection::genesis(run_id, self.base.clock.now()),
        };
        let pending = self
            .base
            .store
            .read_after(run_id, projection.last_event_sequence)
            .await?;
        heikas_domain::state::replay_from(&mut projection, &pending)?;

        let mut committed = Vec::new();
        for payload in payloads {
            let event = self.base.store.append(run_id, payload).await?;
            projection.apply(&event)?;
            committed.push(event);
        }
        self.base.store.store(&projection).await?;
        self.base.store.store_metrics(run_id, &projection).await?;
        for event in &committed {
            self.base.publisher.publish(event).await?;
        }
        Ok(committed)
    }
}

fn apply_request_overrides(
    configuration: &mut EffectiveConfiguration,
    request: &CreateRunRequest,
) -> ApplicationResult<()> {
    let mut budgets: RunBudgets = configuration.budgets;
    if let Some(count) = request.candidate_count {
        budgets.candidates = CandidateCount::new(count)?;
    }
    if let Some(parallel) = request.max_parallel_candidates {
        budgets.max_parallel_candidates = parallel;
    }
    if let Some(repairs) = request.max_repairs_per_candidate {
        budgets.max_repairs_per_candidate = repairs;
    }
    if let Some(seconds) = request.wall_clock_seconds {
        budgets.wall_clock_seconds = seconds;
    }
    budgets.validate()?;
    configuration.budgets = budgets;

    if let Some(policy) = request.commit_policy {
        configuration.commit_policy = policy;
    }
    if let Some(profile) = request.quality_profile {
        configuration.quality.profile = profile;
    }
    if request.minimum_line_coverage.is_some() {
        configuration.quality.minimum_line_coverage = request.minimum_line_coverage;
    }
    if request.include_dirty {
        configuration.git.include_dirty = true;
    }
    if let Some(model) = &request.agent_model {
        configuration.agent.model = Some(model.clone());
    }
    if let Some(driver) = &request.agent_driver {
        configuration.agent.driver = driver.parse()?;
    }
    configuration.demonstration_mode = request.demonstration_mode;
    Ok(())
}
