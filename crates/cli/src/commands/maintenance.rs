use std::path::{Path, PathBuf};

use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::model::request::ExportRequest;
use serde::Serialize;

use crate::context::CommandContext;
use crate::exit::ExitCode;
use crate::presentation::Table;

#[derive(Debug, Serialize)]
pub struct ExportOutcomeReport {
    pub run_id: String,
    pub archive_path: String,
    pub byte_length: u64,
    pub entry_count: u64,
    pub redacted: bool,
}

pub async fn export(
    context: &CommandContext,
    reference: &str,
    output: PathBuf,
    include_worktrees: bool,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let outcome = context
        .service()
        .export(
            run_id,
            ExportRequest {
                output_path: output,
                include_worktrees,
            },
        )
        .await?;
    let report = ExportOutcomeReport {
        run_id: run_id.to_string(),
        archive_path: outcome.archive_path.display().to_string(),
        byte_length: outcome.byte_length,
        entry_count: outcome.entry_count,
        redacted: outcome.redacted,
    };
    context.emit(&report, |palette| {
        format!(
            "{}\nArchive: {}\nEntries: {}\nBytes: {}\nRedacted: {}\n",
            palette.heading("Evidence exported"),
            report.archive_path,
            report.entry_count,
            report.byte_length,
            report.redacted
        )
    });
    Ok(ExitCode::Success)
}

#[derive(Debug, Serialize)]
pub struct CleanupOutcome {
    pub run_id: String,
    pub removed: Vec<String>,
}

pub async fn cleanup(
    context: &CommandContext,
    reference: &str,
    force: bool,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let projection = context.service().projection(run_id).await?;
    if !force {
        context.note(&format!(
            "This removes the worktrees for run {run_id} while preserving evidence. Pass --force to proceed."
        ));
        return Ok(ExitCode::InvalidUsage);
    }
    if !projection.status.is_terminal() {
        return Err(ApplicationError::InvalidRunState {
            run: run_id,
            status: projection.status.to_string(),
            operation: "cleanup",
        });
    }
    let removed = context.service().cleanup(run_id).await?;
    let outcome = CleanupOutcome {
        run_id: run_id.to_string(),
        removed: removed.clone(),
    };
    context.emit(&outcome, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Worktrees removed\n"));
        for path in &removed {
            text.push_str(&format!("  {path}\n"));
        }
        text.push_str("Run evidence is preserved.\n");
        text
    });
    Ok(ExitCode::Success)
}

pub fn policy(context: &CommandContext, path: &Path) -> ApplicationResult<ExitCode> {
    let report = heikas_policy::check_repository(path).map_err(|error| {
        ApplicationError::PolicyViolation(format!("the policy check could not run: {error}"))
    })?;
    let passed = report.passed();
    context.emit(&report, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Repository policy\n"));
        text.push_str(&format!("Files checked: {}\n", report.files_checked));
        text.push_str(&format!("Rules run: {}\n", report.rules_run.len()));
        if passed {
            text.push_str(&palette.success("Every policy rule passed.\n"));
            return text;
        }
        let mut table = Table::new(&["Rule", "Location", "Finding"]);
        for finding in report.violations() {
            let location = match (&finding.path, finding.line) {
                (Some(path), Some(line)) => format!("{path}:{line}"),
                (Some(path), None) => path.clone(),
                _ => "-".to_string(),
            };
            table.push(vec![
                finding.rule.clone(),
                location,
                finding.message.clone(),
            ]);
        }
        text.push_str(&table.render(palette));
        text.push_str(&palette.failure(&format!(
            "{} policy violations were found.\n",
            report.violations().count()
        )));
        text
    });
    Ok(if passed {
        ExitCode::Success
    } else {
        ExitCode::PolicyViolation
    })
}

pub fn schemas(context: &CommandContext, output: &Path) -> ApplicationResult<ExitCode> {
    let written = crate::schemas::write_all(output)?;
    context.emit(&written, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Schemas written\n"));
        for path in &written {
            text.push_str(&format!("  {path}\n"));
        }
        text
    });
    Ok(ExitCode::Success)
}
