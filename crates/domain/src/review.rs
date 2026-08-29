use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::clock::Timestamp;
use crate::error::DomainError;
use crate::identity::ContentDigest;

pub const REVIEW_REPORT_SCHEMA_VERSION: u32 = 1;

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
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
    Blocker,
}

impl IssueSeverity {
    pub const ALL: [IssueSeverity; 6] = [
        IssueSeverity::Info,
        IssueSeverity::Low,
        IssueSeverity::Medium,
        IssueSeverity::High,
        IssueSeverity::Critical,
        IssueSeverity::Blocker,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            IssueSeverity::Info => "info",
            IssueSeverity::Low => "low",
            IssueSeverity::Medium => "medium",
            IssueSeverity::High => "high",
            IssueSeverity::Critical => "critical",
            IssueSeverity::Blocker => "blocker",
        }
    }

    pub fn weight(&self) -> u64 {
        match self {
            IssueSeverity::Info => 0,
            IssueSeverity::Low => 1,
            IssueSeverity::Medium => 3,
            IssueSeverity::High => 9,
            IssueSeverity::Critical => 27,
            IssueSeverity::Blocker => 81,
        }
    }
}

impl fmt::Display for IssueSeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for IssueSeverity {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalised = value.to_ascii_lowercase();
        IssueSeverity::ALL
            .into_iter()
            .find(|severity| severity.as_str() == normalised)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "IssueSeverity",
                value: value.to_string(),
            })
    }
}

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
#[serde(rename_all = "snake_case")]
pub enum IssueCategory {
    Security,
    Reliability,
    Maintainability,
    Coverage,
    Formatting,
    Policy,
    TestIntegrity,
    Dependency,
    Secret,
}

impl IssueCategory {
    pub const ALL: [IssueCategory; 9] = [
        IssueCategory::Security,
        IssueCategory::Reliability,
        IssueCategory::Maintainability,
        IssueCategory::Coverage,
        IssueCategory::Formatting,
        IssueCategory::Policy,
        IssueCategory::TestIntegrity,
        IssueCategory::Dependency,
        IssueCategory::Secret,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            IssueCategory::Security => "security",
            IssueCategory::Reliability => "reliability",
            IssueCategory::Maintainability => "maintainability",
            IssueCategory::Coverage => "coverage",
            IssueCategory::Formatting => "formatting",
            IssueCategory::Policy => "policy",
            IssueCategory::TestIntegrity => "test_integrity",
            IssueCategory::Dependency => "dependency",
            IssueCategory::Secret => "secret",
        }
    }
}

impl fmt::Display for IssueCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for IssueCategory {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalised = value.to_ascii_lowercase();
        IssueCategory::ALL
            .into_iter()
            .find(|category| category.as_str() == normalised)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "IssueCategory",
                value: value.to_string(),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityGateOutcome {
    Passed,
    Failed,
    NotApplicable,
}

impl QualityGateOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            QualityGateOutcome::Passed => "passed",
            QualityGateOutcome::Failed => "failed",
            QualityGateOutcome::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewIssue {
    pub provider: String,
    pub fingerprint: String,
    pub rule_id: String,
    pub category: IssueCategory,
    pub severity: IssueSeverity,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub help_reference: Option<String>,
    pub is_new: bool,
}

impl ReviewIssue {
    pub fn compute_fingerprint(
        provider: &str,
        rule_id: &str,
        file: Option<&str>,
        message: &str,
    ) -> String {
        let material = format!(
            "{provider}\u{1f}{rule_id}\u{1f}{}\u{1f}{message}",
            file.unwrap_or("")
        );
        ContentDigest::of_str(&material).short().to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct ReviewMetrics {
    pub line_coverage_percent: Option<f64>,
    pub branch_coverage_percent: Option<f64>,
    pub changed_lines: Option<u64>,
    pub changed_files: Option<u32>,
    pub analysed_files: Option<u32>,
    pub duplicated_lines: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewArtifactReference {
    pub label: String,
    pub relative_path: String,
    pub media_type: String,
    pub digest: ContentDigest,
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewReport {
    pub schema_version: u32,
    pub provider: String,
    pub required: bool,
    pub advisory: bool,
    pub passed: bool,
    pub quality_gate: QualityGateOutcome,
    pub issues: Vec<ReviewIssue>,
    pub metrics: ReviewMetrics,
    pub artifacts: Vec<ReviewArtifactReference>,
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
    pub failure_summary: Option<String>,
}

impl ReviewReport {
    pub fn issue_count(&self, severity: IssueSeverity) -> u64 {
        self.issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .count() as u64
    }

    pub fn weighted_new_score(&self, category: IssueCategory) -> u64 {
        self.issues
            .iter()
            .filter(|issue| issue.is_new && issue.category == category)
            .map(|issue| issue.severity.weight())
            .sum()
    }

    pub fn blocking_issues(&self) -> Vec<&ReviewIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == IssueSeverity::Blocker)
            .collect()
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.schema_version != REVIEW_REPORT_SCHEMA_VERSION {
            return Err(DomainError::InvariantViolated(format!(
                "review report schema version {} is not supported",
                self.schema_version
            )));
        }
        if self.provider.trim().is_empty() {
            return Err(DomainError::MissingField { field: "provider" });
        }
        if self.required && self.advisory {
            return Err(DomainError::InvariantViolated(
                "a review provider cannot be both required and advisory".to_string(),
            ));
        }
        if self.passed && self.quality_gate == QualityGateOutcome::Failed {
            return Err(DomainError::InvariantViolated(
                "a passed review report cannot carry a failed quality gate".to_string(),
            ));
        }
        if self.finished_at < self.started_at {
            return Err(DomainError::InvariantViolated(
                "review report finished before it started".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct AggregatedReview {
    pub reports: Vec<ReviewReport>,
}

impl AggregatedReview {
    pub fn required_reports_passed(&self) -> bool {
        self.reports
            .iter()
            .filter(|report| report.required)
            .all(|report| report.passed)
    }

    pub fn has_required_provider(&self) -> bool {
        self.reports.iter().any(|report| report.required)
    }

    pub fn failed_required_providers(&self) -> Vec<&str> {
        self.reports
            .iter()
            .filter(|report| report.required && !report.passed)
            .map(|report| report.provider.as_str())
            .collect()
    }

    pub fn issue_count(&self, severity: IssueSeverity) -> u64 {
        self.reports
            .iter()
            .map(|report| report.issue_count(severity))
            .sum()
    }

    pub fn weighted_new_score(&self, category: IssueCategory) -> u64 {
        self.reports
            .iter()
            .map(|report| report.weighted_new_score(category))
            .sum()
    }

    pub fn line_coverage_percent(&self) -> Option<f64> {
        self.reports
            .iter()
            .filter_map(|report| report.metrics.line_coverage_percent)
            .next_back()
    }

    pub fn blocker_policy_issues(&self) -> Vec<&ReviewIssue> {
        self.reports
            .iter()
            .flat_map(|report| report.issues.iter())
            .filter(|issue| {
                issue.severity == IssueSeverity::Blocker
                    && matches!(
                        issue.category,
                        IssueCategory::Policy | IssueCategory::TestIntegrity
                    )
            })
            .collect()
    }

    pub fn test_integrity_penalty(&self) -> u64 {
        self.reports
            .iter()
            .flat_map(|report| report.issues.iter())
            .filter(|issue| issue.category == IssueCategory::TestIntegrity)
            .map(|issue| issue.severity.weight())
            .sum()
    }
}
