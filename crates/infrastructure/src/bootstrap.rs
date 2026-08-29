use std::sync::Arc;

use heikas_application::error::ApplicationResult;
use heikas_application::ports::clock::{Clock, IdentifierFactory, LocalIdentity};
use heikas_application::ports::environment::HostEnvironment;
use heikas_application::ports::git::GitService;
use heikas_application::ports::observability::{DomainEventPublisher, Redactor, RunLogReader};
use heikas_application::ports::process::ProcessRunner;
use heikas_application::ports::runtime::{ConfigurationResolver, EvidenceExporter, RuntimeFactory};
use heikas_application::ports::store::{RunLockService, RunStore};
use heikas_application::usecases::{ApplicationService, BaseServices};

use crate::configuration::LayeredConfigurationResolver;
use crate::export::ZipEvidenceExporter;
use crate::git::CommandLineGitService;
use crate::layout::StoreLayout;
use crate::process::supervisor::essential_environment_variables;
use crate::process::SupervisedProcessRunner;
use crate::redaction::PatternRedactor;
use crate::runtime::AdapterRuntimeFactory;
use crate::store::{FileRunLocks, FileRunStore};
use crate::system::{
    LocalHostEnvironment, OperatingSystemIdentity, SystemClock, UuidIdentifierFactory,
};
use crate::telemetry::{BroadcastEventPublisher, FileRunLog};

pub const DEFAULT_AUTHOR_NAME: &str = "Isaac Oselukwue";

#[derive(Clone)]
pub struct Runtime {
    pub service: Arc<ApplicationService>,
    pub layout: StoreLayout,
    pub events: BroadcastEventPublisher,
    pub logs: Arc<FileRunLog>,
    pub store: Arc<dyn RunStore>,
    pub git: Arc<dyn GitService>,
    pub processes: Arc<dyn ProcessRunner>,
    pub host: Arc<dyn HostEnvironment>,
    pub configuration: Arc<dyn ConfigurationResolver>,
    pub factory: Arc<dyn RuntimeFactory>,
}

impl Runtime {
    pub fn log_reader(&self) -> Arc<dyn RunLogReader> {
        Arc::clone(&self.logs) as Arc<dyn RunLogReader>
    }
}

pub fn build_runtime(layout: StoreLayout) -> ApplicationResult<Runtime> {
    crate::atomic::ensure_directory(layout.root())?;
    crate::atomic::ensure_directory(&layout.runs_directory())?;
    crate::atomic::ensure_directory(&layout.config_directory())?;
    crate::atomic::ensure_directory(&layout.worktrees_directory())?;

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let identifiers: Arc<dyn IdentifierFactory> = Arc::new(UuidIdentifierFactory);
    let identity: Arc<dyn LocalIdentity> = Arc::new(OperatingSystemIdentity);
    let processes: Arc<dyn ProcessRunner> = Arc::new(SupervisedProcessRunner::new(
        essential_environment_variables()
            .into_iter()
            .map(str::to_string)
            .collect(),
    ));
    let git: Arc<dyn GitService> = Arc::new(CommandLineGitService::new(
        Arc::clone(&processes),
        layout.clone(),
        DEFAULT_AUTHOR_NAME.to_string(),
    ));
    let store_implementation = Arc::new(FileRunStore::new(
        layout.clone(),
        Arc::clone(&clock),
        Arc::clone(&identifiers),
    ));
    let store: Arc<dyn RunStore> = store_implementation;
    let locks: Arc<dyn RunLockService> = Arc::new(FileRunLocks::new(layout.clone()));
    let publisher = BroadcastEventPublisher::new();
    let redactor: Arc<dyn Redactor> = Arc::new(PatternRedactor::without_environment());
    let logs = Arc::new(FileRunLog::new(layout.clone(), Arc::clone(&redactor)));
    let host: Arc<dyn HostEnvironment> = Arc::new(LocalHostEnvironment::new(layout.clone()));
    let configuration: Arc<dyn ConfigurationResolver> =
        Arc::new(LayeredConfigurationResolver::new(layout.clone()));
    let factory: Arc<dyn RuntimeFactory> = Arc::new(AdapterRuntimeFactory::new(
        Arc::clone(&processes),
        Arc::clone(&git),
        Arc::clone(&clock),
    ));
    let exporter: Arc<dyn EvidenceExporter> = Arc::new(ZipEvidenceExporter::new(
        layout.clone(),
        Arc::clone(&redactor),
    ));

    let base = BaseServices {
        store: Arc::clone(&store),
        locks,
        clock: Arc::clone(&clock),
        identifiers,
        identity,
        git: Arc::clone(&git),
        processes: Arc::clone(&processes),
        publisher: Arc::new(publisher.clone()) as Arc<dyn DomainEventPublisher>,
        host: Arc::clone(&host),
        logs: Arc::clone(&logs) as Arc<dyn heikas_application::ports::observability::RunLogWriter>,
    };

    let service = Arc::new(ApplicationService::new(
        base,
        Arc::clone(&factory),
        Arc::clone(&configuration),
        exporter,
    ));

    Ok(Runtime {
        service,
        layout,
        events: publisher,
        logs,
        store,
        git,
        processes,
        host,
        configuration,
        factory,
    })
}
