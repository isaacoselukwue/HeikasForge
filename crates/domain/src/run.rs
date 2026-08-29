use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Validating,
    Planning,
    AwaitingPlanApproval,
    RunningCandidates,
    Joining,
    Integrating,
    AwaitingCommitApproval,
    Succeeded,
    Exhausted,
    Failed,
    Cancelled,
    RecoveryRequired,
}

impl RunStatus {
    pub const ALL: [RunStatus; 13] = [
        RunStatus::Created,
        RunStatus::Validating,
        RunStatus::Planning,
        RunStatus::AwaitingPlanApproval,
        RunStatus::RunningCandidates,
        RunStatus::Joining,
        RunStatus::Integrating,
        RunStatus::AwaitingCommitApproval,
        RunStatus::Succeeded,
        RunStatus::Exhausted,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::RecoveryRequired,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Created => "created",
            RunStatus::Validating => "validating",
            RunStatus::Planning => "planning",
            RunStatus::AwaitingPlanApproval => "awaiting_plan_approval",
            RunStatus::RunningCandidates => "running_candidates",
            RunStatus::Joining => "joining",
            RunStatus::Integrating => "integrating",
            RunStatus::AwaitingCommitApproval => "awaiting_commit_approval",
            RunStatus::Succeeded => "succeeded",
            RunStatus::Exhausted => "exhausted",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::RecoveryRequired => "recovery_required",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RunStatus::Created => "Created",
            RunStatus::Validating => "Validating",
            RunStatus::Planning => "Planning",
            RunStatus::AwaitingPlanApproval => "Awaiting plan approval",
            RunStatus::RunningCandidates => "Running candidates",
            RunStatus::Joining => "Joining",
            RunStatus::Integrating => "Integrating",
            RunStatus::AwaitingCommitApproval => "Awaiting commit approval",
            RunStatus::Succeeded => "Succeeded",
            RunStatus::Exhausted => "Exhausted",
            RunStatus::Failed => "Failed",
            RunStatus::Cancelled => "Cancelled",
            RunStatus::RecoveryRequired => "Recovery required",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunStatus::Succeeded | RunStatus::Exhausted | RunStatus::Failed | RunStatus::Cancelled
        )
    }

    pub fn is_paused(&self) -> bool {
        matches!(
            self,
            RunStatus::AwaitingPlanApproval | RunStatus::AwaitingCommitApproval | RunStatus::RecoveryRequired
        )
    }

    pub fn is_active(&self) -> bool {
        !self.is_terminal() && !self.is_paused()
    }

    pub fn allowed_next(&self) -> &'static [RunStatus] {
        match self {
            RunStatus::Created => &[
                RunStatus::Validating,
                RunStatus::Cancelled,
                RunStatus::Failed,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::Validating => &[
                RunStatus::Planning,
                RunStatus::Failed,
                RunStatus::Cancelled,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::Planning => &[
                RunStatus::AwaitingPlanApproval,
                RunStatus::Failed,
                RunStatus::Cancelled,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::AwaitingPlanApproval => &[
                RunStatus::Planning,
                RunStatus::RunningCandidates,
                RunStatus::Cancelled,
                RunStatus::Failed,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::RunningCandidates => &[
                RunStatus::Joining,
                RunStatus::Cancelled,
                RunStatus::Failed,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::Joining => &[
                RunStatus::Integrating,
                RunStatus::Exhausted,
                RunStatus::Cancelled,
                RunStatus::Failed,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::Integrating => &[
                RunStatus::AwaitingCommitApproval,
                RunStatus::Succeeded,
                RunStatus::Exhausted,
                RunStatus::Cancelled,
                RunStatus::Failed,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::AwaitingCommitApproval => &[
                RunStatus::Succeeded,
                RunStatus::Cancelled,
                RunStatus::Failed,
                RunStatus::RecoveryRequired,
            ],
            RunStatus::RecoveryRequired => &[
                RunStatus::Validating,
                RunStatus::Planning,
                RunStatus::AwaitingPlanApproval,
                RunStatus::RunningCandidates,
                RunStatus::Joining,
                RunStatus::Integrating,
                RunStatus::AwaitingCommitApproval,
                RunStatus::Cancelled,
                RunStatus::Failed,
            ],
            RunStatus::Succeeded | RunStatus::Exhausted | RunStatus::Failed | RunStatus::Cancelled => &[],
        }
    }

    pub fn transition_to(&self, next: RunStatus) -> Result<RunStatus, DomainError> {
        if *self == next {
            return Ok(next);
        }
        if self.allowed_next().contains(&next) {
            Ok(next)
        } else {
            Err(DomainError::IllegalRunTransition {
                from: *self,
                to: next,
            })
        }
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RunStatus {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        RunStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "RunStatus",
                value: value.to_string(),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommitPolicy {
    Manual,
    Automatic,
    None,
}

impl CommitPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommitPolicy::Manual => "manual",
            CommitPolicy::Automatic => "automatic",
            CommitPolicy::None => "none",
        }
    }
}

impl FromStr for CommitPolicy {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "manual" => Ok(CommitPolicy::Manual),
            "automatic" => Ok(CommitPolicy::Automatic),
            "none" => Ok(CommitPolicy::None),
            other => Err(DomainError::InvalidIdentifier {
                kind: "CommitPolicy",
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStrategy {
    MinimalPatch,
    TestLed,
    ArchitectureAware,
}

impl CandidateStrategy {
    pub const ROTATION: [CandidateStrategy; 3] = [
        CandidateStrategy::MinimalPatch,
        CandidateStrategy::TestLed,
        CandidateStrategy::ArchitectureAware,
    ];

    pub fn for_ordinal(ordinal: u8) -> CandidateStrategy {
        let index = usize::from(ordinal.saturating_sub(1)) % Self::ROTATION.len();
        Self::ROTATION[index]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateStrategy::MinimalPatch => "minimal_patch",
            CandidateStrategy::TestLed => "test_led",
            CandidateStrategy::ArchitectureAware => "architecture_aware",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CandidateStrategy::MinimalPatch => "Minimal patch",
            CandidateStrategy::TestLed => "Test led",
            CandidateStrategy::ArchitectureAware => "Architecture aware",
        }
    }

    pub fn emphasis(&self) -> &'static str {
        match self {
            CandidateStrategy::MinimalPatch => {
                "Favour the smallest safe change that satisfies the approved plan."
            }
            CandidateStrategy::TestLed => {
                "Drive the change from failing and regression tests before adjusting production code."
            }
            CandidateStrategy::ArchitectureAware => {
                "Favour consistency with the existing abstractions and long-term maintainability."
            }
        }
    }
}

impl FromStr for CandidateStrategy {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        CandidateStrategy::ROTATION
            .into_iter()
            .find(|strategy| strategy.as_str() == value)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "CandidateStrategy",
                value: value.to_string(),
            })
    }
}
