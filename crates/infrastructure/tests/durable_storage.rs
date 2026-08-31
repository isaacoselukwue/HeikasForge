use std::fs;
use std::path::Path;
use std::sync::Arc;

use heikas_application::ports::clock::{Clock, IdentifierFactory};
use heikas_application::ports::store::RunLockService;
use heikas_application::ports::store::{EventStore, ProjectionStore, RunCatalogue};
use heikas_domain::event::{EventPayload, GENESIS_HASH};
use heikas_domain::identity::{ContentDigest, RunId};
use heikas_domain::run::{CommitPolicy, RunStatus};
use heikas_domain::state::RunProjection;
use heikas_infrastructure::atomic::{read_json, write_atomic, write_atomic_json};
use heikas_infrastructure::layout::StoreLayout;
use heikas_infrastructure::store::event_log::EventLogFile;
use heikas_infrastructure::store::{FileRunLocks, FileRunStore};
use heikas_infrastructure::system::{SystemClock, UuidIdentifierFactory};
use tempfile::TempDir;

fn layout() -> (TempDir, StoreLayout) {
    let directory = TempDir::new().expect("a temporary directory");
    let layout = StoreLayout::new(directory.path().to_path_buf());
    (directory, layout)
}

fn store(layout: &StoreLayout) -> FileRunStore {
    FileRunStore::new(
        layout.clone(),
        Arc::new(SystemClock) as Arc<dyn Clock>,
        Arc::new(UuidIdentifierFactory) as Arc<dyn IdentifierFactory>,
    )
}

fn created_payload() -> EventPayload {
    EventPayload::RunCreated {
        repository_path: "/repositories/example".to_string(),
        task_title: "Demonstration task".to_string(),
        task_digest: ContentDigest::of_str("task"),
        candidate_count: 1,
        commit_policy: CommitPolicy::Manual,
        agent_driver: "fake".to_string(),
        demonstration_mode: true,
    }
}

fn diagnostic(index: u64) -> EventPayload {
    EventPayload::DiagnosticRecorded {
        level: heikas_domain::event::DiagnosticLevel::Info,
        code: format!("code-{index}"),
        message: format!("diagnostic {index}"),
        detail: None,
    }
}

#[test]
fn an_atomic_write_leaves_no_temporary_file_behind() {
    let directory = TempDir::new().expect("a temporary directory");
    let target = directory.path().join("nested").join("value.json");
    write_atomic_json(&target, &serde_json::json!({"value": 42})).expect("the write succeeds");
    let decoded: serde_json::Value = read_json(&target)
        .expect("the file reads")
        .expect("it exists");
    assert_eq!(decoded["value"], 42);

    let leftovers: Vec<_> = fs::read_dir(target.parent().expect("a parent"))
        .expect("the directory reads")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "no temporary file may remain");
}

#[test]
fn an_atomic_write_replaces_the_previous_content_completely() {
    let directory = TempDir::new().expect("a temporary directory");
    let target = directory.path().join("value.txt");
    write_atomic(&target, b"a much longer original document").expect("the first write succeeds");
    write_atomic(&target, b"short").expect("the second write succeeds");
    assert_eq!(fs::read(&target).expect("the file reads"), b"short");
}

#[test]
fn reading_a_missing_file_reports_absence_rather_than_failing() {
    let directory = TempDir::new().expect("a temporary directory");
    let outcome: Option<serde_json::Value> =
        read_json(&directory.path().join("absent.json")).expect("a missing file is not an error");
    assert!(outcome.is_none());
}

#[tokio::test]
async fn the_event_log_rejects_a_mutated_record() {
    let (_directory, layout) = layout();
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let log = EventLogFile::new(layout.events_file(run), layout.quarantine_file(run));
    for index in 0..4 {
        log.append(
            run,
            heikas_domain::identity::EventId::from_uuid(uuid::Uuid::now_v7()),
            SystemClock.now(),
            diagnostic(index),
        )
        .await
        .expect("the append succeeds");
    }
    assert_eq!(log.read_all().expect("the log reads").len(), 4);

    let path = layout.events_file(run);
    let contents = fs::read_to_string(&path).expect("the log reads");
    let mutated = contents.replacen("diagnostic 2", "diagnostic tampered", 1);
    fs::write(&path, mutated).expect("the log is rewritten");

    let reopened = EventLogFile::new(layout.events_file(run), layout.quarantine_file(run));
    let outcome = reopened.read_all();
    assert!(outcome.is_err(), "a mutated payload must be detected");
}

#[tokio::test]
async fn a_partially_written_final_record_is_quarantined_and_never_committed() {
    let (_directory, layout) = layout();
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let log = EventLogFile::new(layout.events_file(run), layout.quarantine_file(run));
    for index in 0..3 {
        log.append(
            run,
            heikas_domain::identity::EventId::from_uuid(uuid::Uuid::now_v7()),
            SystemClock.now(),
            diagnostic(index),
        )
        .await
        .expect("the append succeeds");
    }

    let path = layout.events_file(run);
    let mut contents = fs::read_to_string(&path).expect("the log reads");
    contents.push_str("{\"schema_version\":1,\"sequence\":4,\"event_id\":\"partial");
    fs::write(&path, contents).expect("the log is rewritten");

    let reopened = EventLogFile::new(layout.events_file(run), layout.quarantine_file(run));
    let verification = reopened.verify().await.expect("verification succeeds");
    assert!(verification.quarantined_partial_record);
    assert_eq!(verification.events_verified, 3);
    assert_eq!(verification.last_sequence, 3);
    assert!(layout.quarantine_file(run).exists());

    let events = reopened.read_all().expect("the log reads");
    assert_eq!(
        events.len(),
        3,
        "a partial record is never treated as committed"
    );

    let appended = reopened
        .append(
            run,
            heikas_domain::identity::EventId::from_uuid(uuid::Uuid::now_v7()),
            SystemClock.now(),
            diagnostic(9),
        )
        .await
        .expect("appending continues after quarantine");
    assert_eq!(appended.sequence, 4);
    assert_eq!(reopened.read_all().expect("the log reads").len(), 4);
}

#[tokio::test]
async fn the_first_event_links_to_the_genesis_hash() {
    let (_directory, layout) = layout();
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let log = EventLogFile::new(layout.events_file(run), layout.quarantine_file(run));
    let event = log
        .append(
            run,
            heikas_domain::identity::EventId::from_uuid(uuid::Uuid::now_v7()),
            SystemClock.now(),
            created_payload(),
        )
        .await
        .expect("the append succeeds");
    assert_eq!(event.previous_hash, GENESIS_HASH);
    assert_eq!(event.sequence, 1);
}

#[tokio::test]
async fn a_stale_projection_is_repaired_by_replaying_newer_events() {
    let (_directory, layout) = layout();
    let store = store(&layout);
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let configuration = sample_configuration();
    store
        .initialise(run, "Demonstration task", &configuration)
        .await
        .expect("the run initialises");

    let created = store.append(run, created_payload()).await.expect("append");
    let mut projection = RunProjection::genesis(run, created.recorded_at);
    projection.apply(&created).expect("the projection applies");
    store
        .store(&projection)
        .await
        .expect("the projection stores");

    let later = store
        .append(
            run,
            EventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Validating,
                reason: None,
            },
        )
        .await
        .expect("append");

    let stored = store
        .load(run)
        .await
        .expect("load")
        .expect("a projection exists");
    assert_eq!(stored.last_event_sequence, 1);

    let pending = store
        .read_after(run, stored.last_event_sequence)
        .await
        .expect("read");
    assert_eq!(pending.len(), 1);
    let mut repaired = stored;
    heikas_domain::state::replay_from(&mut repaired, &pending).expect("replay");
    assert_eq!(repaired.last_event_sequence, later.sequence);
    assert_eq!(repaired.status, RunStatus::Validating);
}

#[tokio::test]
async fn completed_attempt_evidence_is_never_overwritten() {
    use heikas_application::model::attempt::{AttemptEvidence, AttemptKey};
    use heikas_application::ports::store::EvidenceStore;
    use heikas_domain::graph::NodeId;
    use heikas_domain::identity::AttemptNumber;
    use heikas_domain::node::NodeResult;

    let (_directory, layout) = layout();
    let store = store(&layout);
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    store
        .initialise(run, "task", &sample_configuration())
        .await
        .expect("the run initialises");

    let started = SystemClock.now();
    let result = NodeResult::builder(run, NodeId::Prepare, AttemptNumber::FIRST, started)
        .succeeded(started, Some(NodeId::Plan));
    store
        .commit_attempt(run, &result, AttemptEvidence::default())
        .await
        .expect("the first commit succeeds");

    let second = store
        .commit_attempt(run, &result, AttemptEvidence::default())
        .await;
    assert!(second.is_err(), "existing evidence must never be replaced");

    let key = AttemptKey::new(NodeId::Prepare, None, AttemptNumber::FIRST);
    let loaded = store
        .read_attempt_result(run, &key)
        .await
        .expect("the result reads")
        .expect("the result exists");
    assert_eq!(loaded.node_id, NodeId::Prepare);
}

#[tokio::test]
async fn only_one_dispatcher_may_hold_the_run_lock() {
    let (_directory, layout) = layout();
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let locks = FileRunLocks::new(layout.clone());
    assert!(!locks.is_locked(run).await.expect("the probe succeeds"));

    let guard = locks.acquire(run).await.expect("the lock is acquired");
    assert!(locks.is_locked(run).await.expect("the probe succeeds"));

    let contended =
        tokio::time::timeout(std::time::Duration::from_millis(600), locks.acquire(run)).await;
    assert!(
        contended.is_err(),
        "a second dispatcher must not acquire the lock while it is held"
    );

    guard.release();
    assert!(!locks.is_locked(run).await.expect("the probe succeeds"));
    let reacquired = locks
        .acquire(run)
        .await
        .expect("the lock is available again");
    reacquired.release();
}

#[tokio::test]
async fn a_run_reference_resolves_from_a_short_prefix() {
    let (_directory, layout) = layout();
    let store = store(&layout);
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    store
        .initialise(run, "task", &sample_configuration())
        .await
        .expect("the run initialises");
    let created = store.append(run, created_payload()).await.expect("append");
    let mut projection = RunProjection::genesis(run, created.recorded_at);
    projection.apply(&created).expect("apply");
    store.store(&projection).await.expect("store");

    let resolved = store
        .resolve_run_reference(&run.short()[..8])
        .await
        .expect("a short prefix resolves");
    assert_eq!(resolved, run);

    assert!(store.resolve_run_reference("zzzzzzzz").await.is_err());
}

fn sample_configuration() -> heikas_application::configuration::EffectiveConfiguration {
    use heikas_application::configuration::*;
    use heikas_domain::budget::RunBudgets;
    use heikas_domain::command::CommandCatalogue;
    use heikas_domain::path_policy::PathPolicy;
    use heikas_domain::retry::{NodeTimeouts, RetryPolicy};

    EffectiveConfiguration {
        schema_version: CONFIGURATION_SCHEMA_VERSION,
        repository_path: Path::new("/repositories/example").to_path_buf(),
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
        detection_notes: Vec::new(),
    }
}

#[tokio::test]
async fn an_oversized_event_record_is_refused() {
    let (_directory, layout) = layout();
    let store = store(&layout);
    let run_id = UuidIdentifierFactory.new_run_id();
    store
        .initialise(run_id, "# Task\n", &sample_configuration())
        .await
        .expect("the run initialises");

    let outcome = store
        .append(
            run_id,
            EventPayload::DiagnosticRecorded {
                level: heikas_domain::event::DiagnosticLevel::Info,
                code: "oversized".to_string(),
                message: "a"
                    .repeat(heikas_infrastructure::store::event_log::MAXIMUM_RECORD_BYTES + 1),
                detail: None,
            },
        )
        .await;
    assert!(
        outcome.is_err(),
        "a record beyond the size limit must be refused rather than written"
    );
}

#[tokio::test]
async fn repeated_reads_reuse_the_verified_prefix() {
    let (_directory, layout) = layout();
    let store = store(&layout);
    let run_id = UuidIdentifierFactory.new_run_id();
    store
        .initialise(run_id, "# Task\n", &sample_configuration())
        .await
        .expect("the run initialises");
    store
        .append(run_id, created_payload())
        .await
        .expect("the event appends");
    for index in 0..25 {
        store
            .append(run_id, diagnostic(index))
            .await
            .expect("the event appends");
    }

    let first = store.read_range(run_id, 0, 5).await.expect("a page reads");
    assert_eq!(first.len(), 5);
    let all = store.read_after(run_id, 0).await.expect("the log reads");
    assert_eq!(all.len(), 26);
    let verification = store
        .verify_chain(run_id)
        .await
        .expect("the chain verifies");
    assert_eq!(verification.events_verified, 26);
    assert!(!verification.quarantined_partial_record);
}
