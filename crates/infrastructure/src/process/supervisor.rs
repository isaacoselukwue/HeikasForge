use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::process::{
    CancellationSignal, ProcessOutcome, ProcessRequest, ProcessRunner,
};
use heikas_domain::clock::DurationMs;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tracing::{debug, warn};

use crate::process::tree;

const GRACEFUL_TERMINATION_DELAY: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

pub struct SupervisedProcessRunner {
    environment_allowlist: Vec<String>,
}

impl SupervisedProcessRunner {
    pub fn new(environment_allowlist: Vec<String>) -> Self {
        Self {
            environment_allowlist,
        }
    }

    fn base_environment(&self) -> Vec<(String, String)> {
        let mut variables = Vec::new();
        for name in essential_environment_variables().iter().chain(
            self.environment_allowlist
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>()
                .iter(),
        ) {
            if let Ok(value) = std::env::var(name) {
                variables.push(((*name).to_string(), value));
            }
        }
        variables
    }

    fn build_command(&self, request: &ProcessRequest) -> Command {
        let mut command = Command::new(&request.program);
        command.args(&request.args);
        command.current_dir(&request.working_directory);
        command.env_clear();
        for (name, value) in self.base_environment() {
            command.env(name, value);
        }
        for (name, value) in &request.environment {
            command.env(name, value);
        }
        if request.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);
        tree::configure_isolated_group(&mut command);
        command
    }
}

#[async_trait]
impl ProcessRunner for SupervisedProcessRunner {
    async fn run(
        &self,
        request: ProcessRequest,
        mut cancellation: CancellationSignal,
    ) -> ApplicationResult<ProcessOutcome> {
        if !request.working_directory.exists() {
            return Err(ApplicationError::Process(format!(
                "the working directory `{}` does not exist",
                request.working_directory.display()
            )));
        }
        let started = Instant::now();
        let mut child = self.build_command(&request).spawn().map_err(|error| {
            ApplicationError::Process(format!("could not start `{}`: {error}", request.program))
        })?;
        let process_id = child.id();
        let job = tree::register(&child);

        if let Some(payload) = request.stdin.clone() {
            if let Some(mut handle) = child.stdin.take() {
                tokio::spawn(async move {
                    let _ = handle.write_all(&payload).await;
                    let _ = handle.shutdown().await;
                });
            }
        }

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();
        let limit = request.max_output_bytes;
        let stdout_task = tokio::spawn(read_bounded(stdout_handle, limit));
        let stderr_task = tokio::spawn(read_bounded(stderr_handle, limit));

        let timeout = Duration::from_secs(u64::from(request.timeout_seconds));
        let mut timed_out = false;
        let mut cancelled = false;

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|error| ApplicationError::Process(error.to_string()))?
            }
            _ = tokio::time::sleep(timeout) => {
                timed_out = true;
                terminate(&mut child, process_id).await?
            }
            changed = cancellation.changed() => {
                if changed.is_ok() && *cancellation.borrow() {
                    cancelled = true;
                    terminate(&mut child, process_id).await?
                } else {
                    child.wait().await.map_err(|error| ApplicationError::Process(error.to_string()))?
                }
            }
        };

        let children_terminated = tree::terminate_group(process_id, job);

        let (stdout, stdout_truncated) = stdout_task
            .await
            .map_err(|error| ApplicationError::Process(error.to_string()))?;
        let (stderr, stderr_truncated) = stderr_task
            .await
            .map_err(|error| ApplicationError::Process(error.to_string()))?;

        debug!(
            program = %request.program,
            exit_code = ?status.code(),
            timed_out,
            cancelled,
            "supervised process finished"
        );

        Ok(ProcessOutcome {
            exit_code: status.code(),
            signal: tree::signal_of(&status),
            timed_out,
            cancelled,
            duration: DurationMs::from_millis(started.elapsed().as_millis() as u64),
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
            process_id,
            children_terminated,
        })
    }

    async fn probe_executable(&self, program: &str) -> ApplicationResult<Option<String>> {
        let Some(resolved) = resolve_on_path(program) else {
            return Ok(None);
        };
        let mut command = Command::new(&resolved);
        command.arg("--version");
        command.current_dir(std::env::temp_dir());
        command.env_clear();
        for (name, value) in self.base_environment() {
            command.env(name, value);
        }
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);
        tree::configure_isolated_group(&mut command);
        let output = match tokio::time::timeout(PROBE_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(_)) | Err(_) => {
                return Ok(Some(resolved.display().to_string()));
            }
        };
        let text = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if text.is_empty() {
            Ok(Some(resolved.display().to_string()))
        } else {
            Ok(Some(text))
        }
    }
}

async fn terminate(
    child: &mut Child,
    process_id: Option<u32>,
) -> ApplicationResult<std::process::ExitStatus> {
    tree::request_graceful_stop(process_id);
    match tokio::time::timeout(GRACEFUL_TERMINATION_DELAY, child.wait()).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(error)) => Err(ApplicationError::Process(error.to_string())),
        Err(_) => {
            warn!("escalating to forced termination after the graceful period elapsed");
            child
                .kill()
                .await
                .map_err(|error| ApplicationError::Process(error.to_string()))?;
            child
                .wait()
                .await
                .map_err(|error| ApplicationError::Process(error.to_string()))
        }
    }
}

async fn read_bounded<R>(reader: Option<R>, limit: u64) -> (Vec<u8>, bool)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let Some(mut reader) = reader else {
        return (Vec::new(), false);
    };
    let mut collected: Vec<u8> = Vec::new();
    let mut tail: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 8_192];
    let tail_budget = (limit / 4).max(4_096) as usize;
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                let chunk = &buffer[..read];
                if (collected.len() as u64) < limit {
                    let remaining = (limit - collected.len() as u64) as usize;
                    let take = remaining.min(read);
                    collected.extend_from_slice(&chunk[..take]);
                    if take < read {
                        truncated = true;
                        tail.extend_from_slice(&chunk[take..]);
                    }
                } else {
                    truncated = true;
                    tail.extend_from_slice(chunk);
                }
                if tail.len() > tail_budget {
                    let excess = tail.len() - tail_budget;
                    tail.drain(..excess);
                }
            }
            Err(_) => break,
        }
    }
    if truncated {
        collected.extend_from_slice(b"\n[output truncated, tail preserved below]\n");
        collected.extend_from_slice(&tail);
    }
    (collected, truncated)
}

pub fn resolve_on_path(program: &str) -> Option<std::path::PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    if program.contains('/') || program.contains('\\') {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for suffix in executable_suffixes() {
            let resolved = directory.join(format!("{program}{suffix}"));
            if resolved.is_file() {
                return Some(resolved);
            }
        }
    }
    None
}

#[cfg(windows)]
fn executable_suffixes() -> Vec<String> {
    std::env::var("PATHEXT")
        .map(|value| {
            value
                .split(';')
                .map(|entry| entry.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|_| vec![".exe".to_string(), ".cmd".to_string(), ".bat".to_string()])
}

#[cfg(not(windows))]
fn executable_suffixes() -> Vec<String> {
    vec![String::new()]
}

pub fn essential_environment_variables() -> Vec<&'static str> {
    #[cfg(windows)]
    {
        vec![
            "PATH",
            "PATHEXT",
            "SYSTEMROOT",
            "WINDIR",
            "TEMP",
            "TMP",
            "COMSPEC",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "NUMBER_OF_PROCESSORS",
            "PROCESSOR_ARCHITECTURE",
        ]
    }
    #[cfg(not(windows))]
    {
        vec![
            "PATH", "HOME", "LANG", "LC_ALL", "TMPDIR", "SHELL", "USER", "LOGNAME", "TERM", "TZ",
        ]
    }
}
