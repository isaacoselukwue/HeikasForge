use thiserror::Error;

use crate::candidate::CandidateStatus;
use crate::graph::NodeId;
use crate::run::RunStatus;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum DomainError {
    #[error("value `{value}` is not a valid {kind}")]
    InvalidIdentifier { kind: &'static str, value: String },

    #[error("run transition from `{from}` to `{to}` is not permitted")]
    IllegalRunTransition { from: RunStatus, to: RunStatus },

    #[error("candidate transition from `{from}` to `{to}` is not permitted")]
    IllegalCandidateTransition {
        from: CandidateStatus,
        to: CandidateStatus,
    },

    #[error("node `{from}` may not route to `{to}`")]
    IllegalNodeTransition { from: NodeId, to: NodeId },

    #[error("event sequence {actual} does not follow {expected}")]
    EventSequenceGap { expected: u64, actual: u64 },

    #[error("event {sequence} previous hash does not match the recorded chain")]
    EventChainBroken { sequence: u64 },

    #[error("event {sequence} payload hash does not match its payload")]
    EventPayloadHashMismatch { sequence: u64 },

    #[error("approval references plan hash `{approved}` but the current plan hash is `{current}`")]
    ApprovalHashMismatch { approved: String, current: String },

    #[error("path `{path}` escapes its assigned worktree")]
    PathEscapesWorktree { path: String },

    #[error("path `{path}` matches the protected pattern `{pattern}`")]
    PathProtected { path: String, pattern: String },

    #[error("path `{path}` matches the sensitive pattern `{pattern}`")]
    PathSensitive { path: String, pattern: String },

    #[error("candidate count {requested} is outside the supported range 1 to 8")]
    CandidateCountOutOfRange { requested: u32 },

    #[error("value `{value}` is outside the supported range for {field}")]
    ValueOutOfRange { field: &'static str, value: String },

    #[error("required field `{field}` is missing")]
    MissingField { field: &'static str },

    #[error("state patch for `{field}` conflicts with the recorded projection")]
    ConflictingStatePatch { field: String },

    #[error("candidate `{candidate}` is not registered on this run")]
    UnknownCandidate { candidate: String },

    #[error("node `{node}` is not registered in the graph")]
    UnknownNode { node: String },

    #[error("plan version {version} does not exist")]
    UnknownPlanVersion { version: u32 },

    #[error("{0}")]
    InvariantViolated(String),
}

pub type DomainResult<T> = Result<T, DomainError>;
