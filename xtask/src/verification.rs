use std::path::PathBuf;
use std::time::Instant;

use crate::error::{TaskError, TaskResult};
use crate::workspace::{cargo_executable, stream, workspace_root};

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub skip_browser: bool,
    pub skip_media: bool,
    pub fail_fast: bool,
}

struct Step {
    name: &'static str,
    program: String,
    arguments: Vec<String>,
    directory: PathBuf,
}

impl Step {
    fn cargo(name: &'static str, arguments: &[&str]) -> Self {
        Self {
            name,
            program: cargo_executable(),
            arguments: arguments.iter().map(|value| (*value).to_string()).collect(),
            directory: workspace_root(),
        }
    }

    fn web(name: &'static str, script: &str) -> Self {
        Self {
            name,
            program: "pnpm".to_string(),
            arguments: vec![
                "--dir".to_string(),
                "apps/web".to_string(),
                "run".to_string(),
                script.to_string(),
            ],
            directory: workspace_root(),
        }
    }
}

fn describe_skipped(name: &str) {
    println!("--- {name} was NOT run in this environment ---");
}

pub fn run(options: Options) -> TaskResult<()> {
    let mut steps = vec![
        Step::cargo("format checks", &["fmt", "--all", "--", "--check"]),
        Step::cargo(
            "Rust lint with warnings denied",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        Step::cargo("Rust tests", &["test", "--workspace"]),
        Step::web("frontend lint", "lint"),
        Step::web("frontend type checking", "typecheck"),
        Step::web("frontend format", "format:check"),
        Step::web("frontend tests", "test"),
        Step::cargo(
            "integration tests",
            &["test", "--workspace", "--test", "graph_integration"],
        ),
        Step::cargo(
            "policy checks",
            &[
                "run",
                "-q",
                "-p",
                "heikas-cli",
                "--",
                "--plain",
                "policy",
                ".",
            ],
        ),
        Step::web("interface bundle build", "build"),
        Step {
            name: "schema and wire type drift",
            program: cargo_executable(),
            arguments: vec![
                "run".to_string(),
                "-q".to_string(),
                "-p".to_string(),
                "xtask".to_string(),
                "--".to_string(),
                "drift".to_string(),
            ],
            directory: workspace_root(),
        },
        Step {
            name: "dependency advisories and licences",
            program: cargo_executable(),
            arguments: vec!["deny".to_string(), "check".to_string()],
            directory: workspace_root(),
        },
        Step {
            name: "interface dependency audit",
            program: "pnpm".to_string(),
            arguments: vec![
                "audit".to_string(),
                "--audit-level".to_string(),
                "high".to_string(),
            ],
            directory: workspace_root(),
        },
    ];

    if !options.skip_browser {
        steps.push(Step::web("Playwright tests", "e2e"));
    }
    if !options.skip_media {
        steps.push(Step {
            name: "README media validation",
            program: cargo_executable(),
            arguments: vec![
                "run".to_string(),
                "-q".to_string(),
                "-p".to_string(),
                "xtask".to_string(),
                "--".to_string(),
                "media".to_string(),
                "--validate-only".to_string(),
            ],
            directory: workspace_root(),
        });
    }
    steps.push(Step {
        name: "authorship validation",
        program: cargo_executable(),
        arguments: vec![
            "run".to_string(),
            "-q".to_string(),
            "-p".to_string(),
            "xtask".to_string(),
            "--".to_string(),
            "authorship".to_string(),
        ],
        directory: workspace_root(),
    });
    steps.push(Step::cargo(
        "release smoke build",
        &["build", "--release", "-p", "heikas-cli"],
    ));

    let mut failures = Vec::new();
    for step in &steps {
        let started = Instant::now();
        println!();
        println!("=== {} ===", step.name);
        let arguments: Vec<&str> = step.arguments.iter().map(String::as_str).collect();
        let succeeded = stream(&step.program, &arguments, &step.directory, &[])?;
        let elapsed = started.elapsed();
        if succeeded {
            println!(
                "--- {} passed in {:.1}s ---",
                step.name,
                elapsed.as_secs_f64()
            );
        } else {
            println!(
                "--- {} FAILED after {:.1}s ---",
                step.name,
                elapsed.as_secs_f64()
            );
            failures.push(step.name);
            if options.fail_fast {
                break;
            }
        }
    }

    println!();
    if options.skip_browser {
        describe_skipped("Playwright tests");
    }
    if options.skip_media {
        describe_skipped("README media validation");
    }
    if failures.is_empty() {
        if options.skip_browser || options.skip_media {
            println!(
                "Every verification step that ran passed. The steps listed above were skipped."
            );
        } else {
            println!("Every verification step passed.");
        }
        return Ok(());
    }
    for failure in &failures {
        eprintln!("failed: {failure}");
    }
    Err(TaskError::StepFailed {
        step: failures.join(", "),
    })
}

pub fn regenerate_schemas() -> TaskResult<()> {
    let root = workspace_root();
    let succeeded = stream(
        &cargo_executable(),
        &[
            "run",
            "-q",
            "-p",
            "heikas-cli",
            "--",
            "schemas",
            "--output",
            "schemas",
        ],
        &root,
        &[],
    )?;
    if !succeeded {
        return Err(TaskError::StepFailed {
            step: "schemas".to_string(),
        });
    }
    let succeeded = stream(
        "pnpm",
        &["--dir", "apps/web", "run", "generate:types"],
        &root,
        &[],
    )?;
    if !succeeded {
        return Err(TaskError::StepFailed {
            step: "generate:types".to_string(),
        });
    }
    println!("Schemas and generated wire types are up to date.");
    Ok(())
}

pub fn check_schema_drift() -> TaskResult<()> {
    regenerate_schemas()?;
    let root = workspace_root();
    let succeeded = stream(
        "git",
        &[
            "diff",
            "--exit-code",
            "--",
            "schemas",
            "apps/web/src/generated",
        ],
        &root,
        &[],
    )?;
    if !succeeded {
        return Err(TaskError::StepFailed {
            step: "schema and wire type drift".to_string(),
        });
    }
    println!("The published schemas and generated wire types match the source.");
    Ok(())
}
