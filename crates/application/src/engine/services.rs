use std::sync::Arc;

use crate::ports::agent::AgentDriver;
use crate::ports::clock::{Clock, IdentifierFactory, LocalIdentity};
use crate::ports::environment::HostEnvironment;
use crate::ports::git::GitService;
use crate::ports::observability::{DomainEventPublisher, Redactor, RunLogWriter};
use crate::ports::process::ProcessRunner;
use crate::ports::quality::{ReviewProvider, TestGateRunner};
use crate::ports::store::{RunLockService, RunStore};

#[derive(Clone)]
pub struct EngineServices {
    pub store: Arc<dyn RunStore>,
    pub locks: Arc<dyn RunLockService>,
    pub clock: Arc<dyn Clock>,
    pub identifiers: Arc<dyn IdentifierFactory>,
    pub identity: Arc<dyn LocalIdentity>,
    pub git: Arc<dyn GitService>,
    pub processes: Arc<dyn ProcessRunner>,
    pub agent: Arc<dyn AgentDriver>,
    pub tests: Arc<dyn TestGateRunner>,
    pub reviews: Vec<Arc<dyn ReviewProvider>>,
    pub publisher: Arc<dyn DomainEventPublisher>,
    pub redactor: Arc<dyn Redactor>,
    pub host: Arc<dyn HostEnvironment>,
    pub logs: Arc<dyn RunLogWriter>,
}

impl EngineServices {
    pub fn required_review_providers(&self) -> Vec<Arc<dyn ReviewProvider>> {
        self.reviews
            .iter()
            .filter(|provider| provider.required())
            .cloned()
            .collect()
    }
}
