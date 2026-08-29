use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::error::{TaskError, TaskResult};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn target_directory() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
}

pub fn cargo_executable() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

pub fn heikas_executable() -> TaskResult<PathBuf> {
    let release = target_directory()
        .join("release")
        .join(executable_name("heikas"));
    if release.is_file() {
        return Ok(release);
    }
    let debug = target_directory()
        .join("debug")
        .join(executable_name("heikas"));
    if debug.is_file() {
        return Ok(debug);
    }
    build_heikas()?;
    let debug = target_directory()
        .join("debug")
        .join(executable_name("heikas"));
    if debug.is_file() {
        Ok(debug)
    } else {
        Err(TaskError::Missing(format!(
            "the heikas executable was not produced at {}",
            debug.display()
        )))
    }
}

pub fn build_heikas() -> TaskResult<()> {
    run_checked(
        &cargo_executable(),
        &["build", "-p", "heikas-cli"],
        &workspace_root(),
        &[],
    )
    .map(|_| ())
}

pub fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

pub fn run(
    program: &str,
    arguments: &[&str],
    working_directory: &Path,
    environment: &[(&str, String)],
) -> TaskResult<Output> {
    let mut command = Command::new(program);
    command.args(arguments);
    command.current_dir(working_directory);
    for (name, value) in environment {
        command.env(name, value);
    }
    command
        .output()
        .map_err(|error| TaskError::Process(format!("could not start `{program}`: {error}")))
}

pub fn run_checked(
    program: &str,
    arguments: &[&str],
    working_directory: &Path,
    environment: &[(&str, String)],
) -> TaskResult<String> {
    let output = run(program, arguments, working_directory, environment)?;
    if !output.status.success() {
        return Err(TaskError::Process(format!(
            "`{program} {}` failed with status {}\n{}\n{}",
            arguments.join(" "),
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn stream(
    program: &str,
    arguments: &[&str],
    working_directory: &Path,
    environment: &[(&str, String)],
) -> TaskResult<bool> {
    let mut command = Command::new(program);
    command.args(arguments);
    command.current_dir(working_directory);
    for (name, value) in environment {
        command.env(name, value);
    }
    let status = command
        .status()
        .map_err(|error| TaskError::Process(format!("could not start `{program}`: {error}")))?;
    Ok(status.success())
}

pub fn copy_tree(source: &Path, destination: &Path) -> TaskResult<()> {
    std::fs::create_dir_all(destination).map_err(|error| {
        TaskError::Filesystem(format!(
            "could not create `{}`: {error}",
            destination.display()
        ))
    })?;
    for entry in walkdir::WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
    {
        let relative = match entry.path().strip_prefix(source) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| {
                TaskError::Filesystem(format!("could not create `{}`: {error}", target.display()))
            })?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    TaskError::Filesystem(format!(
                        "could not create `{}`: {error}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::copy(entry.path(), &target).map_err(|error| {
                TaskError::Filesystem(format!(
                    "could not copy `{}`: {error}",
                    entry.path().display()
                ))
            })?;
        }
    }
    Ok(())
}

pub const FIXTURE_EMAIL: &str = "heikas-fixture@localhost.invalid";

pub fn git_email(root: &Path) -> TaskResult<String> {
    let output = run("git", &["config", "user.email"], root, &[])?;
    let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if email.is_empty() {
        eprintln!(
            "no Git email is configured on this host, so the disposable fixture repository uses {FIXTURE_EMAIL}"
        );
        return Ok(FIXTURE_EMAIL.to_string());
    }
    Ok(email)
}

pub fn node_modules_binary(relative: &str) -> Option<PathBuf> {
    let candidate = workspace_root().join("node_modules").join(relative);
    candidate.is_file().then_some(candidate)
}
