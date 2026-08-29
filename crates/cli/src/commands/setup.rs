use std::path::Path;

use heikas_application::error::ApplicationResult;
use serde::Serialize;

use crate::context::CommandContext;
use crate::exit::ExitCode;
use crate::internal_notes;
use crate::presentation::{Palette, Table};

#[derive(Debug, Serialize)]
pub struct InitOutcome {
    pub configuration_path: String,
    pub project_kind: String,
    pub commands: Vec<String>,
    pub internal_notes_path: String,
}

pub async fn init(
    context: &CommandContext,
    path: &Path,
    force: bool,
) -> ApplicationResult<ExitCode> {
    let repository = heikas_infrastructure::paths::canonical_root(path)?;
    let configuration = context.runtime.configuration.detect(&repository).await?;
    let target = repository.join(".heikas/forge.toml");
    if target.exists() && !force {
        context.note(&format!(
            "`{}` already exists. Pass --force to overwrite it.",
            target.display()
        ));
        return Ok(ExitCode::InvalidUsage);
    }
    let written = context
        .runtime
        .configuration
        .write_repository_configuration(&repository, &configuration)
        .await?;
    let notes = internal_notes::write(&repository)?;
    let outcome = InitOutcome {
        configuration_path: written.display().to_string(),
        project_kind: heikas_infrastructure::configuration::detection::detect_project_kind(
            &repository,
        )
        .as_str()
        .to_string(),
        commands: configuration
            .commands
            .commands
            .iter()
            .map(|command| format!("{} ({})", command.id, command.display_line()))
            .collect(),
        internal_notes_path: notes.display().to_string(),
    };
    context.emit(&outcome, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Repository prepared\n"));
        text.push_str(&format!("Configuration: {}\n", outcome.configuration_path));
        text.push_str(&format!("Project kind: {}\n", outcome.project_kind));
        text.push_str(&format!(
            "Internal notes: {}\n",
            outcome.internal_notes_path
        ));
        text.push_str("Commands:\n");
        for command in &outcome.commands {
            text.push_str(&format!("  {command}\n"));
        }
        text
    });
    Ok(ExitCode::Success)
}

pub async fn doctor(context: &CommandContext, path: &Path) -> ApplicationResult<ExitCode> {
    let repository = heikas_infrastructure::paths::canonical_root(path).ok();
    let report = context.service().diagnose(repository.as_deref()).await?;
    let ready = report.ready;
    context.emit(&report, |palette| render_doctor(&report, palette));
    Ok(if ready {
        ExitCode::Success
    } else {
        ExitCode::InvalidUsage
    })
}

fn render_doctor(
    report: &heikas_application::model::doctor::DoctorReport,
    palette: &Palette,
) -> String {
    let mut text = String::new();
    text.push_str(&palette.heading("Environment diagnosis\n"));
    let mut table = Table::new(&["Check", "Outcome", "Detail"]);
    for check in &report.checks {
        let outcome = match check.outcome {
            heikas_application::model::doctor::CheckOutcome::Passed => {
                palette.success(check.outcome.label())
            }
            heikas_application::model::doctor::CheckOutcome::Warning => {
                palette.warning(check.outcome.label())
            }
            heikas_application::model::doctor::CheckOutcome::Failed => {
                palette.failure(check.outcome.label())
            }
            heikas_application::model::doctor::CheckOutcome::Skipped => {
                palette.muted(check.outcome.label())
            }
        };
        table.push(vec![check.title.clone(), outcome, check.detail.clone()]);
    }
    text.push_str(&table.render(palette));
    if !report.adapters.is_empty() {
        text.push_str(&palette.heading("\nAdapters\n"));
        let mut adapters = Table::new(&["Adapter", "Kind", "Available", "Paid account", "Detail"]);
        for adapter in &report.adapters {
            adapters.push(vec![
                adapter.name.clone(),
                adapter.kind.clone(),
                if adapter.available {
                    palette.success("yes")
                } else {
                    palette.failure("no")
                },
                if adapter.requires_paid_account {
                    palette.warning("required")
                } else {
                    palette.success("not required")
                },
                adapter.detail.clone(),
            ]);
        }
        text.push_str(&adapters.render(palette));
    }
    let failures: Vec<_> = report.failures();
    if failures.is_empty() {
        text.push_str(&palette.success("\nThe environment is ready.\n"));
    } else {
        text.push_str(&palette.failure(&format!(
            "\n{} checks failed. Correct them before starting a run.\n",
            failures.len()
        )));
        for failure in failures {
            if let Some(remedy) = &failure.remedy {
                text.push_str(&format!("  {}: {remedy}\n", failure.title));
            }
        }
    }
    text
}

#[derive(Debug, Serialize)]
pub struct InternalNotesOutcome {
    pub path: String,
    pub tracked: bool,
    pub ignored: bool,
}

pub fn internal_readme(context: &CommandContext, path: &Path) -> ApplicationResult<ExitCode> {
    let repository = heikas_infrastructure::paths::canonical_root(path)?;
    let written = internal_notes::refresh(&repository)?;
    let tracked =
        heikas_policy::repository::path_is_tracked(&repository, internal_notes::FILE_NAME);
    let ignored =
        heikas_policy::repository::path_is_ignored(&repository, internal_notes::FILE_NAME);
    let outcome = InternalNotesOutcome {
        path: written.display().to_string(),
        tracked,
        ignored,
    };
    context.emit(&outcome, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Internal working notes\n"));
        text.push_str(&format!("Path: {}\n", outcome.path));
        text.push_str(&format!(
            "Tracked: {}\n",
            if tracked {
                palette.failure("yes, which violates the documentation policy")
            } else {
                palette.success("no")
            }
        ));
        text.push_str(&format!(
            "Ignored: {}\n",
            if ignored {
                palette.failure("yes, which violates the documentation policy")
            } else {
                palette.success("no")
            }
        ));
        text
    });
    Ok(if tracked || ignored {
        ExitCode::PolicyViolation
    } else {
        ExitCode::Success
    })
}
