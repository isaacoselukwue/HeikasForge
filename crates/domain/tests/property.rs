use std::str::FromStr;

use heikas_domain::candidate::CandidateRecord;
use heikas_domain::clock::{DurationMs, Timestamp};
use heikas_domain::event::{DurableEvent, EventPayload, GENESIS_HASH};
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{
    AttemptNumber, CandidateId, CandidateOrdinal, CommitHash, ContentDigest, EventId, RunId,
};
use heikas_domain::path_policy::{
    evaluate_path, PathAccess, PathPolicy, PatternMatcher, RelativeWorkspacePath,
};
use heikas_domain::review::AggregatedReview;
use heikas_domain::run::CandidateStrategy;
use heikas_domain::score::ScoreTuple;
use heikas_domain::state::replay;
use heikas_domain::test_evidence::TestEvidence;
use proptest::prelude::*;
use uuid::Uuid;

struct WildcardMatcher;

impl PatternMatcher for WildcardMatcher {
    fn matches(&self, pattern: &str, path: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix("/**") {
            return path == prefix || path.starts_with(&format!("{prefix}/"));
        }
        if let Some(suffix) = pattern.strip_prefix("**/") {
            return path
                .split('/')
                .any(|segment| wildcard_matches(suffix, segment))
                || wildcard_matches(suffix, path);
        }
        wildcard_matches(pattern, path)
    }
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern_characters: Vec<char> = pattern.chars().collect();
    let value_characters: Vec<char> = value.chars().collect();
    let mut pattern_index = 0usize;
    let mut value_index = 0usize;
    let mut star_index: Option<usize> = None;
    let mut match_index = 0usize;
    while value_index < value_characters.len() {
        if pattern_index < pattern_characters.len()
            && (pattern_characters[pattern_index] == '?'
                || pattern_characters[pattern_index] == value_characters[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern_characters.len()
            && pattern_characters[pattern_index] == '*'
        {
            star_index = Some(pattern_index);
            match_index = value_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            value_index = match_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern_characters.len() && pattern_characters[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern_characters.len()
}

fn run_id() -> RunId {
    RunId::from_uuid(Uuid::from_u128(0x0198_f5b0_42f0_7fd1_a164_93f3_13c6_b1b8))
}

fn moment(offset: i128) -> Timestamp {
    Timestamp::from_unix_nanos(1_700_000_000_000_000_000 + offset).expect("a valid timestamp")
}

fn candidate_record(ordinal: u8, changed_lines: u64, repairs: u32, gate: u64) -> CandidateRecord {
    let ordinal = CandidateOrdinal::new(ordinal).expect("a valid ordinal");
    let mut record = CandidateRecord::new(
        CandidateId::derive(run_id(), ordinal),
        ordinal,
        CandidateStrategy::for_ordinal(ordinal.get()),
        CommitHash::from_str(&"c".repeat(40)).expect("a valid hash"),
        "branch".to_string(),
        "worktree".to_string(),
        3,
    );
    record.changed_lines = changed_lines;
    record.repairs_used = repairs;
    record.gate_duration = DurationMs::from_millis(gate);
    record
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_sealed_chain_of_any_length_always_verifies(count in 1usize..40) {
        let mut previous = GENESIS_HASH.to_string();
        let mut events = Vec::new();
        for sequence in 1..=count as u64 {
            let payload = EventPayload::DiagnosticRecorded {
                level: heikas_domain::event::DiagnosticLevel::Info,
                code: format!("code-{sequence}"),
                message: format!("diagnostic {sequence}"),
                detail: None,
            };
            let event = DurableEvent::seal(
                sequence,
                EventId::from_uuid(Uuid::from_u128(u128::from(sequence))),
                run_id(),
                moment(i128::from(sequence)),
                &previous,
                payload,
            )
            .expect("the event seals");
            previous = event.chain_hash();
            events.push(event);
        }
        let mut expected = GENESIS_HASH.to_string();
        for (index, event) in events.iter().enumerate() {
            prop_assert!(event.verify(index as u64 + 1, &expected).is_ok());
            expected = event.chain_hash();
        }
    }

    #[test]
    fn replaying_a_prefix_then_the_remainder_matches_a_full_replay(count in 1usize..24, split in 0usize..24) {
        let mut previous = GENESIS_HASH.to_string();
        let mut events = Vec::new();
        for sequence in 1..=count as u64 {
            let payload = EventPayload::DiagnosticRecorded {
                level: heikas_domain::event::DiagnosticLevel::Info,
                code: format!("code-{sequence}"),
                message: format!("diagnostic {sequence}"),
                detail: None,
            };
            let event = DurableEvent::seal(
                sequence,
                EventId::from_uuid(Uuid::from_u128(u128::from(sequence))),
                run_id(),
                moment(i128::from(sequence)),
                &previous,
                payload,
            )
            .expect("the event seals");
            previous = event.chain_hash();
            events.push(event);
        }
        let boundary = split.min(events.len());
        let complete = replay(run_id(), moment(0), &events).expect("a full replay succeeds");
        let mut partial = replay(run_id(), moment(0), &events[..boundary]).expect("a prefix replays");
        heikas_domain::state::replay_from(&mut partial, &events[boundary..]).expect("the remainder replays");
        prop_assert_eq!(complete.last_event_sequence, partial.last_event_sequence);
        prop_assert_eq!(&complete.last_event_hash, &partial.last_event_hash);
        prop_assert_eq!(
            serde_json::to_string(&complete).expect("encodes"),
            serde_json::to_string(&partial).expect("encodes")
        );
    }

    #[test]
    fn score_ordering_is_a_total_order(
        lines_a in 0u64..5_000,
        lines_b in 0u64..5_000,
        repairs_a in 0u32..8,
        repairs_b in 0u32..8,
        gate_a in 0u64..600_000,
        gate_b in 0u64..600_000,
    ) {
        let review = AggregatedReview::default();
        let tests = TestEvidence::default();
        let first = ScoreTuple::build(&candidate_record(1, lines_a, repairs_a, gate_a), &review, &tests);
        let second = ScoreTuple::build(&candidate_record(2, lines_b, repairs_b, gate_b), &review, &tests);
        let forward = first.cmp(&second);
        let backward = second.cmp(&first);
        prop_assert_eq!(forward, backward.reverse());
        prop_assert_eq!(first.cmp(&first), std::cmp::Ordering::Equal);
    }

    #[test]
    fn score_ordering_is_transitive(
        lines in prop::collection::vec(0u64..2_000, 3..4),
        repairs in prop::collection::vec(0u32..5, 3..4),
    ) {
        let review = AggregatedReview::default();
        let tests = TestEvidence::default();
        let tuples: Vec<ScoreTuple> = (0..3)
            .map(|index| {
                ScoreTuple::build(
                    &candidate_record(
                        index as u8 + 1,
                        lines[index],
                        repairs[index],
                        1_000,
                    ),
                    &review,
                    &tests,
                )
            })
            .collect();
        let mut sorted = tuples.clone();
        sorted.sort();
        for window in sorted.windows(2) {
            prop_assert!(window[0] <= window[1]);
        }
        if tuples[0] <= tuples[1] && tuples[1] <= tuples[2] {
            prop_assert!(tuples[0] <= tuples[2]);
        }
    }

    #[test]
    fn an_identifier_round_trips_through_its_text_form(seed in any::<u128>()) {
        let identifier = RunId::from_uuid(Uuid::from_u128(seed));
        let parsed = RunId::from_str(&identifier.to_string()).expect("a run identifier parses");
        prop_assert_eq!(identifier, parsed);
        prop_assert_eq!(identifier.short().len(), 12);
    }

    #[test]
    fn arbitrary_text_never_parses_as_a_content_digest_unless_it_is_hexadecimal(
        text in "[^ ]{0,80}"
    ) {
        let outcome = ContentDigest::from_str(&text);
        let valid = text.len() == 64 && text.chars().all(|character| character.is_ascii_hexdigit());
        prop_assert_eq!(outcome.is_ok(), valid);
    }

    #[test]
    fn a_candidate_identifier_round_trips_and_reports_its_ordinal(ordinal in 1u8..=8) {
        let ordinal = CandidateOrdinal::new(ordinal).expect("a valid ordinal");
        let identifier = CandidateId::derive(run_id(), ordinal);
        let parsed = CandidateId::from_str(identifier.as_str()).expect("a candidate identifier parses");
        prop_assert_eq!(&identifier, &parsed);
        prop_assert_eq!(parsed.ordinal(), Some(ordinal));
    }

    #[test]
    fn an_attempt_key_is_stable(node_index in 0usize..14, attempt in 1u32..50) {
        let node = NodeId::ALL[node_index];
        let attempt = AttemptNumber::new(attempt).expect("a valid attempt");
        prop_assert_eq!(NodeId::from_str(node.as_str()).expect("the node parses"), node);
        prop_assert_eq!(attempt.next().get(), attempt.get() + 1);
    }

    #[test]
    fn a_path_with_a_parent_component_never_parses(
        prefix in "[a-z]{1,8}",
        suffix in "[a-z]{1,8}",
    ) {
        let candidate = format!("{prefix}/../{suffix}");
        prop_assert!(RelativeWorkspacePath::parse(&candidate).is_err());
    }

    #[test]
    fn an_absolute_path_never_parses(segments in prop::collection::vec("[a-z]{1,6}", 1..5)) {
        let candidate = format!("/{}", segments.join("/"));
        prop_assert!(RelativeWorkspacePath::parse(&candidate).is_err());
    }

    #[test]
    fn a_plain_relative_path_always_parses_and_normalises(
        segments in prop::collection::vec("[a-z][a-z0-9_]{0,7}", 1..6)
    ) {
        let candidate = segments.join("/");
        let parsed = RelativeWorkspacePath::parse(&candidate).expect("a plain path parses");
        prop_assert_eq!(parsed.as_str(), candidate.as_str());
        let with_noise = format!("./{}", segments.join("/./"));
        let normalised = RelativeWorkspacePath::parse(&with_noise).expect("noise normalises");
        prop_assert_eq!(normalised.as_str(), candidate.as_str());
    }

    #[test]
    fn a_protected_path_is_never_writable(name in "[a-z]{1,10}") {
        let policy = PathPolicy::default();
        let candidate = format!(".git/{name}");
        let parsed = RelativeWorkspacePath::parse(&candidate).expect("the path parses");
        let write = evaluate_path(&policy, &WildcardMatcher, &parsed, PathAccess::Write);
        prop_assert!(write.is_err());
        let read = evaluate_path(&policy, &WildcardMatcher, &parsed, PathAccess::Read);
        prop_assert!(read.is_ok());
    }

    #[test]
    fn a_sensitive_path_is_never_readable(stem in "[a-z]{1,10}") {
        let policy = PathPolicy::default();
        let candidate = format!("secrets/{stem}.pem");
        let parsed = RelativeWorkspacePath::parse(&candidate).expect("the path parses");
        for access in [PathAccess::Read, PathAccess::Write, PathAccess::Delete] {
            prop_assert!(evaluate_path(&policy, &WildcardMatcher, &parsed, access).is_err());
        }
    }
}
