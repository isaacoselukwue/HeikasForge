use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "heikas",
    version,
    about = "Local-first agentic software engineering orchestrator",
    long_about = "Heikas Forge plans a coding task, pauses for human approval, runs isolated implementation candidates, applies deterministic gates and commits the strongest valid result."
)]
pub struct Arguments {
    #[arg(long, global = true, help = "Emit a single JSON object instead of human-readable output")]
    pub json: bool,

    #[arg(long, global = true, help = "Suppress progress output but keep errors")]
    pub quiet: bool,

    #[arg(long, global = true, help = "Disable colour even when the output is a terminal")]
    pub plain: bool,

    #[arg(long, global = true, env = "HEIKAS_HOME", help = "Override the application data root")]
    pub home: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Detect the project and create the repository configuration")]
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, help = "Write the configuration without asking for confirmation")]
        force: bool,
    },

    #[command(about = "Inspect Git, agents, models, commands, scanners, permissions and disk space")]
    Doctor {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    #[command(about = "Create and dispatch a run")]
    Run {
        #[arg(long, help = "Path to the target Git repository")]
        repo: PathBuf,
        #[arg(long, conflicts_with = "task_file", help = "Inline task description")]
        task: Option<String>,
        #[arg(long, conflicts_with = "task", help = "Path to a file containing the task")]
        task_file: Option<PathBuf>,
        #[arg(long, help = "Number of implementation candidates, from 1 to 8")]
        candidates: Option<u8>,
        #[arg(long, help = "Maximum candidates running at the same time")]
        parallel: Option<u8>,
        #[arg(long, help = "Repair attempts allowed for each candidate")]
        repairs: Option<u32>,
        #[arg(long, value_enum, help = "Commit policy applied after the final gates")]
        commit_policy: Option<CommitPolicyArgument>,
        #[arg(long, value_enum, help = "Quality profile applied to every candidate")]
        profile: Option<QualityProfileArgument>,
        #[arg(long, help = "Minimum line coverage percentage required for eligibility")]
        minimum_coverage: Option<f64>,
        #[arg(long, help = "Capture the current uncommitted changes as the candidate baseline")]
        include_dirty: bool,
        #[arg(long, help = "Agent driver identifier")]
        agent: Option<String>,
        #[arg(long, help = "Model identifier passed to the agent driver")]
        model: Option<String>,
        #[arg(long, help = "Wall clock budget for the whole run, in seconds")]
        wall_clock_seconds: Option<u32>,
        #[arg(long, help = "Run with the deterministic demonstration agent")]
        demonstration: bool,
        #[arg(long, help = "Create the run without dispatching it")]
        no_dispatch: bool,
    },

    #[command(about = "Resume a paused or interrupted run")]
    Resume {
        run: String,
    },

    #[command(about = "Approve the current plan version")]
    ApprovePlan {
        run: String,
        #[arg(long, help = "Replace the plan with the contents of this file before approving")]
        plan_file: Option<PathBuf>,
        #[arg(long, help = "Optional approval note")]
        note: Option<String>,
    },

    #[command(about = "Request a new plan version")]
    RevisePlan {
        run: String,
        #[arg(long)]
        note: String,
    },

    #[command(about = "Reject the plan and end the run before any code changes")]
    RejectPlan {
        run: String,
        #[arg(long)]
        reason: Option<String>,
    },

    #[command(about = "Permit the final commit")]
    ApproveCommit {
        run: String,
        #[arg(long)]
        note: Option<String>,
    },

    #[command(about = "Cancel active work and terminate child processes")]
    Cancel {
        run: String,
        #[arg(long)]
        reason: Option<String>,
    },

    #[command(about = "List runs with status, repository, current node, age and winner")]
    List {
        #[arg(long, help = "Show only runs with this status")]
        status: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },

    #[command(about = "Show a run summary and candidate table")]
    Show {
        run: String,
    },

    #[command(about = "Stream structured run logs")]
    Logs {
        run: String,
        #[arg(long, help = "Keep streaming until interrupted")]
        follow: bool,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },

    #[command(about = "Render executed transitions")]
    Timeline {
        run: String,
        #[arg(long, value_enum, default_value_t = TimelineFormat::Text)]
        format: TimelineFormat,
    },

    #[command(about = "Create a redacted evidence archive")]
    Export {
        run: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, help = "Include the candidate worktrees in the archive")]
        include_worktrees: bool,
    },

    #[command(about = "Start the local graphical interface")]
    Ui {
        #[arg(long)]
        run: Option<String>,
        #[arg(long, default_value_t = 0, help = "Preferred loopback port, zero selects any free port")]
        port: u16,
        #[arg(long, help = "Do not open a browser window automatically")]
        no_open: bool,
        #[arg(long, help = "Start in demonstration mode with the deterministic agent")]
        demonstration: bool,
        #[arg(long, hide = true, help = "Bind every interface, for development only")]
        unsafe_bind_all_interfaces: bool,
    },

    #[command(about = "Remove worktrees while preserving evidence")]
    Cleanup {
        run: String,
        #[arg(long, help = "Remove without asking for confirmation")]
        force: bool,
    },

    #[command(name = "internal-readme", about = "Create or refresh the local untracked internal notes")]
    InternalReadme {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    #[command(about = "Run the repository conformance checks")]
    Policy {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    #[command(about = "Write the published JSON schemas")]
    Schemas {
        #[arg(long, default_value = "schemas")]
        output: PathBuf,
    },

    #[command(about = "Generate a shell completion script")]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CommitPolicyArgument {
    Manual,
    Automatic,
    None,
}

impl CommitPolicyArgument {
    pub fn to_domain(self) -> heikas_domain::run::CommitPolicy {
        match self {
            CommitPolicyArgument::Manual => heikas_domain::run::CommitPolicy::Manual,
            CommitPolicyArgument::Automatic => heikas_domain::run::CommitPolicy::Automatic,
            CommitPolicyArgument::None => heikas_domain::run::CommitPolicy::None,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum QualityProfileArgument {
    Standard,
    Strict,
}

impl QualityProfileArgument {
    pub fn to_domain(self) -> heikas_domain::budget::QualityProfile {
        match self {
            QualityProfileArgument::Standard => heikas_domain::budget::QualityProfile::Standard,
            QualityProfileArgument::Strict => heikas_domain::budget::QualityProfile::Strict,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum TimelineFormat {
    Text,
    Json,
    Html,
}
