use std::str::FromStr;

use heikas_domain::candidate::CandidateStatus;
use heikas_domain::clock::Timestamp;
use heikas_domain::event::{DurableEvent, EventPayload, GENESIS_HASH};
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{
    AttemptNumber, CandidateId, CandidateOrdinal, CommitHash, ContentDigest, EventId, RunId,
};
use heikas_domain::run::{CandidateStrategy, CommitPolicy, RunStatus};
use heikas_domain::state::{replay, RunProjection};
use heikas_domain::DomainError;
use uuid::Uuid;

fn run_id() -> RunId {
    RunId::from_uuid(Uuid::from_u128(0x0198_f5b0_42f0_7fd1_a164_93f3_13c6_b1b8))
}

fn event_id(seed: u128) -> EventId {
    EventId::from_uuid(Uuid::from_u128(seed))
}

fn moment(offset: i128) -> Timestamp {
    Timestamp::from_unix_nanos(1_700_000_000_000_000_000 + offset).expect("a valid timestamp")
}

struct Chain {
    events: Vec<DurableEvent>,
    previous: String,
    sequence: u64,
}

impl Chain {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            previous: GENESIS_HASH.to_string(),
            sequence: 0,
        }
    }

    fn push(&mut self, payload: EventPayload) -> DurableEvent {
        self.sequence += 1;
        let event = DurableEvent::seal(
            self.sequence,
            event_id(u128::from(self.sequence) + 1),
            run_id(),
            moment(i128::from(self.sequence) * 1_000_000),
            &self.previous,
            payload,
        )
        .expect("the event seals");
        self.previous = event.chain_hash();
        self.events.push(event.clone());
        event
    }
}

fn created_payload() -> EventPayload {
    EventPayload::RunCreated {
        repository_path: "/repositories/example".to_string(),
        task_title: "Round invoice currency half away from zero".to_string(),
        task_digest: ContentDigest::of_str("task"),
        candidate_count: 2,
        commit_policy: CommitPolicy::Manual,
        agent_driver: "fake".to_string(),
        demonstration_mode: true,
    }
}

fn baseline_payload() -> EventPayload {
    EventPayload::BaselineResolved {
        baseline_commit: CommitHash::from_str("a".repeat(40).as_str()).expect("a valid hash"),
        default_branch: "main".to_string(),
        dirty_snapshot: false,
    }
}

fn full_chain() -> Chain {
    let mut chain = Chain::new();
    chain.push(created_payload());
    chain.push(EventPayload::RunStatusChanged {
        from: RunStatus::Created,
        to: RunStatus::Validating,
        reason: None,
    });
    chain.push(baseline_payload());
    chain.push(EventPayload::NodeStarted {
        node_id: NodeId::Prepare,
        candidate_id: None,
        attempt: AttemptNumber::FIRST,
        prompt_template_hash: None,
    });
    chain.push(EventPayload::NodeSucceeded {
        node_id: NodeId::Prepare,
        candidate_id: None,
        attempt: AttemptNumber::FIRST,
        duration: heikas_domain::clock::DurationMs::from_millis(1_200),
        next: Some(NodeId::Plan),
        result_digest: ContentDigest::of_str("prepare"),
    });
    chain.push(EventPayload::CandidateRegistered {
        candidate_id: CandidateId::derive(run_id(), CandidateOrdinal::new(1).expect("ordinal")),
        ordinal: CandidateOrdinal::new(1).expect("ordinal"),
        strategy: CandidateStrategy::MinimalPatch,
        branch: "heikas/work/c01".to_string(),
        worktree_relative_path: "worktrees/run/c01".to_string(),
        repair_budget: 2,
    });
    chain
}

#[test]
fn a_sealed_event_verifies_against_its_position_in_the_chain() {
    let chain = full_chain();
    let mut previous = GENESIS_HASH.to_string();
    for (index, event) in chain.events.iter().enumerate() {
        event
            .verify(index as u64 + 1, &previous)
            .expect("each event verifies in order");
        previous = event.chain_hash();
    }
}

#[test]
fn a_reordered_event_breaks_the_chain() {
    let chain = full_chain();
    let second = &chain.events[1];
    let outcome = second.verify(1, GENESIS_HASH);
    assert!(matches!(outcome, Err(DomainError::EventSequenceGap { .. })));
}

#[test]
fn a_mutated_payload_is_rejected_by_the_payload_hash() {
    let mut chain = full_chain();
    let event = &mut chain.events[0];
    event.payload = EventPayload::RunCreated {
        repository_path: "/repositories/tampered".to_string(),
        task_title: "Round invoice currency half away from zero".to_string(),
        task_digest: ContentDigest::of_str("task"),
        candidate_count: 2,
        commit_policy: CommitPolicy::Manual,
        agent_driver: "fake".to_string(),
        demonstration_mode: true,
    };
    let outcome = event.verify(1, GENESIS_HASH);
    assert!(matches!(
        outcome,
        Err(DomainError::EventPayloadHashMismatch { .. })
    ));
}

#[test]
fn a_truncated_predecessor_breaks_the_following_link() {
    let chain = full_chain();
    let outcome = chain.events[2].verify(3, GENESIS_HASH);
    assert!(matches!(outcome, Err(DomainError::EventChainBroken { .. })));
}

#[test]
fn replay_reconstructs_the_projection_exactly() {
    let chain = full_chain();
    let projection = replay(run_id(), moment(0), &chain.events).expect("replay succeeds");
    assert_eq!(projection.status, RunStatus::Validating);
    assert_eq!(projection.candidate_count, 2);
    assert!(projection.demonstration_mode);
    assert_eq!(projection.candidates.len(), 1);
    assert_eq!(projection.candidates[0].status, CandidateStatus::Pending);
    assert_eq!(projection.last_event_sequence, chain.events.len() as u64);
    assert_eq!(
        projection.last_event_hash,
        chain.events.last().expect("an event").chain_hash()
    );
    assert!(projection.completed_successfully(NodeId::Prepare, None));
}

#[test]
fn replay_is_deterministic_across_repeated_evaluation() {
    let chain = full_chain();
    let first = replay(run_id(), moment(0), &chain.events).expect("replay succeeds");
    let second = replay(run_id(), moment(0), &chain.events).expect("replay succeeds");
    assert_eq!(
        serde_json::to_string(&first).expect("encodes"),
        serde_json::to_string(&second).expect("encodes")
    );
}

#[test]
fn a_projection_refuses_an_event_from_another_run() {
    let chain = full_chain();
    let other = RunId::from_uuid(Uuid::from_u128(7));
    let mut projection = RunProjection::genesis(other, moment(0));
    let outcome = projection.apply(&chain.events[0]);
    assert!(matches!(outcome, Err(DomainError::InvariantViolated(_))));
}

#[test]
fn a_projection_refuses_a_gap_in_the_sequence() {
    let chain = full_chain();
    let mut projection = RunProjection::genesis(run_id(), moment(0));
    let outcome = projection.apply(&chain.events[1]);
    assert!(matches!(outcome, Err(DomainError::EventSequenceGap { .. })));
}

#[test]
fn a_duplicate_node_attempt_is_an_invariant_violation() {
    let mut chain = Chain::new();
    chain.push(created_payload());
    chain.push(baseline_payload());
    chain.push(EventPayload::NodeStarted {
        node_id: NodeId::Plan,
        candidate_id: None,
        attempt: AttemptNumber::FIRST,
        prompt_template_hash: None,
    });
    chain.push(EventPayload::NodeStarted {
        node_id: NodeId::Plan,
        candidate_id: None,
        attempt: AttemptNumber::FIRST,
        prompt_template_hash: None,
    });
    let outcome = replay(run_id(), moment(0), &chain.events);
    assert!(matches!(outcome, Err(DomainError::InvariantViolated(_))));
}

#[test]
fn closing_an_attempt_that_never_started_is_rejected() {
    let mut chain = Chain::new();
    chain.push(created_payload());
    chain.push(EventPayload::NodeSucceeded {
        node_id: NodeId::Plan,
        candidate_id: None,
        attempt: AttemptNumber::FIRST,
        duration: heikas_domain::clock::DurationMs::ZERO,
        next: Some(NodeId::Approval),
        result_digest: ContentDigest::of_str("plan"),
    });
    let outcome = replay(run_id(), moment(0), &chain.events);
    assert!(matches!(outcome, Err(DomainError::InvariantViolated(_))));
}

#[test]
fn an_illegal_run_status_change_is_rejected_during_replay() {
    let mut chain = Chain::new();
    chain.push(created_payload());
    chain.push(EventPayload::RunStatusChanged {
        from: RunStatus::Created,
        to: RunStatus::Succeeded,
        reason: None,
    });
    let outcome = replay(run_id(), moment(0), &chain.events);
    assert!(matches!(
        outcome,
        Err(DomainError::IllegalRunTransition { .. })
    ));
}

#[test]
fn every_event_payload_reports_a_stable_type_name() {
    let chain = full_chain();
    for event in &chain.events {
        assert_eq!(event.event_type, event.payload.type_name());
        assert!(!event.payload.human_summary().is_empty());
    }
}
