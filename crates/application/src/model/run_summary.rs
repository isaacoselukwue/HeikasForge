use heikas_domain::candidate::CandidateStatus;
use heikas_domain::clock::{DurationMs, Timestamp};
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{CandidateId, RunId};
use heikas_domain::run::RunStatus;
use heikas_domain::score::{ExclusionReason, ScoreTuple};
use heikas_domain::state::RunProjection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunHeader {
    pub run_id: RunId,
    pub created_at: Timestamp,
    pub status: RunStatus,
    pub repository_path: String,
    pub task_title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunSummary {
    pub run_id: RunId,
    pub status: RunStatus,
    pub status_label: String,
    pub repository_path: String,
    pub task_title: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub elapsed: DurationMs,
    pub current_nodes: Vec<NodeId>,
    pub candidate_progress: CandidateProgress,
    pub winner: Option<CandidateId>,
    pub last_event_summary: Option<String>,
    pub demonstration_mode: bool,
    pub commit_hash: Option<String>,
    pub branch: Option<String>,
    pub plan_version: Option<u32>,
    pub plan_approved: bool,
    pub commit_approved: bool,
    pub recovery_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct CandidateProgress {
    pub total: u32,
    pub eligible: u32,
    pub ineligible: u32,
    pub active: u32,
    pub pending: u32,
}

impl RunSummary {
    pub fn from_projection(projection: &RunProjection, now: Timestamp) -> Self {
        let mut progress = CandidateProgress {
            total: projection.candidates.len() as u32,
            ..CandidateProgress::default()
        };
        for candidate in &projection.candidates {
            match candidate.status {
                CandidateStatus::Eligible => progress.eligible += 1,
                CandidateStatus::Ineligible | CandidateStatus::Cancelled => progress.ineligible += 1,
                CandidateStatus::Pending => progress.pending += 1,
                _ => progress.active += 1,
            }
        }
        let elapsed = if projection.status.is_terminal() {
            projection.updated_at.duration_since(projection.created_at)
        } else {
            now.duration_since(projection.created_at)
        };
        Self {
            run_id: projection.run_id,
            status: projection.status,
            status_label: projection.status.label().to_string(),
            repository_path: projection.repository_path.clone(),
            task_title: projection.task_title.clone(),
            created_at: projection.created_at,
            updated_at: projection.updated_at,
            elapsed,
            current_nodes: projection
                .open_attempts()
                .iter()
                .map(|attempt| attempt.node_id)
                .collect(),
            candidate_progress: progress,
            winner: projection.winner.clone(),
            last_event_summary: projection.last_event_summary.clone(),
            demonstration_mode: projection.demonstration_mode,
            commit_hash: projection
                .commit
                .as_ref()
                .map(|commit| commit.commit_hash.to_string()),
            branch: projection
                .commit
                .as_ref()
                .map(|commit| commit.branch.to_string()),
            plan_version: projection.plan.current().map(|version| version.version),
            plan_approved: projection.plan.is_approved(),
            commit_approved: projection.commit_approved,
            recovery_reason: projection.recovery_reason.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CandidateView {
    pub candidate_id: CandidateId,
    pub ordinal: u8,
    pub strategy: String,
    pub strategy_label: String,
    pub status: CandidateStatus,
    pub status_label: String,
    pub branch: String,
    pub repairs_used: u32,
    pub repair_budget: u32,
    pub changed_files: u32,
    pub changed_lines: u64,
    pub gate_duration: DurationMs,
    pub score: Option<ScoreTuple>,
    pub score_components: Vec<heikas_domain::score::ScoreComponent>,
    pub exclusion_reasons: Vec<ExclusionReason>,
    pub exclusion_summaries: Vec<String>,
    pub rank: Option<u32>,
    pub is_winner: bool,
    pub promotable: bool,
    pub tests_passed: Option<bool>,
    pub review_passed: Option<bool>,
    pub line_coverage_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TimelineEntry {
    pub sequence: u64,
    pub recorded_at: Timestamp,
    pub node_id: Option<NodeId>,
    pub node_label: Option<String>,
    pub candidate_id: Option<CandidateId>,
    pub attempt: Option<u32>,
    pub event_type: String,
    pub summary: String,
    pub duration: Option<DurationMs>,
    pub level: TimelineLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimelineLevel {
    Information,
    Success,
    Warning,
    Failure,
}
