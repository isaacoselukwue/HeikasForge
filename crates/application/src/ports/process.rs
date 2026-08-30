use std::path::PathBuf;

use async_trait::async_trait;
use heikas_domain::clock::DurationMs;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::error::ApplicationResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequest {
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub environment: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout_seconds: u32,
    pub max_output_bytes: u64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProcessOutcome {
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub duration: DurationMs,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub process_id: Option<u32>,
    pub children_terminated: u32,
}

impl ProcessOutcome {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.cancelled
    }
}

pub type CancellationSignal = watch::Receiver<bool>;

#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn run(
        &self,
        request: ProcessRequest,
        cancellation: CancellationSignal,
    ) -> ApplicationResult<ProcessOutcome>;

    async fn probe_executable(&self, program: &str) -> ApplicationResult<Option<String>>;
}
