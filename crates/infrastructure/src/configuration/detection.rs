use std::collections::BTreeSet;
use std::path::Path;
use std::str::FromStr;

use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{
    CommandCatalogue, CommandId, CommandKind, CommandSpecification, ReportFormat,
    MAXIMUM_COMMAND_TIMEOUT_SECONDS,
};
use serde::{Deserialize, Serialize};

use crate::paths::read_confined_file;
use crate::quality::integrity::is_test_path;

const MAXIMUM_MANIFEST_BYTES: u64 = 1_048_576;
const MAXIMUM_SURVEY_DEPTH: usize = 3;

const VENDORED_SEGMENTS: [&str; 12] = [
    "node_modules",
    "vendor",
    "third_party",
    "thirdparty",
    "target",
    "dist",
    "build",
    "out",
    ".venv",
    "venv",
    "__pycache__",
    "site-packages",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Rust,
    Go,
    Python,
    Node,
    Cmake,
}

impl Ecosystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            Ecosystem::Rust => "rust",
            Ecosystem::Go => "go",
            Ecosystem::Python => "python",
            Ecosystem::Node => "node",
            Ecosystem::Cmake => "cmake",
        }
    }

    fn markers(&self) -> &'static [&'static str] {
        match self {
            Ecosystem::Rust => &["Cargo.toml"],
            Ecosystem::Go => &["go.mod"],
            Ecosystem::Python => &[
                "pyproject.toml",
                "setup.py",
                "setup.cfg",
                "requirements.txt",
            ],
            Ecosystem::Node => &["package.json"],
            Ecosystem::Cmake => &["CMakeLists.txt"],
        }
    }

    fn aggregates_subdirectories(&self) -> bool {
        matches!(self, Ecosystem::Rust | Ecosystem::Go)
    }
}

pub const SURVEYED_MARKERS: [&str; 8] = [
    "CMakeLists.txt",
    "Cargo.toml",
    "go.mod",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "requirements.txt",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedModule {
    pub ecosystem: Ecosystem,
    pub directory: Option<String>,
    pub marker: String,
}

impl DetectedModule {
    fn label(&self) -> String {
        match &self.directory {
            Some(directory) => format!("{} in `{directory}`", self.ecosystem.as_str()),
            None => format!("{} at the repository root", self.ecosystem.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurveyDecline {
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSurvey {
    pub modules: Vec<DetectedModule>,
    pub commands: Vec<CommandSpecification>,
    pub declines: Vec<SurveyDecline>,
    pub tracked_listing_available: bool,
}

impl ProjectSurvey {
    pub fn describe_kinds(&self) -> String {
        if self.modules.is_empty() {
            return "unknown".to_string();
        }
        self.modules
            .iter()
            .map(|module| module.ecosystem.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn directory_of(path: &str) -> Option<String> {
    path.rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
}

fn file_name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn depth_of(directory: &Option<String>) -> usize {
    match directory {
        Some(directory) => directory.split('/').count(),
        None => 0,
    }
}

fn is_vendored(path: &str) -> bool {
    path.split('/')
        .any(|segment| VENDORED_SEGMENTS.contains(&segment) || segment.starts_with('.'))
}

fn within(directory: &Option<String>, path: &str) -> bool {
    match directory {
        Some(directory) => path.starts_with(&format!("{directory}/")),
        None => true,
    }
}

fn module_relative(directory: &Option<String>, path: &str) -> String {
    match directory {
        Some(directory) => path
            .strip_prefix(&format!("{directory}/"))
            .unwrap_or(path)
            .to_string(),
        None => path.to_string(),
    }
}

pub fn survey_project(repository: &Path, tracked: Option<&[String]>) -> ProjectSurvey {
    let Some(tracked) = tracked else {
        return ProjectSurvey {
            declines: vec![SurveyDecline {
                subject: "repository".to_string(),
                detail:
                    "the tracked file listing could not be read, so no project could be surveyed"
                        .to_string(),
            }],
            ..ProjectSurvey::default()
        };
    };

    let mut survey = ProjectSurvey {
        tracked_listing_available: true,
        ..ProjectSurvey::default()
    };

    let mut discovered: Vec<DetectedModule> = Vec::new();
    for path in tracked {
        if is_vendored(path) {
            continue;
        }
        let directory = directory_of(path);
        if depth_of(&directory) > MAXIMUM_SURVEY_DEPTH {
            continue;
        }
        let name = file_name_of(path);
        for ecosystem in [
            Ecosystem::Rust,
            Ecosystem::Go,
            Ecosystem::Node,
            Ecosystem::Python,
            Ecosystem::Cmake,
        ] {
            if ecosystem.markers().contains(&name) {
                discovered.push(DetectedModule {
                    ecosystem,
                    directory: directory.clone(),
                    marker: name.to_string(),
                });
            }
        }
    }

    discovered.sort_by(|left, right| {
        depth_of(&left.directory)
            .cmp(&depth_of(&right.directory))
            .then_with(|| left.ecosystem.cmp(&right.ecosystem))
            .then_with(|| left.directory.cmp(&right.directory))
    });

    let mut selected: Vec<DetectedModule> = Vec::new();
    for module in discovered {
        let already = selected.iter().any(|existing| {
            existing.ecosystem == module.ecosystem
                && (existing.ecosystem.aggregates_subdirectories()
                    || existing.directory == module.directory)
        });
        if !already {
            selected.push(module);
        }
    }

    for module in &selected {
        match module.ecosystem {
            Ecosystem::Rust => propose_rust(module, &mut survey),
            Ecosystem::Go => propose_go(module, &mut survey),
            Ecosystem::Python => propose_python(module, tracked, &mut survey),
            Ecosystem::Node => propose_node(repository, module, tracked, &mut survey),
            Ecosystem::Cmake => propose_cmake(repository, module, tracked, &mut survey),
        }
    }

    survey.modules = selected;
    let catalogue = CommandCatalogue {
        commands: survey.commands.clone(),
    };
    if let Err(error) = catalogue.validate() {
        survey.declines.push(SurveyDecline {
            subject: "repository".to_string(),
            detail: format!("the proposed commands were discarded because they are not a valid catalogue: {error}"),
        });
        survey.commands.clear();
    }
    survey
}

struct Proposal<'a> {
    suffix: &'a str,
    kind: CommandKind,
    program: &'a str,
    args: &'a [&'a str],
    timeout_seconds: u32,
    required: bool,
    report_format: ReportFormat,
}

fn command(module: &DetectedModule, proposal: Proposal<'_>) -> Option<CommandSpecification> {
    let Proposal {
        suffix,
        kind,
        program,
        args,
        timeout_seconds,
        required,
        report_format,
    } = proposal;
    let identifier = match &module.directory {
        Some(directory) => format!(
            "{}-{}-{suffix}",
            module.ecosystem.as_str(),
            slug_of(directory)
        ),
        None => format!("{}-{suffix}", module.ecosystem.as_str()),
    };
    let id = CommandId::from_str(&identifier).ok()?;
    let specification = CommandSpecification {
        id,
        kind,
        program: program.to_string(),
        args: args.iter().map(|value| (*value).to_string()).collect(),
        working_subdirectory: module.directory.clone(),
        timeout: TimeoutSeconds::clamped(timeout_seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
        required,
        report_format,
        report_path: None,
        environment: Vec::new(),
        success_exit_codes: vec![0],
    };
    specification.validate().ok()?;
    Some(specification)
}

fn slug_of(directory: &str) -> String {
    let mapped: String = directory
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed: String = mapped
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    trimmed.chars().take(40).collect()
}

fn push(survey: &mut ProjectSurvey, specification: Option<CommandSpecification>) {
    if let Some(specification) = specification {
        survey.commands.push(specification);
    }
}

fn propose_rust(module: &DetectedModule, survey: &mut ProjectSurvey) {
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "format",
                kind: CommandKind::Format,
                program: "cargo",
                args: &["fmt", "--all", "--", "--check"],
                timeout_seconds: 180,
                required: false,
                report_format: ReportFormat::None,
            },
        ),
    );
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "lint",
                kind: CommandKind::Lint,
                program: "cargo",
                args: &["clippy", "--workspace", "--all-targets"],
                timeout_seconds: 900,
                required: false,
                report_format: ReportFormat::None,
            },
        ),
    );
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "test",
                kind: CommandKind::Test,
                program: "cargo",
                args: &["test", "--workspace", "--no-fail-fast"],
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::CargoTestText,
            },
        ),
    );
}

fn propose_go(module: &DetectedModule, survey: &mut ProjectSurvey) {
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "lint",
                kind: CommandKind::Lint,
                program: "go",
                args: &["vet", "./..."],
                timeout_seconds: 600,
                required: false,
                report_format: ReportFormat::None,
            },
        ),
    );
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "test",
                kind: CommandKind::Test,
                program: "go",
                args: &["test", "-count=1", "-json", "./..."],
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::GoTestJson,
            },
        ),
    );
}

fn propose_python(module: &DetectedModule, tracked: &[String], survey: &mut ProjectSurvey) {
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "lint",
                kind: CommandKind::Lint,
                program: "python3",
                args: &["-m", "ruff", "check", "."],
                timeout_seconds: 600,
                required: false,
                report_format: ReportFormat::None,
            },
        ),
    );
    let has_tests = tracked
        .iter()
        .filter(|path| !is_vendored(path) && within(&module.directory, path))
        .any(|path| {
            let relative = module_relative(&module.directory, path);
            relative.ends_with(".py") && is_test_path(&relative)
        });
    if !has_tests {
        survey.declines.push(SurveyDecline {
            subject: module.label(),
            detail: "no tracked Python test file was found, so no test command was proposed"
                .to_string(),
        });
        return;
    }
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "test",
                kind: CommandKind::Test,
                program: "python3",
                args: &["-m", "pytest", "-q"],
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::PytestText,
            },
        ),
    );
}

const NODE_RUNNERS: [&str; 3] = ["vitest", "jest", "mocha"];

const NODE_MANAGERS: [(&str, &str, &[&str]); 4] = [
    ("package-lock.json", "npm", &["ci", "--ignore-scripts"]),
    (
        "pnpm-lock.yaml",
        "pnpm",
        &["install", "--frozen-lockfile", "--ignore-scripts"],
    ),
    (
        "yarn.lock",
        "yarn",
        &["install", "--immutable", "--mode=skip-build"],
    ),
    (
        "bun.lockb",
        "bun",
        &["install", "--frozen-lockfile", "--ignore-scripts"],
    ),
];

fn declared_script<'a>(manifest: Option<&'a serde_json::Value>, name: &str) -> Option<&'a str> {
    manifest?.get("scripts")?.get(name)?.as_str()
}

fn recognised_node_runner(manifest: Option<&serde_json::Value>) -> Option<&'static str> {
    if let Some(script) = declared_script(manifest, "test") {
        if script.contains("node --test") || script.contains("node:test") {
            return Some("the Node built in test runner");
        }
    }
    let development = manifest.and_then(|document| document.get("devDependencies"));
    let dependencies = manifest.and_then(|document| document.get("dependencies"));
    for runner in NODE_RUNNERS {
        let present = development.and_then(|value| value.get(runner)).is_some()
            || dependencies.and_then(|value| value.get(runner)).is_some();
        if present {
            return Some(match runner {
                "vitest" => "vitest",
                "jest" => "jest",
                _ => "mocha",
            });
        }
        if declared_script(manifest, "test").is_some_and(|script| script.contains(runner)) {
            return Some(match runner {
                "vitest" => "vitest",
                "jest" => "jest",
                _ => "mocha",
            });
        }
    }
    None
}

fn propose_node(
    repository: &Path,
    module: &DetectedModule,
    tracked: &[String],
    survey: &mut ProjectSurvey,
) {
    let manifest_path = match &module.directory {
        Some(directory) => format!("{directory}/package.json"),
        None => "package.json".to_string(),
    };
    let manifest = read_confined_file(repository, &manifest_path, MAXIMUM_MANIFEST_BYTES)
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());

    if declared_script(manifest.as_ref(), "test").is_none() {
        survey.declines.push(SurveyDecline {
            subject: module.label(),
            detail: "`package.json` declares no `test` script, so no test command was proposed"
                .to_string(),
        });
        return;
    }

    let Some(runner) = recognised_node_runner(manifest.as_ref()) else {
        survey.declines.push(SurveyDecline {
            subject: module.label(),
            detail: "the test runner could not be identified from `package.json`, so the number of tests it executes could not be read and no test command was proposed. Vitest, Jest, Mocha and the Node built in runner are recognised".to_string(),
        });
        return;
    };

    let manager = NODE_MANAGERS.iter().find(|(lockfile, _, _)| {
        let candidate = match &module.directory {
            Some(directory) => format!("{directory}/{lockfile}"),
            None => (*lockfile).to_string(),
        };
        tracked.iter().any(|path| path == &candidate)
    });
    let Some((_, program, install)) = manager else {
        survey.declines.push(SurveyDecline {
            subject: module.label(),
            detail: "no tracked lockfile was found, so dependencies cannot be installed reproducibly in a clean worktree and no test command was proposed".to_string(),
        });
        return;
    };

    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "install",
                kind: CommandKind::Build,
                program,
                args: install,
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::None,
            },
        ),
    );
    if declared_script(manifest.as_ref(), "lint").is_some() {
        push(
            survey,
            command(
                module,
                Proposal {
                    suffix: "lint",
                    kind: CommandKind::Lint,
                    program,
                    args: &["run", "lint"],
                    timeout_seconds: 900,
                    required: false,
                    report_format: ReportFormat::None,
                },
            ),
        );
    }
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "test",
                kind: CommandKind::Test,
                program,
                args: &["run", "test"],
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::NodeTestText,
            },
        ),
    );
    survey.declines.push(SurveyDecline {
        subject: module.label(),
        detail: format!(
            "the test command was proposed on the understanding that it reports its results through {runner}, and dependencies are installed without running lifecycle scripts"
        ),
    });
}

fn propose_cmake(
    repository: &Path,
    module: &DetectedModule,
    tracked: &[String],
    survey: &mut ProjectSurvey,
) {
    let ignore_path = match &module.directory {
        Some(directory) => format!("{directory}/.gitignore"),
        None => ".gitignore".to_string(),
    };
    let ignores_build = read_confined_file(repository, &ignore_path, MAXIMUM_MANIFEST_BYTES)
        .ok()
        .flatten()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .is_some_and(|contents| {
            contents.lines().any(|line| {
                let trimmed = line.trim().trim_end_matches('/');
                trimmed == "build" || trimmed == "/build"
            })
        });
    let _ = tracked;
    if !ignores_build {
        survey.declines.push(SurveyDecline {
            subject: module.label(),
            detail: "no tracked `.gitignore` ignores a `build` directory, so a configured build would appear in the candidate diff and no test command was proposed".to_string(),
        });
        return;
    }

    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "configure",
                kind: CommandKind::Build,
                program: "cmake",
                args: &["-S", ".", "-B", "build"],
                timeout_seconds: 900,
                required: true,
                report_format: ReportFormat::None,
            },
        ),
    );
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "build",
                kind: CommandKind::Build,
                program: "cmake",
                args: &["--build", "build"],
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::None,
            },
        ),
    );
    push(
        survey,
        command(
            module,
            Proposal {
                suffix: "test",
                kind: CommandKind::Test,
                program: "ctest",
                args: &[
                    "--test-dir",
                    "build",
                    "--no-tests=error",
                    "--output-on-failure",
                ],
                timeout_seconds: 1_800,
                required: true,
                report_format: ReportFormat::CTestText,
            },
        ),
    );
}
