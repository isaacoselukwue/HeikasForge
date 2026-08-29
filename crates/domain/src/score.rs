use std::fmt;

use serde::{Deserialize, Serialize};

use crate::candidate::{CandidateRecord, CandidateStatus};
use crate::clock::DurationMs;
use crate::identity::CandidateId;
use crate::review::{AggregatedReview, IssueCategory, IssueSeverity};
use crate::test_evidence::TestEvidence;

pub const COVERAGE_SCALE: f64 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CoverageRank {
    Measured(i64),
    Missing,
}

impl CoverageRank {
    pub fn from_percent(percent: Option<f64>) -> Self {
        match percent {
            Some(value) => {
                let scaled = (value.clamp(0.0, 100.0) * COVERAGE_SCALE).round() as i64;
                CoverageRank::Measured(-scaled)
            }
            None => CoverageRank::Missing,
        }
    }

    pub fn percent(&self) -> Option<f64> {
        match self {
            CoverageRank::Measured(scaled) => Some(-(*scaled as f64) / COVERAGE_SCALE),
            CoverageRank::Missing => None,
        }
    }
}

impl fmt::Display for CoverageRank {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.percent() {
            Some(value) => write!(formatter, "{value:.2}%"),
            None => formatter.write_str("not measured"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreTuple {
    pub blocker_issues: u64,
    pub critical_issues: u64,
    pub high_issues: u64,
    pub medium_issues: u64,
    pub new_security_weight: u64,
    pub new_reliability_weight: u64,
    pub new_maintainability_weight: u64,
    pub coverage_rank: CoverageRank,
    pub test_integrity_penalty: u64,
    pub changed_lines: u64,
    pub repair_attempts: u32,
    pub gate_duration_ms: u64,
    pub candidate_id: CandidateId,
}

impl ScoreTuple {
    pub fn build(
        candidate: &CandidateRecord,
        review: &AggregatedReview,
        tests: &TestEvidence,
    ) -> Self {
        let coverage = review
            .line_coverage_percent()
            .or(tests.line_coverage_percent);
        Self {
            blocker_issues: review.issue_count(IssueSeverity::Blocker),
            critical_issues: review.issue_count(IssueSeverity::Critical),
            high_issues: review.issue_count(IssueSeverity::High),
            medium_issues: review.issue_count(IssueSeverity::Medium),
            new_security_weight: review.weighted_new_score(IssueCategory::Security),
            new_reliability_weight: review.weighted_new_score(IssueCategory::Reliability),
            new_maintainability_weight: review.weighted_new_score(IssueCategory::Maintainability),
            coverage_rank: CoverageRank::from_percent(coverage),
            test_integrity_penalty: review.test_integrity_penalty(),
            changed_lines: candidate.changed_lines,
            repair_attempts: candidate.repairs_used,
            gate_duration_ms: candidate.gate_duration.millis(),
            candidate_id: candidate.id.clone(),
        }
    }

    pub fn components(&self) -> Vec<ScoreComponent> {
        vec![
            ScoreComponent::new("Blocker issues", self.blocker_issues.to_string()),
            ScoreComponent::new("Critical issues", self.critical_issues.to_string()),
            ScoreComponent::new("High issues", self.high_issues.to_string()),
            ScoreComponent::new("Medium issues", self.medium_issues.to_string()),
            ScoreComponent::new("New security weight", self.new_security_weight.to_string()),
            ScoreComponent::new("New reliability weight", self.new_reliability_weight.to_string()),
            ScoreComponent::new(
                "New maintainability weight",
                self.new_maintainability_weight.to_string(),
            ),
            ScoreComponent::new("Line coverage", self.coverage_rank.to_string()),
            ScoreComponent::new("Test integrity penalty", self.test_integrity_penalty.to_string()),
            ScoreComponent::new("Changed lines", self.changed_lines.to_string()),
            ScoreComponent::new("Repair attempts", self.repair_attempts.to_string()),
            ScoreComponent::new(
                "Total gate duration",
                DurationMs::from_millis(self.gate_duration_ms).human(),
            ),
            ScoreComponent::new("Candidate identifier", self.candidate_id.to_string()),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreComponent {
    pub label: String,
    pub value: String,
}

impl ScoreComponent {
    fn new(label: &str, value: String) -> Self {
        Self {
            label: label.to_string(),
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ExclusionReason {
    RequiredTestFailed { command_id: String, detail: String },
    RequiredTestMissing { command_id: String },
    RequiredReviewFailed { provider: String, detail: String },
    RequiredReviewMissing,
    BlockerPolicyFinding { rule_id: String, detail: String },
    EmptyDiff,
    DiffDoesNotApply { detail: String },
    CoverageBelowThreshold { measured: f64, required: f64 },
    RepairBudgetExhausted { used: u32, budget: u32 },
    TimeBudgetExceeded { detail: String },
    Cancelled,
    Interrupted,
    IntegrationFailed { detail: String },
}

impl ExclusionReason {
    pub fn summary(&self) -> String {
        match self {
            ExclusionReason::RequiredTestFailed { command_id, detail } => {
                format!("Required test command `{command_id}` failed: {detail}")
            }
            ExclusionReason::RequiredTestMissing { command_id } => {
                format!("Required test command `{command_id}` produced no valid report")
            }
            ExclusionReason::RequiredReviewFailed { provider, detail } => {
                format!("Required review provider `{provider}` failed: {detail}")
            }
            ExclusionReason::RequiredReviewMissing => {
                "No required review provider produced a report".to_string()
            }
            ExclusionReason::BlockerPolicyFinding { rule_id, detail } => {
                format!("Blocker policy finding `{rule_id}`: {detail}")
            }
            ExclusionReason::EmptyDiff => {
                "The candidate produced no change while the task requires one".to_string()
            }
            ExclusionReason::DiffDoesNotApply { detail } => {
                format!("The candidate patch does not apply to the baseline: {detail}")
            }
            ExclusionReason::CoverageBelowThreshold { measured, required } => {
                format!("Line coverage {measured:.2}% is below the required {required:.2}%")
            }
            ExclusionReason::RepairBudgetExhausted { used, budget } => {
                format!("The repair budget was exhausted after {used} of {budget} attempts")
            }
            ExclusionReason::TimeBudgetExceeded { detail } => {
                format!("The candidate exceeded its time budget: {detail}")
            }
            ExclusionReason::Cancelled => "The candidate was cancelled".to_string(),
            ExclusionReason::Interrupted => {
                "The candidate was interrupted and could not be resumed".to_string()
            }
            ExclusionReason::IntegrationFailed { detail } => {
                format!("Integration of this candidate failed: {detail}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EligibilityInput {
    pub candidate_status: CandidateStatus,
    pub required_tests_passed: bool,
    pub failed_test_commands: Vec<(String, String)>,
    pub missing_test_commands: Vec<String>,
    pub diff_is_empty: bool,
    pub change_required: bool,
    pub diff_applies: Result<(), String>,
    pub coverage_percent: Option<f64>,
    pub minimum_line_coverage: Option<f64>,
    pub repairs_used: u32,
    pub repair_budget: u32,
    pub repair_budget_exhausted: bool,
    pub time_budget_exceeded: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EligibilityOutcome {
    pub eligible: bool,
    pub reasons: Vec<ExclusionReason>,
}

pub fn evaluate_eligibility(input: &EligibilityInput, review: &AggregatedReview) -> EligibilityOutcome {
    let mut reasons = Vec::new();

    match input.candidate_status {
        CandidateStatus::Cancelled => reasons.push(ExclusionReason::Cancelled),
        CandidateStatus::Interrupted => reasons.push(ExclusionReason::Interrupted),
        _ => {}
    }

    for (command_id, detail) in &input.failed_test_commands {
        reasons.push(ExclusionReason::RequiredTestFailed {
            command_id: command_id.clone(),
            detail: detail.clone(),
        });
    }
    for command_id in &input.missing_test_commands {
        reasons.push(ExclusionReason::RequiredTestMissing {
            command_id: command_id.clone(),
        });
    }
    if !input.required_tests_passed
        && input.failed_test_commands.is_empty()
        && input.missing_test_commands.is_empty()
    {
        reasons.push(ExclusionReason::RequiredTestMissing {
            command_id: "test".to_string(),
        });
    }

    if !review.has_required_provider() {
        reasons.push(ExclusionReason::RequiredReviewMissing);
    }
    for report in review.reports.iter().filter(|report| report.required && !report.passed) {
        reasons.push(ExclusionReason::RequiredReviewFailed {
            provider: report.provider.clone(),
            detail: report
                .failure_summary
                .clone()
                .unwrap_or_else(|| "the required quality gate did not pass".to_string()),
        });
    }

    for issue in review.blocker_policy_issues() {
        reasons.push(ExclusionReason::BlockerPolicyFinding {
            rule_id: issue.rule_id.clone(),
            detail: issue.message.clone(),
        });
    }

    if input.diff_is_empty && input.change_required {
        reasons.push(ExclusionReason::EmptyDiff);
    }

    if let Err(detail) = &input.diff_applies {
        reasons.push(ExclusionReason::DiffDoesNotApply {
            detail: detail.clone(),
        });
    }

    if let (Some(measured), Some(required)) = (input.coverage_percent, input.minimum_line_coverage) {
        if measured + f64::EPSILON < required {
            reasons.push(ExclusionReason::CoverageBelowThreshold { measured, required });
        }
    }

    if input.repair_budget_exhausted {
        reasons.push(ExclusionReason::RepairBudgetExhausted {
            used: input.repairs_used,
            budget: input.repair_budget,
        });
    }

    if let Some(detail) = &input.time_budget_exceeded {
        reasons.push(ExclusionReason::TimeBudgetExceeded {
            detail: detail.clone(),
        });
    }

    EligibilityOutcome {
        eligible: reasons.is_empty(),
        reasons,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RankedCandidate {
    pub candidate_id: CandidateId,
    pub eligible: bool,
    pub score: Option<ScoreTuple>,
    pub exclusion_reasons: Vec<ExclusionReason>,
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Ranking {
    pub entries: Vec<RankedCandidate>,
    pub winner: Option<CandidateId>,
    pub rationale: Vec<String>,
}

pub fn rank_candidates(mut entries: Vec<RankedCandidate>) -> Ranking {
    entries.sort_by(|left, right| match (&left.score, &right.score) {
        (Some(left_score), Some(right_score)) => left_score.cmp(right_score),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.candidate_id.cmp(&right.candidate_id),
    });

    let mut rank_counter = 0u32;
    for entry in entries.iter_mut() {
        if entry.eligible && entry.score.is_some() {
            rank_counter += 1;
            entry.rank = Some(rank_counter);
        } else {
            entry.rank = None;
        }
    }

    let winner = entries
        .iter()
        .find(|entry| entry.rank == Some(1))
        .map(|entry| entry.candidate_id.clone());

    let rationale = build_rationale(&entries, winner.as_ref());

    Ranking {
        entries,
        winner,
        rationale,
    }
}

fn build_rationale(entries: &[RankedCandidate], winner: Option<&CandidateId>) -> Vec<String> {
    let mut lines = Vec::new();
    let eligible_count = entries.iter().filter(|entry| entry.eligible).count();
    lines.push(format!(
        "{eligible_count} of {} candidates satisfied every required gate.",
        entries.len()
    ));

    let Some(winner_id) = winner else {
        lines.push("No candidate was eligible, so no winner was selected.".to_string());
        return lines;
    };

    let Some(winning_entry) = entries.iter().find(|entry| &entry.candidate_id == winner_id) else {
        return lines;
    };
    let Some(winning_score) = winning_entry.score.as_ref() else {
        return lines;
    };

    let runner_up = entries
        .iter()
        .filter(|entry| entry.eligible && &entry.candidate_id != winner_id)
        .find_map(|entry| entry.score.as_ref());

    match runner_up {
        Some(other) => {
            let deciding = first_difference(winning_score, other);
            lines.push(format!(
                "Candidate {winner_id} ranked first on the deterministic tuple, decided by {deciding}."
            ));
        }
        None => {
            lines.push(format!(
                "Candidate {winner_id} was the only eligible candidate and satisfied every required gate."
            ));
        }
    }

    for component in winning_score.components() {
        lines.push(format!("{}: {}", component.label, component.value));
    }
    lines
}

fn first_difference(left: &ScoreTuple, right: &ScoreTuple) -> String {
    let left_components = left.components();
    let right_components = right.components();
    for (left_component, right_component) in left_components.iter().zip(right_components.iter()) {
        if left_component.value != right_component.value {
            return format!(
                "{} ({} against {})",
                left_component.label, left_component.value, right_component.value
            );
        }
    }
    "the candidate identifier tie-break".to_string()
}
