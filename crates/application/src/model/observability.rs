use heikas_domain::identity::RunId;
use serde::{Deserialize, Serialize};

use crate::ports::observability::StructuredLogRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LogPage {
    pub run_id: RunId,
    pub offset: u64,
    pub total: u64,
    pub records: Vec<StructuredLogRecord>,
}
