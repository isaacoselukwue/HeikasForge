use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::error::ApplicationResult;
use heikas_application::ports::observability::{
    DomainEventPublisher, Redactor, RunLogReader, RunLogWriter, StructuredLogRecord,
};
use heikas_domain::event::DurableEvent;
use heikas_domain::identity::RunId;
use tokio::sync::broadcast;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::atomic::{append_line_synchronised, storage};
use crate::layout::StoreLayout;

pub const EVENT_CHANNEL_CAPACITY: usize = 2_048;

#[derive(Clone)]
pub struct BroadcastEventPublisher {
    sender: broadcast::Sender<DurableEvent>,
}

impl Default for BroadcastEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl BroadcastEventPublisher {
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DurableEvent> {
        self.sender.subscribe()
    }
}

#[async_trait]
impl DomainEventPublisher for BroadcastEventPublisher {
    async fn publish(&self, event: &DurableEvent) -> ApplicationResult<()> {
        let _ = self.sender.send(event.clone());
        Ok(())
    }
}

pub struct FileRunLog {
    layout: StoreLayout,
    redactor: Arc<dyn Redactor>,
}

impl FileRunLog {
    pub fn new(layout: StoreLayout, redactor: Arc<dyn Redactor>) -> Self {
        Self { layout, redactor }
    }

    fn path(&self, run_id: RunId) -> PathBuf {
        self.layout.run_log(run_id)
    }
}

#[async_trait]
impl RunLogWriter for FileRunLog {
    async fn append(&self, run_id: RunId, record: StructuredLogRecord) -> ApplicationResult<()> {
        let mut redacted = record;
        redacted.message = self.redactor.redact_text(&redacted.message);
        redacted.fields = self.redactor.redact_json(&redacted.fields);
        let line = serde_json::to_vec(&redacted)?;
        append_line_synchronised(&self.path(run_id), &line)
    }
}

#[async_trait]
impl RunLogReader for FileRunLog {
    async fn read(
        &self,
        run_id: RunId,
        offset: u64,
        limit: usize,
    ) -> ApplicationResult<Vec<StructuredLogRecord>> {
        let path = self.path(run_id);
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage(&path, "read", error)),
        };
        Ok(contents
            .lines()
            .skip(offset as usize)
            .take(limit)
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect())
    }

    async fn count(&self, run_id: RunId) -> ApplicationResult<u64> {
        let path = self.path(run_id);
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(storage(&path, "read", error)),
        };
        Ok(contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalFormat {
    Compact,
    Json,
    Silent,
}

pub fn install_tracing(format: TerminalFormat, default_directive: &str) {
    let filter =
        EnvFilter::try_from_env("HEIKAS_LOG").unwrap_or_else(|_| EnvFilter::new(default_directive));
    let registry = tracing_subscriber::registry().with(filter);
    match format {
        TerminalFormat::Compact => {
            let layer = tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
                .with_writer(std::io::stderr);
            let _ = registry.with(layer).try_init();
        }
        TerminalFormat::Json => {
            let layer = tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_writer(std::io::stderr);
            let _ = registry.with(layer).try_init();
        }
        TerminalFormat::Silent => {
            let layer = tracing_subscriber::fmt::layer().with_writer(std::io::sink);
            let _ = registry.with(layer).try_init();
        }
    }
}
