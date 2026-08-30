mod arguments;
mod commands;
mod context;
mod exit;
mod internal_notes;
mod presentation;
mod schemas;

use clap::{CommandFactory, Parser};
use heikas_infrastructure::telemetry::{install_tracing, TerminalFormat};

use crate::arguments::{Arguments, Command};
use crate::commands::run_control::RunOptions;
use crate::commands::ui::InterfaceOptions;
use crate::context::CommandContext;
use crate::exit::ExitCode;

fn main() {
    let arguments = Arguments::parse();
    install_tracing(
        if arguments.json {
            TerminalFormat::Json
        } else if arguments.quiet {
            TerminalFormat::Silent
        } else {
            TerminalFormat::Compact
        },
        "heikas_application=info,heikas_infrastructure=info,heikas_api=info,warn",
    );

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("the asynchronous runtime could not start: {error}");
            std::process::exit(ExitCode::Failed.value());
        }
    };

    let code = runtime.block_on(execute(arguments));
    std::process::exit(code.value());
}

async fn execute(arguments: Arguments) -> ExitCode {
    if let Command::Completions { shell } = arguments.command {
        let mut command = Arguments::command();
        let name = command.get_name().to_string();
        clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
        return ExitCode::Success;
    }

    let context = match CommandContext::build(
        arguments.home.clone(),
        arguments.json,
        arguments.quiet,
        arguments.plain,
    ) {
        Ok(context) => context,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::for_error(&error);
        }
    };

    let outcome = match arguments.command {
        Command::Init { path, force } => commands::setup::init(&context, &path, force).await,
        Command::Doctor { path } => commands::setup::doctor(&context, &path).await,
        Command::Run {
            repo,
            task,
            task_file,
            candidates,
            parallel,
            repairs,
            commit_policy,
            profile,
            minimum_coverage,
            include_dirty,
            agent,
            model,
            wall_clock_seconds,
            demonstration,
            no_dispatch,
        } => {
            commands::run_control::create_and_dispatch(
                &context,
                RunOptions {
                    repository: &repo,
                    task,
                    task_file,
                    candidates,
                    parallel,
                    repairs,
                    commit_policy,
                    profile,
                    minimum_coverage,
                    include_dirty,
                    agent,
                    model,
                    wall_clock_seconds,
                    demonstration,
                    dispatch: !no_dispatch,
                },
            )
            .await
        }
        Command::Resume { run } => commands::run_control::resume(&context, &run).await,
        Command::ApprovePlan {
            run,
            plan_file,
            note,
        } => commands::approvals::approve_plan(&context, &run, plan_file, note).await,
        Command::RevisePlan { run, note } => {
            commands::approvals::revise_plan(&context, &run, note).await
        }
        Command::RejectPlan { run, reason } => {
            commands::approvals::reject_plan(&context, &run, reason).await
        }
        Command::ApproveCommit { run, note } => {
            commands::approvals::approve_commit(&context, &run, note).await
        }
        Command::Cancel { run, reason } => {
            commands::run_control::cancel(&context, &run, reason).await
        }
        Command::List { status, limit } => {
            commands::inspection::list(&context, status, limit).await
        }
        Command::Show { run } => commands::inspection::show(&context, &run).await,
        Command::Logs { run, follow, limit } => {
            commands::inspection::logs(&context, &run, follow, limit).await
        }
        Command::Timeline { run, format } => {
            commands::inspection::timeline(&context, &run, format).await
        }
        Command::Export {
            run,
            output,
            include_worktrees,
        } => commands::maintenance::export(&context, &run, output, include_worktrees).await,
        Command::Ui {
            run,
            port,
            no_open,
            demonstration,
            unsafe_bind_all_interfaces,
            public_origin,
        } => {
            commands::ui::serve(
                &context,
                InterfaceOptions {
                    run,
                    port,
                    open_browser: !no_open,
                    demonstration,
                    bind_all_interfaces: unsafe_bind_all_interfaces,
                    public_origin,
                },
            )
            .await
        }
        Command::Cleanup { run, force } => {
            commands::maintenance::cleanup(&context, &run, force).await
        }
        Command::InternalReadme { path } => commands::setup::internal_readme(&context, &path),
        Command::Trust { path, revoke, list } => {
            commands::setup::trust(&context, &path, revoke, list).await
        }
        Command::Policy { path } => commands::maintenance::policy(&context, &path),
        Command::Schemas { output } => commands::maintenance::schemas(&context, &output),
        Command::Completions { .. } => Ok(ExitCode::Success),
    };

    match outcome {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            if let Some(remedy) = error.remedy() {
                eprintln!("{remedy}");
            }
            ExitCode::for_error(&error)
        }
    }
}
