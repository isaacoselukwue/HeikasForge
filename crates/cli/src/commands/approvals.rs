use std::path::PathBuf;

use heikas_application::error::{ApplicationError, ApplicationResult};

use crate::commands::run_control::{dispatch_with_interrupt, report_dispatch};
use crate::context::CommandContext;
use crate::exit::ExitCode;

pub async fn approve_plan(
    context: &CommandContext,
    reference: &str,
    plan_file: Option<PathBuf>,
    note: Option<String>,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let markdown = match plan_file {
        Some(path) => Some(std::fs::read_to_string(&path).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "the plan file `{}` could not be read: {error}",
                path.display()
            ))
        })?),
        None => None,
    };
    context
        .service()
        .approve_plan(run_id, markdown, note)
        .await?;
    let projection = context.service().projection(run_id).await?;
    let version = projection
        .plan
        .current()
        .map(|entry| entry.version)
        .unwrap_or(0);
    context.note(&format!(
        "Plan version {version} approved for run {run_id}."
    ));
    match dispatch_with_interrupt(context, run_id).await? {
        Some(dispatch) => report_dispatch(context, run_id, dispatch).await,
        None => Ok(ExitCode::Interrupted),
    }
}

pub async fn revise_plan(
    context: &CommandContext,
    reference: &str,
    note: String,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    context.service().revise_plan(run_id, note).await?;
    match dispatch_with_interrupt(context, run_id).await? {
        Some(dispatch) => report_dispatch(context, run_id, dispatch).await,
        None => Ok(ExitCode::Interrupted),
    }
}

pub async fn reject_plan(
    context: &CommandContext,
    reference: &str,
    reason: Option<String>,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    context.service().reject_plan(run_id, reason).await?;
    match dispatch_with_interrupt(context, run_id).await? {
        Some(dispatch) => report_dispatch(context, run_id, dispatch).await,
        None => Ok(ExitCode::Interrupted),
    }
}

pub async fn approve_commit(
    context: &CommandContext,
    reference: &str,
    note: Option<String>,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    context.service().approve_commit(run_id, note).await?;
    match dispatch_with_interrupt(context, run_id).await? {
        Some(dispatch) => report_dispatch(context, run_id, dispatch).await,
        None => Ok(ExitCode::Interrupted),
    }
}
