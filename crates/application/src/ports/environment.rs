use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiskSpace {
    pub available_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HostFacts {
    pub operating_system: String,
    pub architecture: String,
    pub logical_processors: usize,
    pub heikas_home: PathBuf,
    pub data_root_writable: bool,
}

#[async_trait]
pub trait HostEnvironment: Send + Sync {
    async fn facts(&self) -> ApplicationResult<HostFacts>;
    async fn disk_space(&self, path: &Path) -> ApplicationResult<DiskSpace>;
    fn environment_variable(&self, name: &str) -> Option<String>;
    fn home_directory(&self) -> Option<PathBuf>;
}
