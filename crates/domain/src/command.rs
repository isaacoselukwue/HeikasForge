use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::clock::TimeoutSeconds;
use crate::error::DomainError;

pub const MAXIMUM_COMMAND_TIMEOUT_SECONDS: u32 = 7_200;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(String);

impl CommandId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for CommandId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '-'
                    || character == '_'
            });
        if !valid {
            return Err(DomainError::InvalidIdentifier {
                kind: "CommandId",
                value: value.to_string(),
            });
        }
        Ok(Self(value.to_string()))
    }
}

impl schemars::JsonSchema for CommandId {
    fn schema_name() -> String {
        "CommandId".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
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
pub enum CommandKind {
    Format,
    Lint,
    Test,
    Coverage,
    Audit,
    SecretScan,
    StaticAnalysis,
    Policy,
    Build,
}

impl CommandKind {
    pub const ALL: [CommandKind; 9] = [
        CommandKind::Format,
        CommandKind::Lint,
        CommandKind::Test,
        CommandKind::Coverage,
        CommandKind::Audit,
        CommandKind::SecretScan,
        CommandKind::StaticAnalysis,
        CommandKind::Policy,
        CommandKind::Build,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            CommandKind::Format => "format",
            CommandKind::Lint => "lint",
            CommandKind::Test => "test",
            CommandKind::Coverage => "coverage",
            CommandKind::Audit => "audit",
            CommandKind::SecretScan => "secret_scan",
            CommandKind::StaticAnalysis => "static_analysis",
            CommandKind::Policy => "policy",
            CommandKind::Build => "build",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CommandKind::Format => "Format check",
            CommandKind::Lint => "Lint",
            CommandKind::Test => "Tests",
            CommandKind::Coverage => "Coverage",
            CommandKind::Audit => "Dependency audit",
            CommandKind::SecretScan => "Secret scan",
            CommandKind::StaticAnalysis => "Static analysis",
            CommandKind::Policy => "Repository policy",
            CommandKind::Build => "Build",
        }
    }

    pub fn is_test_phase(&self) -> bool {
        matches!(
            self,
            CommandKind::Test | CommandKind::Coverage | CommandKind::Build
        )
    }

    pub fn is_review_phase(&self) -> bool {
        !self.is_test_phase()
    }

    pub fn default_issue_category(&self) -> crate::review::IssueCategory {
        match self {
            CommandKind::Format => crate::review::IssueCategory::Formatting,
            CommandKind::Lint | CommandKind::Build => crate::review::IssueCategory::Maintainability,
            CommandKind::Test => crate::review::IssueCategory::Reliability,
            CommandKind::Coverage => crate::review::IssueCategory::Coverage,
            CommandKind::Audit => crate::review::IssueCategory::Dependency,
            CommandKind::SecretScan => crate::review::IssueCategory::Secret,
            CommandKind::StaticAnalysis => crate::review::IssueCategory::Security,
            CommandKind::Policy => crate::review::IssueCategory::Policy,
        }
    }
}

impl fmt::Display for CommandKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CommandKind {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        CommandKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "CommandKind",
                value: value.to_string(),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    None,
    JUnitXml,
    Lcov,
    Sarif,
    CargoTestJson,
    CargoTestText,
    GoTestJson,
    PytestText,
    NodeTestText,
    CTestText,
    Text,
}

impl ReportFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportFormat::None => "none",
            ReportFormat::JUnitXml => "junit_xml",
            ReportFormat::Lcov => "lcov",
            ReportFormat::Sarif => "sarif",
            ReportFormat::CargoTestJson => "cargo_test_json",
            ReportFormat::CargoTestText => "cargo_test_text",
            ReportFormat::GoTestJson => "go_test_json",
            ReportFormat::PytestText => "pytest_text",
            ReportFormat::NodeTestText => "node_test_text",
            ReportFormat::CTestText => "ctest_text",
            ReportFormat::Text => "text",
        }
    }
}

impl ReportFormat {
    pub fn reads_stdout_only(&self) -> bool {
        matches!(
            self,
            ReportFormat::CargoTestJson
                | ReportFormat::CargoTestText
                | ReportFormat::GoTestJson
                | ReportFormat::PytestText
                | ReportFormat::NodeTestText
                | ReportFormat::CTestText
        )
    }

    pub fn counts_executed_tests(&self) -> bool {
        matches!(
            self,
            ReportFormat::JUnitXml
                | ReportFormat::CargoTestJson
                | ReportFormat::CargoTestText
                | ReportFormat::GoTestJson
                | ReportFormat::PytestText
                | ReportFormat::NodeTestText
                | ReportFormat::CTestText
        )
    }
}

impl FromStr for ReportFormat {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(ReportFormat::None),
            "junit_xml" => Ok(ReportFormat::JUnitXml),
            "lcov" => Ok(ReportFormat::Lcov),
            "sarif" => Ok(ReportFormat::Sarif),
            "cargo_test_json" => Ok(ReportFormat::CargoTestJson),
            "cargo_test_text" => Ok(ReportFormat::CargoTestText),
            "go_test_json" => Ok(ReportFormat::GoTestJson),
            "pytest_text" => Ok(ReportFormat::PytestText),
            "node_test_text" => Ok(ReportFormat::NodeTestText),
            "ctest_text" => Ok(ReportFormat::CTestText),
            "text" => Ok(ReportFormat::Text),
            other => Err(DomainError::InvalidIdentifier {
                kind: "ReportFormat",
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommandSpecification {
    pub id: CommandId,
    pub kind: CommandKind,
    pub program: String,
    pub args: Vec<String>,
    pub working_subdirectory: Option<String>,
    pub timeout: TimeoutSeconds,
    pub required: bool,
    pub report_format: ReportFormat,
    pub report_path: Option<String>,
    pub environment: Vec<(String, String)>,
    pub success_exit_codes: Vec<i32>,
}

impl CommandSpecification {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.program.trim().is_empty() {
            return Err(DomainError::MissingField { field: "program" });
        }
        if self.program.contains(['\n', '\r', '\0']) {
            return Err(DomainError::InvalidIdentifier {
                kind: "CommandProgram",
                value: self.program.clone(),
            });
        }
        for argument in &self.args {
            if argument.contains('\0') {
                return Err(DomainError::InvalidIdentifier {
                    kind: "CommandArgument",
                    value: argument.clone(),
                });
            }
        }
        if let Some(subdirectory) = &self.working_subdirectory {
            crate::path_policy::RelativeWorkspacePath::parse(subdirectory)?;
        }
        if let Some(report_path) = &self.report_path {
            crate::path_policy::RelativeWorkspacePath::parse(report_path)?;
        }
        if self.report_format != ReportFormat::None
            && !self.report_format.reads_stdout_only()
            && self.report_path.is_none()
        {
            return Err(DomainError::MissingField {
                field: "report_path",
            });
        }
        Ok(())
    }

    pub fn is_success(&self, exit_code: Option<i32>) -> bool {
        match exit_code {
            Some(code) => {
                if self.success_exit_codes.is_empty() {
                    code == 0
                } else {
                    self.success_exit_codes.contains(&code)
                }
            }
            None => false,
        }
    }

    pub fn display_line(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct CommandCatalogue {
    pub commands: Vec<CommandSpecification>,
}

impl CommandCatalogue {
    pub fn find(&self, id: &CommandId) -> Option<&CommandSpecification> {
        self.commands.iter().find(|command| &command.id == id)
    }

    pub fn of_kind(&self, kind: CommandKind) -> Vec<&CommandSpecification> {
        self.commands
            .iter()
            .filter(|command| command.kind == kind)
            .collect()
    }

    pub fn test_phase(&self) -> Vec<&CommandSpecification> {
        self.commands
            .iter()
            .filter(|command| command.kind.is_test_phase())
            .collect()
    }

    pub fn review_phase(&self) -> Vec<&CommandSpecification> {
        self.commands
            .iter()
            .filter(|command| command.kind.is_review_phase())
            .collect()
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        let mut seen = Vec::new();
        for command in &self.commands {
            command.validate()?;
            if seen.contains(&command.id) {
                return Err(DomainError::InvariantViolated(format!(
                    "command identifier `{}` is declared more than once",
                    command.id
                )));
            }
            seen.push(command.id.clone());
        }
        Ok(())
    }
}
