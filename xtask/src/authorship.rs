use crate::error::{TaskError, TaskResult};
use crate::workspace::workspace_root;

pub const REQUIRED_NAME: &str = "Isaac Oselukwue";

pub fn run() -> TaskResult<()> {
    let root = workspace_root();
    let repository = heikas_policy::TrackedRepository::discover(&root)
        .map_err(|error| TaskError::Invalid(error.to_string()))?;
    let findings = heikas_policy::rules::authorship::check(&repository)
        .map_err(|error| TaskError::Invalid(error.to_string()))?;
    let commits = repository
        .commits()
        .map_err(|error| TaskError::Invalid(error.to_string()))?;

    println!("Inspected {} commits", commits.len());
    if findings.is_empty() {
        println!("Every commit shows only {REQUIRED_NAME} as author and committer.");
        return Ok(());
    }
    for finding in &findings {
        eprintln!("{}: {}", finding.rule, finding.message);
        eprintln!("  {}", finding.remedy);
    }
    Err(TaskError::StepFailed {
        step: "authorship".to_string(),
    })
}
