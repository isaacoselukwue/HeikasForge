use heikas_application::error::ApplicationError;
use heikas_domain::run::RunStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success = 0,
    InvalidUsage = 2,
    AwaitingApproval = 3,
    Exhausted = 4,
    Failed = 5,
    Cancelled = 6,
    RecoveryRequired = 7,
    PolicyViolation = 8,
    Interrupted = 130,
}

impl ExitCode {
    pub fn value(self) -> i32 {
        self as i32
    }

    pub fn for_status(status: RunStatus) -> Self {
        match status {
            RunStatus::Succeeded => ExitCode::Success,
            RunStatus::Exhausted => ExitCode::Exhausted,
            RunStatus::Failed => ExitCode::Failed,
            RunStatus::Cancelled => ExitCode::Cancelled,
            RunStatus::RecoveryRequired => ExitCode::RecoveryRequired,
            RunStatus::AwaitingPlanApproval | RunStatus::AwaitingCommitApproval => {
                ExitCode::AwaitingApproval
            }
            _ => ExitCode::Success,
        }
    }

    pub fn for_error(error: &ApplicationError) -> Self {
        match error {
            ApplicationError::PolicyViolation(_) => ExitCode::PolicyViolation,
            ApplicationError::Cancelled => ExitCode::Cancelled,
            ApplicationError::CorruptEventLog { .. } => ExitCode::RecoveryRequired,
            ApplicationError::InvalidConfiguration(_)
            | ApplicationError::RepositoryUnusable { .. }
            | ApplicationError::RunNotFound(_)
            | ApplicationError::CandidateNotFound { .. }
            | ApplicationError::InvalidRunState { .. } => ExitCode::InvalidUsage,
            ApplicationError::ApprovalRequired(_) | ApplicationError::UserActionRequired(_) => {
                ExitCode::AwaitingApproval
            }
            _ => ExitCode::Failed,
        }
    }
}
