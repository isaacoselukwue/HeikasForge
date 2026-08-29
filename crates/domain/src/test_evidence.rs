use serde::{Deserialize, Serialize};

use crate::clock::DurationMs;
use crate::command::CommandId;
use crate::identity::ContentDigest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    NotRun,
    ReportMissing,
}

impl CommandOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandOutcome::Passed => "passed",
            CommandOutcome::Failed => "failed",
            CommandOutcome::TimedOut => "timed_out",
            CommandOutcome::Cancelled => "cancelled",
            CommandOutcome::NotRun => "not_run",
            CommandOutcome::ReportMissing => "report_missing",
        }
    }

    pub fn is_pass(&self) -> bool {
        matches!(self, CommandOutcome::Passed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TestFailureDetail {
    pub suite: String,
    pub case: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommandExecutionRecord {
    pub command_id: CommandId,
    pub required: bool,
    pub outcome: CommandOutcome,
    pub exit_code: Option<i32>,
    pub duration: DurationMs,
    pub stdout_artifact: Option<ContentDigest>,
    pub stderr_artifact: Option<ContentDigest>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub tests_total: Option<u32>,
    pub tests_failed: Option<u32>,
    pub tests_skipped: Option<u32>,
    pub failures: Vec<TestFailureDetail>,
    pub line_coverage_percent: Option<f64>,
    pub detail: Option<String>,
}

impl CommandExecutionRecord {
    pub fn failure_summary(&self) -> String {
        if let Some(detail) = &self.detail {
            return detail.clone();
        }
        match self.outcome {
            CommandOutcome::TimedOut => "the command exceeded its timeout".to_string(),
            CommandOutcome::Cancelled => "the command was cancelled".to_string(),
            CommandOutcome::ReportMissing => "the command produced no valid report".to_string(),
            CommandOutcome::NotRun => "the command was not run".to_string(),
            _ => match self.exit_code {
                Some(code) => format!("the command exited with status {code}"),
                None => "the command failed without an exit status".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct TestEvidence {
    pub commands: Vec<CommandExecutionRecord>,
    pub line_coverage_percent: Option<f64>,
    pub total_duration: DurationMs,
}

impl TestEvidence {
    pub fn all_required_passed(&self) -> bool {
        self.commands
            .iter()
            .filter(|record| record.required)
            .all(|record| record.outcome.is_pass())
    }

    pub fn failed_required(&self) -> Vec<(String, String)> {
        self.commands
            .iter()
            .filter(|record| {
                record.required
                    && matches!(
                        record.outcome,
                        CommandOutcome::Failed | CommandOutcome::TimedOut | CommandOutcome::Cancelled
                    )
            })
            .map(|record| (record.command_id.to_string(), record.failure_summary()))
            .collect()
    }

    pub fn missing_required(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter(|record| {
                record.required
                    && matches!(record.outcome, CommandOutcome::ReportMissing | CommandOutcome::NotRun)
            })
            .map(|record| record.command_id.to_string())
            .collect()
    }

    pub fn failure_fingerprint(&self) -> Option<String> {
        let mut material = String::new();
        for record in self.commands.iter().filter(|record| !record.outcome.is_pass()) {
            material.push_str(record.command_id.as_str());
            material.push('\u{1f}');
            material.push_str(record.outcome.as_str());
            for failure in &record.failures {
                material.push('\u{1f}');
                material.push_str(&failure.suite);
                material.push('\u{1e}');
                material.push_str(&failure.case);
            }
            material.push('\n');
        }
        if material.is_empty() {
            None
        } else {
            Some(ContentDigest::of_str(&material).short().to_string())
        }
    }

    pub fn recompute_totals(&mut self) {
        self.total_duration = self
            .commands
            .iter()
            .fold(DurationMs::ZERO, |total, record| total.saturating_add(record.duration));
        self.line_coverage_percent = self
            .commands
            .iter()
            .filter_map(|record| record.line_coverage_percent)
            .next_back();
    }
}
