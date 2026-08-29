use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::clock::{DurationMs, Timestamp};
use crate::error::DomainError;
use crate::identity::{CandidateId, CandidateOrdinal, CommitHash, ContentDigest};
use crate::run::CandidateStrategy;
use crate::score::{ExclusionReason, ScoreTuple};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Pending,
    Preparing,
    Implementing,
    Testing,
    Reviewing,
    Repairing,
    Eligible,
    Ineligible,
    Interrupted,
    Cancelled,
}

impl CandidateStatus {
    pub const ALL: [CandidateStatus; 10] = [
        CandidateStatus::Pending,
        CandidateStatus::Preparing,
        CandidateStatus::Implementing,
        CandidateStatus::Testing,
        CandidateStatus::Reviewing,
        CandidateStatus::Repairing,
        CandidateStatus::Eligible,
        CandidateStatus::Ineligible,
        CandidateStatus::Interrupted,
        CandidateStatus::Cancelled,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateStatus::Pending => "pending",
            CandidateStatus::Preparing => "preparing",
            CandidateStatus::Implementing => "implementing",
            CandidateStatus::Testing => "testing",
            CandidateStatus::Reviewing => "reviewing",
            CandidateStatus::Repairing => "repairing",
            CandidateStatus::Eligible => "eligible",
            CandidateStatus::Ineligible => "ineligible",
            CandidateStatus::Interrupted => "interrupted",
            CandidateStatus::Cancelled => "cancelled",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CandidateStatus::Pending => "Pending",
            CandidateStatus::Preparing => "Preparing",
            CandidateStatus::Implementing => "Implementing",
            CandidateStatus::Testing => "Testing",
            CandidateStatus::Reviewing => "Reviewing",
            CandidateStatus::Repairing => "Repairing",
            CandidateStatus::Eligible => "Eligible",
            CandidateStatus::Ineligible => "Ineligible",
            CandidateStatus::Interrupted => "Interrupted",
            CandidateStatus::Cancelled => "Cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CandidateStatus::Eligible | CandidateStatus::Ineligible | CandidateStatus::Cancelled
        )
    }

    pub fn allowed_next(&self) -> &'static [CandidateStatus] {
        match self {
            CandidateStatus::Pending => &[
                CandidateStatus::Preparing,
                CandidateStatus::Cancelled,
                CandidateStatus::Ineligible,
            ],
            CandidateStatus::Preparing => &[
                CandidateStatus::Implementing,
                CandidateStatus::Ineligible,
                CandidateStatus::Interrupted,
                CandidateStatus::Cancelled,
            ],
            CandidateStatus::Implementing => &[
                CandidateStatus::Testing,
                CandidateStatus::Ineligible,
                CandidateStatus::Interrupted,
                CandidateStatus::Cancelled,
            ],
            CandidateStatus::Testing => &[
                CandidateStatus::Reviewing,
                CandidateStatus::Repairing,
                CandidateStatus::Ineligible,
                CandidateStatus::Interrupted,
                CandidateStatus::Cancelled,
            ],
            CandidateStatus::Reviewing => &[
                CandidateStatus::Eligible,
                CandidateStatus::Repairing,
                CandidateStatus::Ineligible,
                CandidateStatus::Interrupted,
                CandidateStatus::Cancelled,
            ],
            CandidateStatus::Repairing => &[
                CandidateStatus::Testing,
                CandidateStatus::Ineligible,
                CandidateStatus::Interrupted,
                CandidateStatus::Cancelled,
            ],
            CandidateStatus::Interrupted => &[
                CandidateStatus::Preparing,
                CandidateStatus::Implementing,
                CandidateStatus::Testing,
                CandidateStatus::Reviewing,
                CandidateStatus::Repairing,
                CandidateStatus::Ineligible,
                CandidateStatus::Cancelled,
            ],
            CandidateStatus::Eligible => &[CandidateStatus::Ineligible],
            CandidateStatus::Ineligible | CandidateStatus::Cancelled => &[],
        }
    }

    pub fn transition_to(&self, next: CandidateStatus) -> Result<CandidateStatus, DomainError> {
        if *self == next {
            return Ok(next);
        }
        if self.allowed_next().contains(&next) {
            Ok(next)
        } else {
            Err(DomainError::IllegalCandidateTransition {
                from: *self,
                to: next,
            })
        }
    }
}

impl fmt::Display for CandidateStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CandidateStatus {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        CandidateStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "CandidateStatus",
                value: value.to_string(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CandidateRecord {
    pub id: CandidateId,
    pub ordinal: CandidateOrdinal,
    pub strategy: CandidateStrategy,
    pub status: CandidateStatus,
    pub baseline_commit: CommitHash,
    pub branch: String,
    pub worktree_relative_path: String,
    pub repairs_used: u32,
    pub repair_budget: u32,
    pub changed_lines: u64,
    pub changed_files: u32,
    pub diff_digest: Option<ContentDigest>,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub gate_duration: DurationMs,
    pub last_failure_fingerprint: Option<String>,
    pub repeated_fingerprint_count: u32,
    pub score: Option<ScoreTuple>,
    pub exclusion_reasons: Vec<ExclusionReason>,
    pub promotable: bool,
    pub integration_attempted: bool,
}

impl CandidateRecord {
    pub fn new(
        id: CandidateId,
        ordinal: CandidateOrdinal,
        strategy: CandidateStrategy,
        baseline_commit: CommitHash,
        branch: String,
        worktree_relative_path: String,
        repair_budget: u32,
    ) -> Self {
        Self {
            id,
            ordinal,
            strategy,
            status: CandidateStatus::Pending,
            baseline_commit,
            branch,
            worktree_relative_path,
            repairs_used: 0,
            repair_budget,
            changed_lines: 0,
            changed_files: 0,
            diff_digest: None,
            started_at: None,
            finished_at: None,
            gate_duration: DurationMs::ZERO,
            last_failure_fingerprint: None,
            repeated_fingerprint_count: 0,
            score: None,
            exclusion_reasons: Vec::new(),
            promotable: true,
            integration_attempted: false,
        }
    }

    pub fn repair_budget_remaining(&self) -> u32 {
        self.repair_budget.saturating_sub(self.repairs_used)
    }

    pub fn has_repair_budget(&self) -> bool {
        self.repair_budget_remaining() > 0 && self.repeated_fingerprint_count < 2
    }

    pub fn observe_failure_fingerprint(&mut self, fingerprint: String) {
        if self.last_failure_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            self.repeated_fingerprint_count = self.repeated_fingerprint_count.saturating_add(1);
        } else {
            self.repeated_fingerprint_count = 0;
        }
        self.last_failure_fingerprint = Some(fingerprint);
    }
}
