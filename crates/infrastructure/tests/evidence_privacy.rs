use std::io::Read;
use std::sync::Arc;

use heikas_application::configuration::{
    AgentConfiguration, EffectiveConfiguration, GitConfiguration, QualityConfiguration,
    RedactionConfiguration, CONFIGURATION_SCHEMA_VERSION,
};
use heikas_application::ports::clock::{Clock, IdentifierFactory};
use heikas_application::ports::observability::Redactor;
use heikas_application::ports::runtime::EvidenceExporter;
use heikas_application::ports::store::{EventStore, RunCatalogue};
use heikas_domain::budget::RunBudgets;
use heikas_domain::command::CommandCatalogue;
use heikas_domain::event::{DiagnosticLevel, EventPayload};
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;
use heikas_infrastructure::export::ZipEvidenceExporter;
use heikas_infrastructure::layout::StoreLayout;
use heikas_infrastructure::redaction::{PatternRedactor, REDACTION_PLACEHOLDER};
use heikas_infrastructure::store::FileRunStore;
use heikas_infrastructure::system::{SystemClock, UuidIdentifierFactory};
use tempfile::TempDir;

const LEAKED_TOKEN: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";

fn store(layout: &StoreLayout) -> FileRunStore {
    FileRunStore::new(
        layout.clone(),
        Arc::new(SystemClock) as Arc<dyn Clock>,
        Arc::new(UuidIdentifierFactory) as Arc<dyn IdentifierFactory>,
    )
}

fn configuration(repository: &std::path::Path) -> EffectiveConfiguration {
    EffectiveConfiguration {
        schema_version: CONFIGURATION_SCHEMA_VERSION,
        repository_path: repository.to_path_buf(),
        budgets: RunBudgets::default(),
        commit_policy: CommitPolicy::Manual,
        agent: AgentConfiguration::default(),
        quality: QualityConfiguration::default(),
        git: GitConfiguration::default(),
        commands: CommandCatalogue::default(),
        path_policy: PathPolicy::default(),
        redaction: RedactionConfiguration::default(),
        retry: RetryPolicy::default(),
        timeouts: NodeTimeouts::default(),
        environment_allowlist: Vec::new(),
        demonstration_mode: true,
        repository_trust: Default::default(),
        command_source: Default::default(),
    }
}

#[tokio::test]
async fn a_secret_in_an_event_payload_never_reaches_the_durable_log() {
    let directory = TempDir::new().expect("a temporary directory");
    let layout = StoreLayout::new(directory.path().to_path_buf());
    let store = store(&layout);
    let run_id = UuidIdentifierFactory.new_run_id();
    store
        .initialise(run_id, "# Task\n", &configuration(directory.path()))
        .await
        .expect("the run initialises");

    store
        .append(
            run_id,
            EventPayload::DiagnosticRecorded {
                level: DiagnosticLevel::Warning,
                code: "leak".to_string(),
                message: format!("the suite printed {LEAKED_TOKEN} while failing"),
                detail: None,
            },
        )
        .await
        .expect("the event appends");

    let raw = std::fs::read_to_string(layout.events_file(run_id)).expect("the log reads");
    assert!(
        !raw.contains(LEAKED_TOKEN),
        "a secret must never reach the durable event log"
    );
    assert!(raw.contains(REDACTION_PLACEHOLDER));

    let events = store
        .read_after(run_id, 0)
        .await
        .expect("the log replays with a valid chain");
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn the_export_reports_what_it_actually_redacted() {
    let directory = TempDir::new().expect("a temporary directory");
    let layout = StoreLayout::new(directory.path().to_path_buf());
    let store = store(&layout);
    let run_id = UuidIdentifierFactory.new_run_id();
    store
        .initialise(run_id, "# Task\n", &configuration(directory.path()))
        .await
        .expect("the run initialises");

    let reports = layout.integration_directory(run_id).join("reports");
    std::fs::create_dir_all(&reports).expect("the directory creates");
    std::fs::write(
        reports.join("suite.txt"),
        format!("the suite printed {LEAKED_TOKEN}\n"),
    )
    .expect("the report writes");
    std::fs::write(
        reports.join("coverage.bin"),
        [0u8, 159, 146, 150, 0, 1, 2, 3],
    )
    .expect("the binary writes");

    let worktrees = layout.run_worktrees(run_id).join("candidate-a");
    std::fs::create_dir_all(&worktrees).expect("the directory creates");
    std::fs::write(
        worktrees.join("module.py"),
        format!("KEY = \"{LEAKED_TOKEN}\"\n"),
    )
    .expect("the module writes");
    std::fs::write(worktrees.join(".env"), "DATABASE_PASSWORD=hunter2\n")
        .expect("the environment file writes");

    let redactor: Arc<dyn Redactor> = Arc::new(PatternRedactor::without_environment());
    let exporter = ZipEvidenceExporter::new(layout.clone(), redactor);
    let destination = directory.path().join("out").join("evidence.zip");
    let outcome = exporter
        .export(run_id, &destination, true)
        .await
        .expect("the archive is written");

    assert!(
        !outcome.fully_redacted(),
        "an archive containing binary content must not be reported as fully redacted"
    );
    assert_eq!(outcome.unredactable_entries, 1);
    assert!(
        outcome
            .excluded_sensitive_paths
            .iter()
            .any(|path| path.ends_with(".env")),
        "a sensitive worktree path must be excluded, excluded: {:?}",
        outcome.excluded_sensitive_paths
    );

    let file = std::fs::File::open(&destination).expect("the archive opens");
    let mut archive = zip::ZipArchive::new(file).expect("the archive reads");
    let mut names = Vec::new();
    let mut combined = String::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("an entry reads");
        names.push(entry.name().to_string());
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents).expect("the entry reads");
        combined.push_str(&String::from_utf8_lossy(&contents));
    }
    assert!(
        !names.iter().any(|name| name.ends_with(".env")),
        "the sensitive file must be absent from the archive: {names:?}"
    );
    assert!(
        !combined.contains(LEAKED_TOKEN),
        "no text entry may carry the secret through the archive"
    );
    assert!(combined.contains("\"fully_redacted\": false"));
}

#[test]
fn stored_evidence_is_not_world_readable() {
    let directory = TempDir::new().expect("a temporary directory");
    let target = directory.path().join("nested").join("value.json");
    heikas_infrastructure::atomic::write_atomic_json(&target, &serde_json::json!({"value": 1}))
        .expect("the write succeeds");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file_mode = std::fs::metadata(&target)
            .expect("the metadata reads")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            file_mode, 0o600,
            "evidence files must be owner readable only"
        );
        let directory_mode = std::fs::metadata(target.parent().expect("a parent"))
            .expect("the metadata reads")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            directory_mode, 0o700,
            "evidence directories must be owner accessible only"
        );
    }
    #[cfg(not(unix))]
    {
        assert!(target.exists());
    }
}
