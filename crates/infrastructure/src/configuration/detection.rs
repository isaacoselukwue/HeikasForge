use std::path::Path;

use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{
    CommandId, CommandKind, CommandSpecification, ReportFormat, MAXIMUM_COMMAND_TIMEOUT_SECONDS,
};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Rust,
    NodeJavaScript,
    Python,
    Go,
    Unknown,
}

impl ProjectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectKind::Rust => "rust",
            ProjectKind::NodeJavaScript => "node",
            ProjectKind::Python => "python",
            ProjectKind::Go => "go",
            ProjectKind::Unknown => "unknown",
        }
    }
}

pub fn detect_project_kind(repository: &Path) -> ProjectKind {
    if repository.join("Cargo.toml").exists() {
        return ProjectKind::Rust;
    }
    if repository.join("go.mod").exists() {
        return ProjectKind::Go;
    }
    if repository.join("package.json").exists() {
        return ProjectKind::NodeJavaScript;
    }
    if repository.join("pyproject.toml").exists()
        || repository.join("setup.py").exists()
        || repository.join("setup.cfg").exists()
        || repository.join("requirements.txt").exists()
    {
        return ProjectKind::Python;
    }
    ProjectKind::Unknown
}

pub fn proposed_commands(kind: ProjectKind) -> Vec<CommandSpecification> {
    match kind {
        ProjectKind::Rust => vec![
            command("format", CommandKind::Format, "cargo", &["fmt", "--all", "--", "--check"], 180, true),
            command(
                "lint",
                CommandKind::Lint,
                "cargo",
                &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
                900,
                true,
            ),
            command("test", CommandKind::Test, "cargo", &["test", "--workspace"], 1_200, true),
        ],
        ProjectKind::NodeJavaScript => vec![
            command("lint", CommandKind::Lint, "npm", &["run", "lint"], 600, true),
            command("test", CommandKind::Test, "npm", &["test"], 900, true),
        ],
        ProjectKind::Python => vec![
            command("lint", CommandKind::Lint, "python3", &["-m", "ruff", "check", "."], 600, false),
            command("test", CommandKind::Test, "python3", &["-m", "pytest", "-q"], 900, true),
        ],
        ProjectKind::Go => vec![
            command("format", CommandKind::Format, "gofmt", &["-l", "."], 180, true),
            command("lint", CommandKind::Lint, "go", &["vet", "./..."], 600, true),
            command("test", CommandKind::Test, "go", &["test", "./..."], 900, true),
        ],
        ProjectKind::Unknown => Vec::new(),
    }
}

fn command(
    id: &str,
    kind: CommandKind,
    program: &str,
    args: &[&str],
    timeout_seconds: u32,
    required: bool,
) -> CommandSpecification {
    CommandSpecification {
        id: CommandId::from_str(id).unwrap_or_else(|_| CommandId::from_str("command").expect("literal command identifier is valid")),
        kind,
        program: program.to_string(),
        args: args.iter().map(|value| (*value).to_string()).collect(),
        working_subdirectory: None,
        timeout: TimeoutSeconds::clamped(timeout_seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
        required,
        report_format: ReportFormat::None,
        report_path: None,
        environment: Vec::new(),
        success_exit_codes: vec![0],
    }
}
