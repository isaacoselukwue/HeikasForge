use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Warning,
    Violation,
}

impl FindingSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingSeverity::Warning => "warning",
            FindingSeverity::Violation => "violation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PolicyFinding {
    pub rule: String,
    pub severity: FindingSeverity,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub remedy: String,
}

impl PolicyFinding {
    pub fn violation(rule: &str, message: impl Into<String>, remedy: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            severity: FindingSeverity::Violation,
            path: None,
            line: None,
            column: None,
            message: message.into(),
            remedy: remedy.into(),
        }
    }

    pub fn at(mut self, path: impl Into<String>, line: u32, column: u32) -> Self {
        self.path = Some(path.into());
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn in_file(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct PolicyReport {
    pub findings: Vec<PolicyFinding>,
    pub files_checked: u64,
    pub rules_run: Vec<String>,
}

impl PolicyReport {
    pub fn violations(&self) -> impl Iterator<Item = &PolicyFinding> {
        self.findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Violation)
    }

    pub fn passed(&self) -> bool {
        self.violations().count() == 0
    }

    pub fn extend(&mut self, findings: Vec<PolicyFinding>) {
        self.findings.extend(findings);
    }
}
