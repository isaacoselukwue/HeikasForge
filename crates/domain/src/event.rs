use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::candidate::CandidateStatus;
use crate::clock::{DurationMs, Timestamp};
use crate::error::DomainError;
use crate::failure::NodeFailure;
use crate::graph::NodeId;
use crate::identity::{
    ApprovalId, AttemptNumber, BranchName, CandidateId, CandidateOrdinal, CommitHash,
    ContentDigest, EventId, RunId,
};
use crate::plan::ApprovalDecision;
use crate::run::{CandidateStrategy, RunStatus};
use crate::score::{ExclusionReason, Ranking, ScoreTuple};

pub const EVENT_SCHEMA_VERSION: u32 = 1;
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    RunCreated {
        repository_path: String,
        task_title: String,
        task_digest: ContentDigest,
        candidate_count: u8,
        commit_policy: crate::run::CommitPolicy,
        agent_driver: String,
        demonstration_mode: bool,
    },
    RunStatusChanged {
        from: RunStatus,
        to: RunStatus,
        reason: Option<String>,
    },
    BaselineResolved {
        baseline_commit: CommitHash,
        default_branch: String,
        dirty_snapshot: bool,
    },
    ConfigurationSnapshotted {
        digest: ContentDigest,
        command_ids: Vec<String>,
        required_command_ids: Vec<String>,
        review_providers: Vec<String>,
    },
    NodeStarted {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        prompt_template_hash: Option<ContentDigest>,
    },
    NodeSucceeded {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        duration: DurationMs,
        next: Option<NodeId>,
        result_digest: ContentDigest,
    },
    NodeFailed {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        duration: DurationMs,
        failure: NodeFailure,
        next: Option<NodeId>,
        result_digest: ContentDigest,
    },
    NodePaused {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        reason: String,
        result_digest: ContentDigest,
    },
    NodeCancelled {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        result_digest: ContentDigest,
    },
    NodeInterrupted {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        detected_at: Timestamp,
    },
    NodeRetryScheduled {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        attempt: AttemptNumber,
        delay: DurationMs,
        reason: String,
    },
    PlanVersionWritten {
        version: u32,
        plan_hash: ContentDigest,
        author: crate::plan::PlanAuthor,
        revision_note: Option<String>,
        byte_length: u64,
    },
    PlanDecisionRecorded {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        plan_version: u32,
        plan_hash: ContentDigest,
        local_user: String,
        note: Option<String>,
    },
    PlanApprovalInvalidated {
        previous_plan_hash: ContentDigest,
        current_plan_hash: ContentDigest,
    },
    CandidateRegistered {
        candidate_id: CandidateId,
        ordinal: CandidateOrdinal,
        strategy: CandidateStrategy,
        branch: String,
        worktree_relative_path: String,
        repair_budget: u32,
    },
    CandidateStatusChanged {
        candidate_id: CandidateId,
        from: CandidateStatus,
        to: CandidateStatus,
        reason: Option<String>,
    },
    CandidateDiffRecorded {
        candidate_id: CandidateId,
        diff_digest: ContentDigest,
        changed_files: u32,
        changed_lines: u64,
    },
    CandidateRepairStarted {
        candidate_id: CandidateId,
        repairs_used: u32,
        repair_budget: u32,
        failure_fingerprint: Option<String>,
    },
    TestEvidenceRecorded {
        candidate_id: Option<CandidateId>,
        node_id: NodeId,
        passed: bool,
        commands: Vec<String>,
        failed_commands: Vec<String>,
        line_coverage_percent: Option<f64>,
        duration: DurationMs,
    },
    ReviewEvidenceRecorded {
        candidate_id: Option<CandidateId>,
        node_id: NodeId,
        passed: bool,
        providers: Vec<String>,
        failed_providers: Vec<String>,
        blocker_issues: u64,
        duration: DurationMs,
    },
    CandidateScored {
        candidate_id: CandidateId,
        score: ScoreTuple,
    },
    CandidateExcluded {
        candidate_id: CandidateId,
        reasons: Vec<ExclusionReason>,
    },
    RankingComputed {
        ranking: Ranking,
    },
    WinnerSelected {
        candidate_id: CandidateId,
        rank: u32,
    },
    IntegrationAttempted {
        candidate_id: CandidateId,
        applied: bool,
        detail: Option<String>,
    },
    CandidatePromotionRequested {
        previous_candidate_id: CandidateId,
        next_candidate_id: Option<CandidateId>,
        reason: String,
    },
    CommitApprovalRecorded {
        approval_id: ApprovalId,
        local_user: String,
        note: Option<String>,
    },
    CommitCreated {
        branch: BranchName,
        commit_hash: CommitHash,
        author_name: String,
        committer_name: String,
        changed_files: u32,
        signed: bool,
    },
    CancellationRequested {
        requested_by: String,
        reason: Option<String>,
    },
    RecoveryStarted {
        last_applied_sequence: u64,
        interrupted_attempts: u32,
    },
    RecoveryCompleted {
        replayed_events: u64,
        repaired_projections: Vec<String>,
    },
    ProcessSupervisionRecorded {
        node_id: NodeId,
        candidate_id: Option<CandidateId>,
        command_id: String,
        process_id: Option<u32>,
        exit_code: Option<i32>,
        timed_out: bool,
        children_terminated: u32,
    },
    ArtifactStored {
        artifact_id: ContentDigest,
        label: String,
        relative_path: String,
        byte_length: u64,
        truncated: bool,
    },
    RunExported {
        archive_relative_path: String,
        byte_length: u64,
        redacted: bool,
    },
    DiagnosticRecorded {
        level: DiagnosticLevel,
        code: String,
        message: String,
        detail: Option<Value>,
    },
}

impl EventPayload {
    pub fn type_name(&self) -> &'static str {
        match self {
            EventPayload::RunCreated { .. } => "run_created",
            EventPayload::RunStatusChanged { .. } => "run_status_changed",
            EventPayload::BaselineResolved { .. } => "baseline_resolved",
            EventPayload::ConfigurationSnapshotted { .. } => "configuration_snapshotted",
            EventPayload::NodeStarted { .. } => "node_started",
            EventPayload::NodeSucceeded { .. } => "node_succeeded",
            EventPayload::NodeFailed { .. } => "node_failed",
            EventPayload::NodePaused { .. } => "node_paused",
            EventPayload::NodeCancelled { .. } => "node_cancelled",
            EventPayload::NodeInterrupted { .. } => "node_interrupted",
            EventPayload::NodeRetryScheduled { .. } => "node_retry_scheduled",
            EventPayload::PlanVersionWritten { .. } => "plan_version_written",
            EventPayload::PlanDecisionRecorded { .. } => "plan_decision_recorded",
            EventPayload::PlanApprovalInvalidated { .. } => "plan_approval_invalidated",
            EventPayload::CandidateRegistered { .. } => "candidate_registered",
            EventPayload::CandidateStatusChanged { .. } => "candidate_status_changed",
            EventPayload::CandidateDiffRecorded { .. } => "candidate_diff_recorded",
            EventPayload::CandidateRepairStarted { .. } => "candidate_repair_started",
            EventPayload::TestEvidenceRecorded { .. } => "test_evidence_recorded",
            EventPayload::ReviewEvidenceRecorded { .. } => "review_evidence_recorded",
            EventPayload::CandidateScored { .. } => "candidate_scored",
            EventPayload::CandidateExcluded { .. } => "candidate_excluded",
            EventPayload::RankingComputed { .. } => "ranking_computed",
            EventPayload::WinnerSelected { .. } => "winner_selected",
            EventPayload::IntegrationAttempted { .. } => "integration_attempted",
            EventPayload::CandidatePromotionRequested { .. } => "candidate_promotion_requested",
            EventPayload::CommitApprovalRecorded { .. } => "commit_approval_recorded",
            EventPayload::CommitCreated { .. } => "commit_created",
            EventPayload::CancellationRequested { .. } => "cancellation_requested",
            EventPayload::RecoveryStarted { .. } => "recovery_started",
            EventPayload::RecoveryCompleted { .. } => "recovery_completed",
            EventPayload::ProcessSupervisionRecorded { .. } => "process_supervision_recorded",
            EventPayload::ArtifactStored { .. } => "artifact_stored",
            EventPayload::RunExported { .. } => "run_exported",
            EventPayload::DiagnosticRecorded { .. } => "diagnostic_recorded",
        }
    }

    pub fn candidate_id(&self) -> Option<&CandidateId> {
        match self {
            EventPayload::NodeStarted { candidate_id, .. }
            | EventPayload::NodeSucceeded { candidate_id, .. }
            | EventPayload::NodeFailed { candidate_id, .. }
            | EventPayload::NodePaused { candidate_id, .. }
            | EventPayload::NodeCancelled { candidate_id, .. }
            | EventPayload::NodeInterrupted { candidate_id, .. }
            | EventPayload::NodeRetryScheduled { candidate_id, .. }
            | EventPayload::TestEvidenceRecorded { candidate_id, .. }
            | EventPayload::ReviewEvidenceRecorded { candidate_id, .. }
            | EventPayload::ProcessSupervisionRecorded { candidate_id, .. } => {
                candidate_id.as_ref()
            }
            EventPayload::CandidateRegistered { candidate_id, .. }
            | EventPayload::CandidateStatusChanged { candidate_id, .. }
            | EventPayload::CandidateDiffRecorded { candidate_id, .. }
            | EventPayload::CandidateRepairStarted { candidate_id, .. }
            | EventPayload::CandidateScored { candidate_id, .. }
            | EventPayload::CandidateExcluded { candidate_id, .. }
            | EventPayload::WinnerSelected { candidate_id, .. }
            | EventPayload::IntegrationAttempted { candidate_id, .. } => Some(candidate_id),
            _ => None,
        }
    }

    pub fn node_id(&self) -> Option<NodeId> {
        match self {
            EventPayload::NodeStarted { node_id, .. }
            | EventPayload::NodeSucceeded { node_id, .. }
            | EventPayload::NodeFailed { node_id, .. }
            | EventPayload::NodePaused { node_id, .. }
            | EventPayload::NodeCancelled { node_id, .. }
            | EventPayload::NodeInterrupted { node_id, .. }
            | EventPayload::NodeRetryScheduled { node_id, .. }
            | EventPayload::TestEvidenceRecorded { node_id, .. }
            | EventPayload::ReviewEvidenceRecorded { node_id, .. }
            | EventPayload::ProcessSupervisionRecorded { node_id, .. } => Some(*node_id),
            _ => None,
        }
    }

    pub fn attempt(&self) -> Option<AttemptNumber> {
        match self {
            EventPayload::NodeStarted { attempt, .. }
            | EventPayload::NodeSucceeded { attempt, .. }
            | EventPayload::NodeFailed { attempt, .. }
            | EventPayload::NodePaused { attempt, .. }
            | EventPayload::NodeCancelled { attempt, .. }
            | EventPayload::NodeInterrupted { attempt, .. }
            | EventPayload::NodeRetryScheduled { attempt, .. } => Some(*attempt),
            _ => None,
        }
    }

    pub fn is_node_terminal(&self) -> bool {
        matches!(
            self,
            EventPayload::NodeSucceeded { .. }
                | EventPayload::NodeFailed { .. }
                | EventPayload::NodePaused { .. }
                | EventPayload::NodeCancelled { .. }
        )
    }

    pub fn human_summary(&self) -> String {
        match self {
            EventPayload::RunCreated { task_title, .. } => {
                format!("Run created for `{task_title}`")
            }
            EventPayload::RunStatusChanged { from, to, .. } => {
                format!("Run status moved from {} to {}", from.label(), to.label())
            }
            EventPayload::BaselineResolved {
                baseline_commit, ..
            } => {
                format!("Baseline resolved at {}", baseline_commit.short())
            }
            EventPayload::ConfigurationSnapshotted { command_ids, .. } => {
                format!(
                    "Configuration snapshotted with {} commands",
                    command_ids.len()
                )
            }
            EventPayload::NodeStarted {
                node_id, attempt, ..
            } => {
                format!("{} started, attempt {attempt}", node_id.label())
            }
            EventPayload::NodeSucceeded {
                node_id, duration, ..
            } => {
                format!("{} succeeded in {}", node_id.label(), duration.human())
            }
            EventPayload::NodeFailed {
                node_id, failure, ..
            } => {
                format!("{} failed: {}", node_id.label(), failure.message)
            }
            EventPayload::NodePaused {
                node_id, reason, ..
            } => {
                format!("{} paused: {reason}", node_id.label())
            }
            EventPayload::NodeCancelled { node_id, .. } => format!("{} cancelled", node_id.label()),
            EventPayload::NodeInterrupted { node_id, .. } => {
                format!("{} was interrupted and will be recovered", node_id.label())
            }
            EventPayload::NodeRetryScheduled {
                node_id,
                delay,
                reason,
                ..
            } => format!(
                "{} retry scheduled in {} because {reason}",
                node_id.label(),
                delay.human()
            ),
            EventPayload::PlanVersionWritten { version, .. } => {
                format!("Plan version {version} written")
            }
            EventPayload::PlanDecisionRecorded {
                decision,
                plan_version,
                ..
            } => {
                format!(
                    "Plan version {plan_version} {}",
                    decision.as_str().replace('_', " ")
                )
            }
            EventPayload::PlanApprovalInvalidated { .. } => {
                "Plan approval invalidated by a later edit".to_string()
            }
            EventPayload::CandidateRegistered {
                candidate_id,
                strategy,
                ..
            } => {
                format!(
                    "Candidate {candidate_id} registered with the {} strategy",
                    strategy.label()
                )
            }
            EventPayload::CandidateStatusChanged {
                candidate_id, to, ..
            } => {
                format!("Candidate {candidate_id} is now {}", to.label())
            }
            EventPayload::CandidateDiffRecorded {
                candidate_id,
                changed_files,
                changed_lines,
                ..
            } => {
                format!("Candidate {candidate_id} changed {changed_lines} lines across {changed_files} files")
            }
            EventPayload::CandidateRepairStarted {
                candidate_id,
                repairs_used,
                repair_budget,
                ..
            } => {
                format!("Candidate {candidate_id} repair {repairs_used} of {repair_budget} started")
            }
            EventPayload::TestEvidenceRecorded {
                passed,
                failed_commands,
                ..
            } => {
                if *passed {
                    "All required test commands passed".to_string()
                } else {
                    format!("Test commands failed: {}", failed_commands.join(", "))
                }
            }
            EventPayload::ReviewEvidenceRecorded {
                passed,
                failed_providers,
                ..
            } => {
                if *passed {
                    "All required review providers passed".to_string()
                } else {
                    format!("Review providers failed: {}", failed_providers.join(", "))
                }
            }
            EventPayload::CandidateScored { candidate_id, .. } => {
                format!("Candidate {candidate_id} scored")
            }
            EventPayload::CandidateExcluded {
                candidate_id,
                reasons,
            } => format!(
                "Candidate {candidate_id} excluded: {}",
                reasons
                    .iter()
                    .map(ExclusionReason::summary)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            EventPayload::RankingComputed { ranking } => {
                format!(
                    "Ranking computed across {} candidates",
                    ranking.entries.len()
                )
            }
            EventPayload::WinnerSelected { candidate_id, rank } => {
                format!("Candidate {candidate_id} selected at rank {rank}")
            }
            EventPayload::IntegrationAttempted {
                candidate_id,
                applied,
                detail,
            } => {
                if *applied {
                    format!("Candidate {candidate_id} applied to the integration worktree")
                } else {
                    format!(
                        "Candidate {candidate_id} could not be applied: {}",
                        detail.as_deref().unwrap_or("unknown reason")
                    )
                }
            }
            EventPayload::CandidatePromotionRequested {
                next_candidate_id,
                reason,
                ..
            } => match next_candidate_id {
                Some(next) => format!("Promoting candidate {next} because {reason}"),
                None => format!("No candidate remains to promote because {reason}"),
            },
            EventPayload::CommitApprovalRecorded { local_user, .. } => {
                format!("Commit approved by {local_user}")
            }
            EventPayload::CommitCreated {
                branch,
                commit_hash,
                ..
            } => {
                format!("Commit {} created on {branch}", commit_hash.short())
            }
            EventPayload::CancellationRequested { requested_by, .. } => {
                format!("Cancellation requested by {requested_by}")
            }
            EventPayload::RecoveryStarted {
                interrupted_attempts,
                ..
            } => {
                format!("Recovery started with {interrupted_attempts} interrupted attempts")
            }
            EventPayload::RecoveryCompleted {
                replayed_events, ..
            } => {
                format!("Recovery replayed {replayed_events} events")
            }
            EventPayload::ProcessSupervisionRecorded {
                command_id,
                timed_out,
                ..
            } => {
                if *timed_out {
                    format!("Command `{command_id}` exceeded its timeout and its process tree was terminated")
                } else {
                    format!("Command `{command_id}` process tree completed")
                }
            }
            EventPayload::ArtifactStored {
                label, byte_length, ..
            } => {
                format!("Artefact `{label}` stored with {byte_length} bytes")
            }
            EventPayload::RunExported {
                archive_relative_path,
                ..
            } => {
                format!("Run exported to {archive_relative_path}")
            }
            EventPayload::DiagnosticRecorded { level, message, .. } => {
                format!("{}: {message}", level.label())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

impl DiagnosticLevel {
    pub fn label(&self) -> &'static str {
        match self {
            DiagnosticLevel::Info => "Information",
            DiagnosticLevel::Warning => "Warning",
            DiagnosticLevel::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DurableEvent {
    pub schema_version: u32,
    pub sequence: u64,
    pub event_id: EventId,
    pub run_id: RunId,
    pub candidate_id: Option<CandidateId>,
    pub node_id: Option<NodeId>,
    pub attempt: Option<AttemptNumber>,
    pub recorded_at: Timestamp,
    pub event_type: String,
    pub previous_hash: String,
    pub payload_hash: String,
    pub payload: EventPayload,
}

impl DurableEvent {
    pub fn seal(
        sequence: u64,
        event_id: EventId,
        run_id: RunId,
        recorded_at: Timestamp,
        previous_hash: &str,
        payload: EventPayload,
    ) -> Result<Self, DomainError> {
        let payload_hash = hash_payload(&payload)?;
        Ok(Self {
            schema_version: EVENT_SCHEMA_VERSION,
            sequence,
            event_id,
            run_id,
            candidate_id: payload.candidate_id().cloned(),
            node_id: payload.node_id(),
            attempt: payload.attempt(),
            recorded_at,
            event_type: payload.type_name().to_string(),
            previous_hash: previous_hash.to_string(),
            payload_hash,
            payload,
        })
    }

    pub fn chain_hash(&self) -> String {
        let material = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.sequence, self.event_id, self.recorded_at, self.previous_hash, self.payload_hash
        );
        ContentDigest::of_str(&material).as_str().to_string()
    }

    pub fn verify(
        &self,
        expected_sequence: u64,
        expected_previous_hash: &str,
    ) -> Result<(), DomainError> {
        if self.schema_version != EVENT_SCHEMA_VERSION {
            return Err(DomainError::InvariantViolated(format!(
                "event schema version {} is not supported",
                self.schema_version
            )));
        }
        if self.sequence != expected_sequence {
            return Err(DomainError::EventSequenceGap {
                expected: expected_sequence,
                actual: self.sequence,
            });
        }
        if self.previous_hash != expected_previous_hash {
            return Err(DomainError::EventChainBroken {
                sequence: self.sequence,
            });
        }
        let computed = hash_payload(&self.payload)?;
        if computed != self.payload_hash {
            return Err(DomainError::EventPayloadHashMismatch {
                sequence: self.sequence,
            });
        }
        if self.event_type != self.payload.type_name() {
            return Err(DomainError::InvariantViolated(format!(
                "event {} declares type `{}` but carries `{}`",
                self.sequence,
                self.event_type,
                self.payload.type_name()
            )));
        }
        Ok(())
    }
}

pub fn hash_payload(payload: &EventPayload) -> Result<String, DomainError> {
    let encoded = serde_json::to_vec(payload).map_err(|error| {
        DomainError::InvariantViolated(format!("event payload could not be encoded: {error}"))
    })?;
    Ok(ContentDigest::of_bytes(&encoded).as_str().to_string())
}
