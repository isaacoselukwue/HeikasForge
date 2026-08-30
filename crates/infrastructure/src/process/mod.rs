pub mod supervisor;
pub mod tree;

use std::path::Path;

use heikas_application::error::ApplicationResult;
use heikas_application::ports::process::ProcessRequest;
use heikas_domain::command::CommandSpecification;

pub use supervisor::{resolve_on_path, SupervisedProcessRunner};

pub fn request_for_command(
    specification: &CommandSpecification,
    worktree: &Path,
    max_output_bytes: u64,
) -> ApplicationResult<ProcessRequest> {
    let working_directory = crate::paths::confined_working_directory(
        worktree,
        specification.working_subdirectory.as_deref(),
    )?;
    Ok(ProcessRequest {
        program: specification.program.clone(),
        args: specification.args.clone(),
        working_directory,
        environment: specification.environment.clone(),
        stdin: None,
        timeout_seconds: specification.timeout.get(),
        max_output_bytes,
        label: specification.id.to_string(),
    })
}
