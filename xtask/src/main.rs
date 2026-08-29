mod authorship;
mod demonstration;
mod error;
mod media;
mod verification;
mod workspace;

use clap::{Parser, Subcommand};

use crate::demonstration::{print_outcome, DemonstrationOptions};
use crate::error::TaskResult;

#[derive(Debug, Parser)]
#[command(
    name = "xtask",
    about = "Workspace automation for Heikas Forge",
    long_about = "Runs the complete local verification suite, the deterministic demonstration, the documentation media pipeline and the authorship checks."
)]
struct Arguments {
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    #[command(about = "Run the complete local verification suite")]
    Verify {
        #[arg(long, help = "Skip the browser end to end tests")]
        skip_browser: bool,
        #[arg(long, help = "Skip the README media validation")]
        skip_media: bool,
        #[arg(long, help = "Stop at the first failing step")]
        fail_fast: bool,
    },

    #[command(about = "Seed and run the deterministic demonstration fixture")]
    Demo {
        #[arg(long, help = "Reuse an existing demonstration working directory")]
        keep: bool,
        #[arg(long, help = "Write the outcome as JSON")]
        json: bool,
    },

    #[command(about = "Derive the animated and MP4 media from the captured frames")]
    Media {
        #[arg(long, help = "Only validate the existing media")]
        validate_only: bool,
    },

    #[command(about = "Verify that every commit carries the required identity")]
    Authorship,

    #[command(about = "Regenerate the published JSON schemas")]
    Schemas,
}

fn main() {
    let arguments = Arguments::parse();
    let outcome = match arguments.command {
        Task::Verify {
            skip_browser,
            skip_media,
            fail_fast,
        } => verification::run(verification::Options {
            skip_browser,
            skip_media,
            fail_fast,
        }),
        Task::Demo { keep, json } => run_demonstration(keep, json),
        Task::Media { validate_only } => media::run(validate_only),
        Task::Authorship => authorship::run(),
        Task::Schemas => verification::regenerate_schemas(),
    };

    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_demonstration(keep: bool, json: bool) -> TaskResult<()> {
    let options = DemonstrationOptions {
        reset: !keep,
        keep_home: keep,
        ..DemonstrationOptions::default()
    };
    let outcome = demonstration::execute(&options)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        print_outcome(&outcome);
    }
    Ok(())
}
