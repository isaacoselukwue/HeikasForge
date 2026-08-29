use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clock::{DurationMs, Timestamp};
use crate::error::DomainError;
use crate::failure::NodeFailure;
use crate::graph::NodeId;
use crate::identity::{AttemptNumber, CandidateId, ContentDigest, RunId};

pub const NODE_RESULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Succeeded,
    Failed,
    Paused,
    Cancelled,
    Interrupted,
}

impl NodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeStatus::Succeeded => "succeeded",
            NodeStatus::Failed => "failed",
            NodeStatus::Paused => "paused",
            NodeStatus::Cancelled => "cancelled",
            NodeStatus::Interrupted => "interrupted",
        }
    }

    pub fn is_terminal(&self) -> bool {
        !matches!(self, NodeStatus::Interrupted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactReference {
    pub id: ContentDigest,
    pub label: String,
    pub relative_path: String,
    pub media_type: String,
    pub byte_length: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeResult {
    pub schema_version: u32,
    pub run_id: RunId,
    pub candidate_id: Option<CandidateId>,
    pub node_id: NodeId,
    pub attempt: AttemptNumber,
    pub status: NodeStatus,
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
    pub duration_ms: DurationMs,
    pub next: Option<NodeId>,
    pub state_patch: StatePatch,
    pub artifacts: Vec<ArtifactReference>,
    pub failure: Option<NodeFailure>,
    pub metrics: Value,
    pub warnings: Vec<String>,
}

impl NodeResult {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.schema_version != NODE_RESULT_SCHEMA_VERSION {
            return Err(DomainError::InvariantViolated(format!(
                "node result schema version {} is not supported",
                self.schema_version
            )));
        }
        if self.finished_at < self.started_at {
            return Err(DomainError::InvariantViolated(
                "node result finished before it started".to_string(),
            ));
        }
        if self.node_id.scope() == crate::graph::NodeScope::Candidate && self.candidate_id.is_none()
        {
            return Err(DomainError::MissingField {
                field: "candidate_id",
            });
        }
        if self.node_id.scope() == crate::graph::NodeScope::Run && self.candidate_id.is_some() {
            return Err(DomainError::InvariantViolated(format!(
                "node `{}` is run scoped and cannot carry a candidate identifier",
                self.node_id
            )));
        }
        match self.status {
            NodeStatus::Succeeded => {
                if let Some(next) = self.next {
                    self.node_id.validate_transition(next)?;
                }
                if self.failure.is_some() {
                    return Err(DomainError::InvariantViolated(
                        "a succeeded node result cannot carry a failure".to_string(),
                    ));
                }
            }
            NodeStatus::Failed => {
                if self.failure.is_none() {
                    return Err(DomainError::MissingField { field: "failure" });
                }
                if let Some(next) = self.next {
                    self.node_id.validate_transition(next)?;
                }
            }
            NodeStatus::Paused | NodeStatus::Cancelled | NodeStatus::Interrupted => {}
        }
        self.state_patch.validate()
    }

    pub fn builder(
        run_id: RunId,
        node_id: NodeId,
        attempt: AttemptNumber,
        started_at: Timestamp,
    ) -> NodeResultBuilder {
        NodeResultBuilder {
            run_id,
            candidate_id: None,
            node_id,
            attempt,
            started_at,
            artifacts: Vec::new(),
            state_patch: StatePatch::default(),
            metrics: Value::Object(serde_json::Map::new()),
            warnings: Vec::new(),
        }
    }
}

pub struct NodeResultBuilder {
    run_id: RunId,
    candidate_id: Option<CandidateId>,
    node_id: NodeId,
    attempt: AttemptNumber,
    started_at: Timestamp,
    artifacts: Vec<ArtifactReference>,
    state_patch: StatePatch,
    metrics: Value,
    warnings: Vec<String>,
}

impl NodeResultBuilder {
    pub fn candidate(mut self, candidate: CandidateId) -> Self {
        self.candidate_id = Some(candidate);
        self
    }

    pub fn artifact(mut self, artifact: ArtifactReference) -> Self {
        self.artifacts.push(artifact);
        self
    }

    pub fn artifacts(mut self, artifacts: Vec<ArtifactReference>) -> Self {
        self.artifacts.extend(artifacts);
        self
    }

    pub fn patch(mut self, patch: StatePatch) -> Self {
        self.state_patch = patch;
        self
    }

    pub fn metrics(mut self, metrics: Value) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    pub fn succeeded(self, finished_at: Timestamp, next: Option<NodeId>) -> NodeResult {
        self.finish(finished_at, NodeStatus::Succeeded, next, None)
    }

    pub fn failed(
        self,
        finished_at: Timestamp,
        failure: NodeFailure,
        next: Option<NodeId>,
    ) -> NodeResult {
        self.finish(finished_at, NodeStatus::Failed, next, Some(failure))
    }

    pub fn paused(self, finished_at: Timestamp) -> NodeResult {
        self.finish(finished_at, NodeStatus::Paused, None, None)
    }

    pub fn cancelled(self, finished_at: Timestamp) -> NodeResult {
        self.finish(finished_at, NodeStatus::Cancelled, None, None)
    }

    fn finish(
        self,
        finished_at: Timestamp,
        status: NodeStatus,
        next: Option<NodeId>,
        failure: Option<NodeFailure>,
    ) -> NodeResult {
        let duration_ms = finished_at.duration_since(self.started_at);
        NodeResult {
            schema_version: NODE_RESULT_SCHEMA_VERSION,
            run_id: self.run_id,
            candidate_id: self.candidate_id,
            node_id: self.node_id,
            attempt: self.attempt,
            status,
            started_at: self.started_at,
            finished_at,
            duration_ms,
            next,
            state_patch: self.state_patch,
            artifacts: self.artifacts,
            failure,
            metrics: self.metrics,
            warnings: self.warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct StatePatch {
    pub run_status: Option<crate::run::RunStatus>,
    pub candidate_status: Option<crate::candidate::CandidateStatus>,
    pub plan_version: Option<u32>,
    pub baseline_commit: Option<crate::identity::CommitHash>,
    pub changed_lines: Option<u64>,
    pub changed_files: Option<u32>,
    pub diff_digest: Option<ContentDigest>,
    pub repairs_used: Option<u32>,
    pub gate_duration_ms: Option<u64>,
    pub failure_fingerprint: Option<String>,
    pub exclusion_reasons: Option<Vec<crate::score::ExclusionReason>>,
    pub score: Option<crate::score::ScoreTuple>,
    pub winner: Option<CandidateId>,
    pub commit_hash: Option<crate::identity::CommitHash>,
    pub branch: Option<crate::identity::BranchName>,
    pub promotable: Option<bool>,
    pub integration_attempted: Option<bool>,
}

impl StatePatch {
    pub fn validate(&self) -> Result<(), DomainError> {
        if let Some(lines) = self.changed_lines {
            if lines > 10_000_000 {
                return Err(DomainError::ValueOutOfRange {
                    field: "changed_lines",
                    value: lines.to_string(),
                });
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self == &StatePatch::default()
    }
}
