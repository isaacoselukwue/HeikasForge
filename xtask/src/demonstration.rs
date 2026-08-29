use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{TaskError, TaskResult};
use crate::workspace::{copy_tree, git_email, heikas_executable, run, run_checked, workspace_root};

pub const DEMONSTRATION_AUTHOR: &str = "Isaac Oselukwue";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemonstrationOutcome {
    pub run_id: String,
    pub status: String,
    pub repository: String,
    pub heikas_home: String,
    pub export_archive: Option<String>,
    pub commit_hash: Option<String>,
    pub branch: Option<String>,
    pub winner: Option<String>,
    pub candidates: Vec<CandidateOutcome>,
    pub failed_test_commands: u32,
    pub repair_loops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateOutcome {
    pub candidate_id: String,
    pub status: String,
    pub rank: Option<u32>,
    pub repairs_used: u32,
    pub changed_lines: u64,
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DemonstrationOptions {
    pub work_directory: PathBuf,
    pub reset: bool,
    pub keep_home: bool,
}

impl Default for DemonstrationOptions {
    fn default() -> Self {
        Self {
            work_directory: crate::workspace::target_directory().join("demonstration"),
            reset: true,
            keep_home: false,
        }
    }
}

pub fn seed_repository(options: &DemonstrationOptions) -> TaskResult<PathBuf> {
    let root = workspace_root();
    let repository = options.work_directory.join("repository");
    if options.reset && repository.exists() {
        std::fs::remove_dir_all(&repository)?;
    }
    if repository.exists() {
        return Ok(repository);
    }
    copy_tree(
        &root.join("fixtures").join("repairable-python"),
        &repository,
    )?;

    let script = root
        .join("fixtures")
        .join("fake-agent")
        .join("demonstration.json");
    if !script.is_file() {
        return Err(TaskError::Missing(format!(
            "the demonstration fixture script is missing at {}",
            script.display()
        )));
    }
    let configuration_path = repository.join(".heikas").join("forge.toml");
    let configuration = std::fs::read_to_string(&configuration_path)?;
    let updated = configuration.replace(
        "model = \"heikas-deterministic-fixture-1.0\"",
        &format!(
            "model = \"heikas-deterministic-fixture-1.0\"\nfixture_script = \"{}\"",
            script.display().to_string().replace('\\', "/")
        ),
    );
    std::fs::write(&configuration_path, updated)?;

    let email = git_email(&root)?;
    run_checked("git", &["init", "--quiet"], &repository, &[])?;
    run_checked(
        "git",
        &["symbolic-ref", "HEAD", "refs/heads/main"],
        &repository,
        &[],
    )?;
    run_checked(
        "git",
        &["config", "user.name", DEMONSTRATION_AUTHOR],
        &repository,
        &[],
    )?;
    run_checked("git", &["config", "user.email", &email], &repository, &[])?;
    run_checked(
        "git",
        &["config", "commit.gpgsign", "false"],
        &repository,
        &[],
    )?;
    run_checked("git", &["add", "-A"], &repository, &[])?;
    run_checked(
        "git",
        &[
            "commit",
            "--quiet",
            "--message",
            "Add the invoice module and its rounding test suite",
        ],
        &repository,
        &[
            ("GIT_AUTHOR_NAME", DEMONSTRATION_AUTHOR.to_string()),
            ("GIT_AUTHOR_EMAIL", email.clone()),
            ("GIT_COMMITTER_NAME", DEMONSTRATION_AUTHOR.to_string()),
            ("GIT_COMMITTER_EMAIL", email),
        ],
    )?;
    Ok(repository)
}

pub fn execute(options: &DemonstrationOptions) -> TaskResult<DemonstrationOutcome> {
    let root = workspace_root();
    std::fs::create_dir_all(&options.work_directory)?;
    let repository = seed_repository(options)?;
    let heikas_home = options.work_directory.join("home");
    if !options.keep_home && heikas_home.exists() {
        std::fs::remove_dir_all(&heikas_home)?;
    }
    std::fs::create_dir_all(&heikas_home)?;

    let executable = heikas_executable()?;
    let program = executable.display().to_string();
    let environment = vec![("HEIKAS_HOME", heikas_home.display().to_string())];
    let repository_argument = repository.display().to_string();
    let task_file = repository.join("TASK.md").display().to_string();

    println!("Creating the demonstration run");
    let created = invoke(
        &program,
        &[
            "--json",
            "run",
            "--repo",
            &repository_argument,
            "--task-file",
            &task_file,
            "--demonstration",
            "--agent",
            "fake",
        ],
        &root,
        &environment,
    )?;
    let run_id = created
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| TaskError::Invalid("the run identifier was not reported".to_string()))?
        .to_string();
    println!("Run {run_id} paused for plan approval");

    invoke(
        &program,
        &[
            "--json",
            "approve-plan",
            &run_id,
            "--note",
            "Approved for the deterministic demonstration",
        ],
        &root,
        &environment,
    )?;
    println!("Plan approved, candidates completed");

    let after_candidates = invoke(&program, &["--json", "show", &run_id], &root, &environment)?;
    let status_after_candidates = after_candidates
        .get("summary")
        .and_then(|summary| summary.get("status"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if status_after_candidates == "awaiting_commit_approval" {
        invoke(
            &program,
            &[
                "--json",
                "approve-commit",
                &run_id,
                "--note",
                "Approved for the deterministic demonstration",
            ],
            &root,
            &environment,
        )?;
        println!("Commit approved");
    }

    let export_directory = options.work_directory.join("export");
    std::fs::create_dir_all(&export_directory)?;
    let export = invoke(
        &program,
        &[
            "--json",
            "export",
            &run_id,
            "--output",
            &export_directory.display().to_string(),
        ],
        &root,
        &environment,
    )
    .ok();

    let detail = invoke(&program, &["--json", "show", &run_id], &root, &environment)?;
    Ok(summarise(
        run_id,
        repository,
        heikas_home,
        export.and_then(|value| {
            value
                .get("archive_path")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        &detail,
    ))
}

fn invoke(
    program: &str,
    arguments: &[&str],
    working_directory: &Path,
    environment: &[(&str, String)],
) -> TaskResult<Value> {
    let output = run(program, arguments, working_directory, environment)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let code = output.status.code().unwrap_or(-1);
    let acceptable = [0, 3];
    if !acceptable.contains(&code) {
        return Err(TaskError::Process(format!(
            "`heikas {}` exited with status {code}\n{stdout}\n{stderr}",
            arguments.join(" ")
        )));
    }
    serde_json::from_str::<Value>(stdout.trim()).map_err(|error| {
        TaskError::Invalid(format!(
            "`heikas {}` did not produce a JSON object: {error}\n{stdout}\n{stderr}",
            arguments.join(" ")
        ))
    })
}

fn summarise(
    run_id: String,
    repository: PathBuf,
    heikas_home: PathBuf,
    export_archive: Option<String>,
    detail: &Value,
) -> DemonstrationOutcome {
    let summary = detail.get("summary");
    let projection = detail.get("projection");
    let candidates = detail
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|candidate| CandidateOutcome {
            candidate_id: candidate
                .get("candidate_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: candidate
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            rank: candidate
                .get("rank")
                .and_then(Value::as_u64)
                .map(|value| value as u32),
            repairs_used: candidate
                .get("repairs_used")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            changed_lines: candidate
                .get("changed_lines")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            exclusions: candidate
                .get("exclusion_summaries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect(),
        })
        .collect();

    let timeline = detail.get("timeline").and_then(Value::as_array);
    let failed_test_commands = timeline
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| {
                    entry.get("event_type").and_then(Value::as_str)
                        == Some("test_evidence_recorded")
                        && entry.get("level").and_then(Value::as_str) == Some("failure")
                })
                .count() as u32
        })
        .unwrap_or(0);

    DemonstrationOutcome {
        run_id,
        status: summary
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        repository: repository.display().to_string(),
        heikas_home: heikas_home.display().to_string(),
        export_archive,
        commit_hash: projection
            .and_then(|value| value.get("commit"))
            .and_then(|value| value.get("commit_hash"))
            .and_then(Value::as_str)
            .map(str::to_string),
        branch: projection
            .and_then(|value| value.get("commit"))
            .and_then(|value| value.get("branch"))
            .and_then(Value::as_str)
            .map(str::to_string),
        winner: summary
            .and_then(|value| value.get("winner"))
            .and_then(Value::as_str)
            .map(str::to_string),
        candidates,
        failed_test_commands,
        repair_loops: projection
            .and_then(|value| value.get("metrics"))
            .and_then(|value| value.get("repair_loops"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }
}

pub fn print_outcome(outcome: &DemonstrationOutcome) {
    println!();
    println!("Demonstration run {}", outcome.run_id);
    println!("  status            {}", outcome.status);
    println!("  repository        {}", outcome.repository);
    println!("  application data  {}", outcome.heikas_home);
    println!(
        "  winner            {}",
        outcome.winner.clone().unwrap_or_else(|| "none".to_string())
    );
    println!(
        "  commit            {}",
        outcome
            .commit_hash
            .clone()
            .map(|hash| hash[..hash.len().min(12)].to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "  branch            {}",
        outcome.branch.clone().unwrap_or_else(|| "none".to_string())
    );
    println!("  failed test gates {}", outcome.failed_test_commands);
    println!("  repair loops      {}", outcome.repair_loops);
    for candidate in &outcome.candidates {
        println!(
            "  candidate {} {:<11} rank {:<6} repairs {} lines {}",
            candidate.candidate_id,
            candidate.status,
            candidate
                .rank
                .map(|rank| rank.to_string())
                .unwrap_or_else(|| "-".to_string()),
            candidate.repairs_used,
            candidate.changed_lines
        );
        for exclusion in &candidate.exclusions {
            println!("      {exclusion}");
        }
    }
    if let Some(archive) = &outcome.export_archive {
        println!("  export            {archive}");
    }
}
