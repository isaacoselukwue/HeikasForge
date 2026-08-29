use serde::{Deserialize, Serialize};

use crate::error::DomainError;

pub const MINIMUM_CANDIDATES: u8 = 1;
pub const MAXIMUM_CANDIDATES: u8 = 8;
pub const DEFAULT_CANDIDATES: u8 = 3;
pub const MAXIMUM_REPAIRS: u32 = 10;
pub const DEFAULT_REPAIRS: u32 = 3;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct CandidateCount(u8);

impl CandidateCount {
    pub fn new(value: u8) -> Result<Self, DomainError> {
        if !(MINIMUM_CANDIDATES..=MAXIMUM_CANDIDATES).contains(&value) {
            return Err(DomainError::CandidateCountOutOfRange {
                requested: u32::from(value),
            });
        }
        Ok(Self(value))
    }

    pub fn get(&self) -> u8 {
        self.0
    }
}

impl Default for CandidateCount {
    fn default() -> Self {
        Self(DEFAULT_CANDIDATES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunBudgets {
    pub candidates: CandidateCount,
    pub max_parallel_candidates: u8,
    pub max_repairs_per_candidate: u32,
    pub wall_clock_seconds: u32,
    pub max_agent_turns: u32,
    pub max_output_bytes_per_stream: u64,
    pub max_total_artifact_bytes: u64,
}

impl Default for RunBudgets {
    fn default() -> Self {
        Self {
            candidates: CandidateCount::default(),
            max_parallel_candidates: DEFAULT_CANDIDATES,
            max_repairs_per_candidate: DEFAULT_REPAIRS,
            wall_clock_seconds: 10_800,
            max_agent_turns: 40,
            max_output_bytes_per_stream: 2_097_152,
            max_total_artifact_bytes: 268_435_456,
        }
    }
}

impl RunBudgets {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.max_parallel_candidates == 0 || self.max_parallel_candidates > MAXIMUM_CANDIDATES {
            return Err(DomainError::ValueOutOfRange {
                field: "max_parallel_candidates",
                value: self.max_parallel_candidates.to_string(),
            });
        }
        if self.max_repairs_per_candidate > MAXIMUM_REPAIRS {
            return Err(DomainError::ValueOutOfRange {
                field: "max_repairs_per_candidate",
                value: self.max_repairs_per_candidate.to_string(),
            });
        }
        if self.wall_clock_seconds == 0 || self.wall_clock_seconds > 86_400 {
            return Err(DomainError::ValueOutOfRange {
                field: "wall_clock_seconds",
                value: self.wall_clock_seconds.to_string(),
            });
        }
        if self.max_agent_turns == 0 || self.max_agent_turns > 500 {
            return Err(DomainError::ValueOutOfRange {
                field: "max_agent_turns",
                value: self.max_agent_turns.to_string(),
            });
        }
        if self.max_output_bytes_per_stream < 4_096 {
            return Err(DomainError::ValueOutOfRange {
                field: "max_output_bytes_per_stream",
                value: self.max_output_bytes_per_stream.to_string(),
            });
        }
        Ok(())
    }

    pub fn effective_parallelism(&self, logical_processors: usize) -> u8 {
        let half = (logical_processors / 2).max(1) as u8;
        self.max_parallel_candidates
            .min(half)
            .min(self.candidates.get())
            .max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityProfile {
    Standard,
    Strict,
}

impl QualityProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            QualityProfile::Standard => "standard",
            QualityProfile::Strict => "strict",
        }
    }

    pub fn default_minimum_line_coverage(&self) -> Option<f64> {
        match self {
            QualityProfile::Standard => None,
            QualityProfile::Strict => Some(80.0),
        }
    }
}

impl std::str::FromStr for QualityProfile {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "standard" => Ok(QualityProfile::Standard),
            "strict" => Ok(QualityProfile::Strict),
            other => Err(DomainError::InvalidIdentifier {
                kind: "QualityProfile",
                value: other.to_string(),
            }),
        }
    }
}
