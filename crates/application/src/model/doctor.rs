use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Passed,
    Warning,
    Failed,
    Skipped,
}

impl CheckOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckOutcome::Passed => "passed",
            CheckOutcome::Warning => "warning",
            CheckOutcome::Failed => "failed",
            CheckOutcome::Skipped => "skipped",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CheckOutcome::Passed => "Passed",
            CheckOutcome::Warning => "Warning",
            CheckOutcome::Failed => "Failed",
            CheckOutcome::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DoctorCheck {
    pub id: String,
    pub category: String,
    pub title: String,
    pub outcome: CheckOutcome,
    pub detail: String,
    pub remedy: Option<String>,
}

impl DoctorCheck {
    pub fn passed(id: &str, category: &str, title: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            category: category.to_string(),
            title: title.to_string(),
            outcome: CheckOutcome::Passed,
            detail: detail.into(),
            remedy: None,
        }
    }

    pub fn failed(
        id: &str,
        category: &str,
        title: &str,
        detail: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            id: id.to_string(),
            category: category.to_string(),
            title: title.to_string(),
            outcome: CheckOutcome::Failed,
            detail: detail.into(),
            remedy: Some(remedy.into()),
        }
    }

    pub fn warning(
        id: &str,
        category: &str,
        title: &str,
        detail: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            id: id.to_string(),
            category: category.to_string(),
            title: title.to_string(),
            outcome: CheckOutcome::Warning,
            detail: detail.into(),
            remedy: Some(remedy.into()),
        }
    }

    pub fn skipped(id: &str, category: &str, title: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            category: category.to_string(),
            title: title.to_string(),
            outcome: CheckOutcome::Skipped,
            detail: detail.into(),
            remedy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdapterStatus {
    pub name: String,
    pub kind: String,
    pub available: bool,
    pub version: Option<String>,
    pub requires_paid_account: bool,
    pub isolation: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DoctorReport {
    pub repository_path: Option<String>,
    pub checks: Vec<DoctorCheck>,
    pub adapters: Vec<AdapterStatus>,
    pub ready: bool,
    pub free_path_available: bool,
}

impl DoctorReport {
    pub fn recompute(&mut self) {
        self.ready = !self
            .checks
            .iter()
            .any(|check| check.outcome == CheckOutcome::Failed);
    }

    pub fn failures(&self) -> Vec<&DoctorCheck> {
        self.checks
            .iter()
            .filter(|check| check.outcome == CheckOutcome::Failed)
            .collect()
    }
}
