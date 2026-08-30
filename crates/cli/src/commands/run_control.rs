use std::path::{Path, PathBuf};

use heikas_application::engine::DispatchOutcome;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::model::request::{CommandDeclaration, CreateRunRequest};
use heikas_domain::command::CommandKind;
use heikas_domain::identity::RunId;
use serde::Serialize;
use std::str::FromStr;

use crate::arguments::{CommitPolicyArgument, QualityProfileArgument};
use crate::context::CommandContext;
use crate::exit::ExitCode;

#[derive(Debug, Serialize)]
pub struct RunOutcome {
    pub run_id: RunId,
    pub status: String,
    pub dispatched: bool,
    pub detail: String,
}

#[allow(clippy::too_many_arguments)]
pub struct RunOptions<'a> {
    pub repository: &'a Path,
    pub task: Option<String>,
    pub task_file: Option<PathBuf>,
    pub candidates: Option<u8>,
    pub parallel: Option<u8>,
    pub repairs: Option<u32>,
    pub commit_policy: Option<CommitPolicyArgument>,
    pub profile: Option<QualityProfileArgument>,
    pub minimum_coverage: Option<f64>,
    pub include_dirty: bool,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub wall_clock_seconds: Option<u32>,
    pub demonstration: bool,
    pub dispatch: bool,
    pub commands: Vec<String>,
    pub command_arguments: Vec<String>,
    pub command_timeouts: Vec<String>,
}

pub async fn create_and_dispatch(
    context: &CommandContext,
    options: RunOptions<'_>,
) -> ApplicationResult<ExitCode> {
    let task_markdown = match (&options.task, &options.task_file) {
        (Some(text), None) => text.clone(),
        (None, Some(path)) => std::fs::read_to_string(path).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "the task file `{}` could not be read: {error}",
                path.display()
            ))
        })?,
        _ => {
            return Err(ApplicationError::InvalidConfiguration(
                "supply exactly one of --task or --task-file".to_string(),
            ))
        }
    };

    let mut request = CreateRunRequest::new(
        heikas_infrastructure::paths::canonical_root(options.repository)?,
        task_markdown,
    );
    request.candidate_count = options.candidates;
    request.max_parallel_candidates = options.parallel;
    request.max_repairs_per_candidate = options.repairs;
    request.commit_policy = options.commit_policy.map(CommitPolicyArgument::to_domain);
    request.quality_profile = options.profile.map(QualityProfileArgument::to_domain);
    request.minimum_line_coverage = options.minimum_coverage;
    request.include_dirty = options.include_dirty;
    request.agent_driver = options.agent.clone();
    request.agent_model = options.model.clone();
    request.wall_clock_seconds = options.wall_clock_seconds;
    request.demonstration_mode = options.demonstration;
    request.command_declarations = build_command_declarations(
        &options.commands,
        &options.command_arguments,
        &options.command_timeouts,
    )?;

    let run_id = context.service().create_run(request).await?;
    context.note(&format!("Run {run_id} created."));

    if !options.dispatch {
        let outcome = RunOutcome {
            run_id,
            status: "created".to_string(),
            dispatched: false,
            detail: "the run was created without dispatching".to_string(),
        };
        context.emit(&outcome, |palette| {
            format!(
                "{}\nRun {run_id} was created without dispatching.\n",
                palette.heading("Run created")
            )
        });
        return Ok(ExitCode::Success);
    }

    match dispatch_with_interrupt(context, run_id).await? {
        Some(dispatch) => report_dispatch(context, run_id, dispatch).await,
        None => Ok(ExitCode::Interrupted),
    }
}

pub async fn dispatch_with_interrupt(
    context: &CommandContext,
    run_id: RunId,
) -> ApplicationResult<Option<DispatchOutcome>> {
    let service = context.service();
    let dispatch = service.dispatch(run_id);
    tokio::pin!(dispatch);
    tokio::select! {
        outcome = &mut dispatch => outcome.map(Some),
        signal = tokio::signal::ctrl_c() => {
            if signal.is_err() {
                return dispatch.await.map(Some);
            }
            context.note("Interrupt received, cancelling the run and terminating child processes.");
            service
                .cancel(run_id, Some("the operator interrupted the command line".to_string()))
                .await?;
            let _ = dispatch.await;
            Ok(None)
        }
    }
}

pub async fn resume(context: &CommandContext, reference: &str) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let projection = context.service().projection(run_id).await?;
    if projection.status.is_terminal() {
        return report_dispatch(
            context,
            run_id,
            DispatchOutcome::Completed(projection.status),
        )
        .await;
    }
    match dispatch_with_interrupt(context, run_id).await? {
        Some(dispatch) => report_dispatch(context, run_id, dispatch).await,
        None => Ok(ExitCode::Interrupted),
    }
}

pub async fn cancel(
    context: &CommandContext,
    reference: &str,
    reason: Option<String>,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    context.service().cancel(run_id, reason).await?;
    let projection = context.service().projection(run_id).await?;
    let outcome = RunOutcome {
        run_id,
        status: projection.status.as_str().to_string(),
        dispatched: false,
        detail: "cancellation was recorded and propagated".to_string(),
    };
    context.emit(&outcome, |palette| {
        format!(
            "{}\nRun {run_id} is now {}.\n",
            palette.heading("Cancellation recorded"),
            palette.run_status(projection.status)
        )
    });
    Ok(ExitCode::Cancelled)
}

pub async fn report_dispatch(
    context: &CommandContext,
    run_id: RunId,
    dispatch: DispatchOutcome,
) -> ApplicationResult<ExitCode> {
    let projection = context.service().projection(run_id).await?;
    let (status_text, detail) = match &dispatch {
        DispatchOutcome::Completed(status) => (
            status.as_str().to_string(),
            format!("the run finished as {}", status.label()),
        ),
        DispatchOutcome::Paused(status) => (
            status.as_str().to_string(),
            format!("the run paused at {}", status.label()),
        ),
        DispatchOutcome::Blocked(reason) => ("blocked".to_string(), reason.clone()),
    };
    let outcome = RunOutcome {
        run_id,
        status: status_text,
        dispatched: true,
        detail: detail.clone(),
    };
    context.emit(&outcome, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Dispatch result\n"));
        text.push_str(&format!("Run: {run_id}\n"));
        text.push_str(&format!(
            "Status: {}\n",
            palette.run_status(projection.status)
        ));
        text.push_str(&format!("Detail: {detail}\n"));
        if let Some(commit) = &projection.commit {
            text.push_str(&format!(
                "Commit: {} on {}\n",
                commit.commit_hash.short(),
                commit.branch
            ));
        }
        if projection.status == heikas_domain::run::RunStatus::AwaitingPlanApproval {
            text.push_str(&format!(
                "Next: review the plan and run `heikas approve-plan {run_id}`.\n"
            ));
        }
        if projection.status == heikas_domain::run::RunStatus::AwaitingCommitApproval {
            text.push_str(&format!(
                "Next: review the integration diff and run `heikas approve-commit {run_id}`.\n"
            ));
        }
        text
    });
    Ok(match dispatch {
        DispatchOutcome::Blocked(_) => ExitCode::RecoveryRequired,
        _ => ExitCode::for_status(projection.status),
    })
}

fn split_declaration(value: &str, flag: &str) -> ApplicationResult<(CommandKind, String)> {
    let Some((kind, remainder)) = value.split_once('=') else {
        return Err(ApplicationError::InvalidConfiguration(format!(
            "`--{flag} {value}` must be written as KIND=VALUE, for example `--{flag} test=pytest`"
        )));
    };
    let parsed = CommandKind::from_str(kind.trim()).map_err(|_| {
        ApplicationError::InvalidConfiguration(format!(
            "`{kind}` is not a command kind. Use one of format, lint, test, coverage, audit, secret-scan, static-analysis, policy or build."
        ))
    })?;
    if remainder.is_empty() {
        return Err(ApplicationError::InvalidConfiguration(format!(
            "`--{flag} {value}` supplies no value after the equals sign"
        )));
    }
    Ok((parsed, remainder.to_string()))
}

fn build_command_declarations(
    commands: &[String],
    arguments: &[String],
    timeouts: &[String],
) -> ApplicationResult<Vec<CommandDeclaration>> {
    let mut declarations: Vec<CommandDeclaration> = Vec::new();
    for entry in commands {
        let (kind, program) = split_declaration(entry, "command")?;
        if declarations.iter().any(|existing| existing.kind == kind) {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "`--command {}=` was given more than once",
                kind.as_str()
            )));
        }
        declarations.push(CommandDeclaration {
            kind,
            program,
            args: Vec::new(),
            timeout_seconds: None,
        });
    }
    for entry in arguments {
        let (kind, argument) = split_declaration(entry, "command-arg")?;
        let Some(declaration) = declarations.iter_mut().find(|item| item.kind == kind) else {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "`--command-arg {}=...` was given without a matching `--command {}=<program>`",
                kind.as_str(),
                kind.as_str()
            )));
        };
        declaration.args.push(argument);
    }
    for entry in timeouts {
        let (kind, seconds) = split_declaration(entry, "command-timeout")?;
        let parsed = seconds.parse::<u32>().map_err(|_| {
            ApplicationError::InvalidConfiguration(format!(
                "`--command-timeout {}={seconds}` must give a whole number of seconds",
                kind.as_str()
            ))
        })?;
        let Some(declaration) = declarations.iter_mut().find(|item| item.kind == kind) else {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "`--command-timeout {}=...` was given without a matching `--command {}=<program>`",
                kind.as_str(),
                kind.as_str()
            )));
        };
        declaration.timeout_seconds = Some(parsed);
    }
    Ok(declarations)
}
