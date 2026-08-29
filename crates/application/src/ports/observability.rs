use async_trait::async_trait;
use heikas_domain::event::DurableEvent;
use heikas_domain::identity::RunId;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;

pub trait Redactor: Send + Sync {
    fn redact_text(&self, value: &str) -> String;
    fn redact_bytes(&self, value: &[u8]) -> Vec<u8>;
    fn redact_json(&self, value: &serde_json::Value) -> serde_json::Value;
}

#[async_trait]
pub trait DomainEventPublisher: Send + Sync {
    async fn publish(&self, event: &DurableEvent) -> ApplicationResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StructuredLogRecord {
    pub recorded_at: heikas_domain::clock::Timestamp,
    pub level: String,
    pub target: String,
    pub message: String,
    pub run_id: Option<RunId>,
    pub candidate_id: Option<String>,
    pub node_id: Option<String>,
    pub attempt: Option<u32>,
    pub fields: serde_json::Value,
}

#[async_trait]
pub trait RunLogReader: Send + Sync {
    async fn read(
        &self,
        run_id: RunId,
        offset: u64,
        limit: usize,
    ) -> ApplicationResult<Vec<StructuredLogRecord>>;
    async fn count(&self, run_id: RunId) -> ApplicationResult<u64>;
}

#[async_trait]
pub trait RunLogWriter: Send + Sync {
    async fn append(&self, run_id: RunId, record: StructuredLogRecord) -> ApplicationResult<()>;
}
