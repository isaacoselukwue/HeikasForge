use heikas_domain::failure::{FailureClass, NodeFailure};
use heikas_domain::identity::{CandidateId, RunId};
use heikas_domain::DomainError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("run `{0}` does not exist")]
    RunNotFound(RunId),

    #[error("candidate `{candidate}` does not exist on run `{run}`")]
    CandidateNotFound { run: RunId, candidate: CandidateId },

    #[error("run `{0}` is already being dispatched by another process")]
    RunLocked(RunId),

    #[error("run `{run}` is {status} and cannot accept the `{operation}` operation")]
    InvalidRunState {
        run: RunId,
        status: String,
        operation: &'static str,
    },

    #[error("configuration is invalid: {0}")]
    InvalidConfiguration(String),

    #[error("the repository at `{path}` cannot be used: {detail}")]
    RepositoryUnusable { path: String, detail: String },

    #[error("required approval is missing: {0}")]
    ApprovalRequired(String),

    #[error("the durable event log for run `{run}` is corrupt: {detail}")]
    CorruptEventLog { run: RunId, detail: String },

    #[error("artefact `{0}` was not found")]
    ArtifactNotFound(String),

    #[error("the operation was cancelled")]
    Cancelled,

    #[error("the operation timed out after {seconds} seconds")]
    TimedOut { seconds: u64 },

    #[error("policy violation: {0}")]
    PolicyViolation(String),

    #[error("user action required: {0}")]
    UserActionRequired(String),

    #[error("storage failure: {0}")]
    Storage(String),

    #[error("process failure: {0}")]
    Process(String),

    #[error("git failure: {0}")]
    Git(String),

    #[error("agent failure: {0}")]
    Agent(String),

    #[error("quality provider failure: {0}")]
    QualityProvider(String),

    #[error("serialisation failure: {0}")]
    Serialisation(String),

    #[error("internal invariant violated: {0}")]
    Internal(String),
}

impl ApplicationError {
    pub fn failure_class(&self) -> FailureClass {
        match self {
            ApplicationError::Domain(DomainError::PathProtected { .. })
            | ApplicationError::Domain(DomainError::PathSensitive { .. })
            | ApplicationError::Domain(DomainError::PathEscapesWorktree { .. })
            | ApplicationError::PolicyViolation(_) => FailureClass::PolicyViolation,
            ApplicationError::Cancelled => FailureClass::Cancelled,
            ApplicationError::UserActionRequired(_) | ApplicationError::ApprovalRequired(_) => {
                FailureClass::UserActionRequired
            }
            ApplicationError::InvalidConfiguration(_)
            | ApplicationError::RepositoryUnusable { .. }
            | ApplicationError::RunNotFound(_)
            | ApplicationError::CandidateNotFound { .. }
            | ApplicationError::InvalidRunState { .. }
            | ApplicationError::ArtifactNotFound(_) => FailureClass::PermanentConfiguration,
            ApplicationError::TimedOut { .. }
            | ApplicationError::Storage(_)
            | ApplicationError::Process(_)
            | ApplicationError::Git(_)
            | ApplicationError::Agent(_)
            | ApplicationError::QualityProvider(_)
            | ApplicationError::RunLocked(_) => FailureClass::TransientInfrastructure,
            ApplicationError::CorruptEventLog { .. }
            | ApplicationError::Serialisation(_)
            | ApplicationError::Internal(_)
            | ApplicationError::Domain(_) => FailureClass::InternalInvariant,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ApplicationError::Domain(_) => "domain_invariant",
            ApplicationError::RunNotFound(_) => "run_not_found",
            ApplicationError::CandidateNotFound { .. } => "candidate_not_found",
            ApplicationError::RunLocked(_) => "run_locked",
            ApplicationError::InvalidRunState { .. } => "invalid_run_state",
            ApplicationError::InvalidConfiguration(_) => "invalid_configuration",
            ApplicationError::RepositoryUnusable { .. } => "repository_unusable",
            ApplicationError::ApprovalRequired(_) => "approval_required",
            ApplicationError::CorruptEventLog { .. } => "corrupt_event_log",
            ApplicationError::ArtifactNotFound(_) => "artifact_not_found",
            ApplicationError::Cancelled => "cancelled",
            ApplicationError::TimedOut { .. } => "timed_out",
            ApplicationError::PolicyViolation(_) => "policy_violation",
            ApplicationError::UserActionRequired(_) => "user_action_required",
            ApplicationError::Storage(_) => "storage_failure",
            ApplicationError::Process(_) => "process_failure",
            ApplicationError::Git(_) => "git_failure",
            ApplicationError::Agent(_) => "agent_failure",
            ApplicationError::QualityProvider(_) => "quality_provider_failure",
            ApplicationError::Serialisation(_) => "serialisation_failure",
            ApplicationError::Internal(_) => "internal_invariant",
        }
    }

    pub fn remedy(&self) -> Option<String> {
        match self {
            ApplicationError::RunLocked(run) => Some(format!(
                "Another dispatcher holds the lock for run {run}. Wait for it to finish or stop that process."
            )),
            ApplicationError::RepositoryUnusable { detail, .. } => Some(format!(
                "Resolve the repository state and run `heikas doctor` again. {detail}"
            )),
            ApplicationError::InvalidConfiguration(_) => Some(
                "Run `heikas doctor` to see which configuration entry is rejected.".to_string(),
            ),
            ApplicationError::ApprovalRequired(_) => {
                Some("Approve the plan in the interface or with `heikas approve-plan`.".to_string())
            }
            ApplicationError::CorruptEventLog { run, .. } => Some(format!(
                "Export the evidence with `heikas export {run}` before attempting any repair."
            )),
            _ => None,
        }
    }

    pub fn to_node_failure(&self) -> NodeFailure {
        let mut failure = NodeFailure::new(self.failure_class(), self.code(), self.to_string());
        if let Some(remedy) = self.remedy() {
            failure = failure.with_remedy(remedy);
        }
        failure
    }
}

impl From<serde_json::Error> for ApplicationError {
    fn from(error: serde_json::Error) -> Self {
        ApplicationError::Serialisation(error.to_string())
    }
}

pub type ApplicationResult<T> = Result<T, ApplicationError>;
