use std::str::FromStr;

use heikas_domain::candidate::{CandidateRecord, CandidateStatus};
use heikas_domain::clock::{DurationMs, Timestamp};
use heikas_domain::identity::{
    ApprovalId, CandidateId, CandidateOrdinal, CommitHash, ContentDigest, RunId,
};
use heikas_domain::plan::{
    validate_plan_document, ApprovalDecision, PlanApproval, PlanAuthor, PlanHistory, PlanVersion,
    REQUIRED_PLAN_HEADINGS,
};
use heikas_domain::review::{
    AggregatedReview, IssueCategory, IssueSeverity, QualityGateOutcome, ReviewIssue, ReviewMetrics,
    ReviewReport, REVIEW_REPORT_SCHEMA_VERSION,
};
use heikas_domain::run::CandidateStrategy;
use heikas_domain::score::{
    evaluate_eligibility, rank_candidates, CoverageRank, EligibilityInput, ExclusionReason,
    RankedCandidate, ScoreTuple,
};
use heikas_domain::test_evidence::TestEvidence;
use uuid::Uuid;

fn run_id() -> RunId {
    RunId::from_uuid(Uuid::from_u128(0x0198_f5b0_42f0_7fd1_a164_93f3_13c6_b1b8))
}

fn moment() -> Timestamp {
    Timestamp::from_unix_nanos(1_700_000_000_000_000_000).expect("a valid timestamp")
}

fn candidate(ordinal: u8, changed_lines: u64, repairs: u32, gate_millis: u64) -> CandidateRecord {
    let ordinal = CandidateOrdinal::new(ordinal).expect("a valid ordinal");
    let mut record = CandidateRecord::new(
        CandidateId::derive(run_id(), ordinal),
        ordinal,
        CandidateStrategy::for_ordinal(ordinal.get()),
        CommitHash::from_str(&"b".repeat(40)).expect("a valid hash"),
        format!("heikas/work/c{:02}", ordinal.get()),
        format!("worktrees/run/c{:02}", ordinal.get()),
        3,
    );
    record.changed_lines = changed_lines;
    record.repairs_used = repairs;
    record.gate_duration = DurationMs::from_millis(gate_millis);
    record
}

fn passing_review(coverage: Option<f64>) -> AggregatedReview {
    AggregatedReview {
        reports: vec![ReviewReport {
            schema_version: REVIEW_REPORT_SCHEMA_VERSION,
            provider: "local".to_string(),
            required: true,
            advisory: false,
            passed: true,
            quality_gate: QualityGateOutcome::Passed,
            issues: Vec::new(),
            metrics: ReviewMetrics {
                line_coverage_percent: coverage,
                ..ReviewMetrics::default()
            },
            artifacts: Vec::new(),
            started_at: moment(),
            finished_at: moment(),
            failure_summary: None,
        }],
    }
}

fn eligible_input() -> EligibilityInput {
    EligibilityInput {
        candidate_status: CandidateStatus::Eligible,
        required_tests_passed: true,
        failed_test_commands: Vec::new(),
        missing_test_commands: Vec::new(),
        diff_is_empty: false,
        change_required: true,
        diff_applies: Ok(()),
        coverage_percent: Some(92.0),
        minimum_line_coverage: Some(80.0),
        repairs_used: 0,
        repair_budget: 3,
        repair_budget_exhausted: false,
        time_budget_exceeded: None,
    }
}

#[test]
fn a_candidate_that_satisfies_every_gate_is_eligible() {
    let outcome = evaluate_eligibility(&eligible_input(), &passing_review(Some(92.0)));
    assert!(outcome.eligible, "reasons: {:?}", outcome.reasons);
    assert!(outcome.reasons.is_empty());
}

#[test]
fn an_empty_diff_excludes_a_candidate_when_change_is_required() {
    let mut input = eligible_input();
    input.diff_is_empty = true;
    let outcome = evaluate_eligibility(&input, &passing_review(Some(92.0)));
    assert!(!outcome.eligible);
    assert!(outcome.reasons.contains(&ExclusionReason::EmptyDiff));
}

#[test]
fn an_empty_diff_is_permitted_when_the_task_requires_no_change() {
    let mut input = eligible_input();
    input.diff_is_empty = true;
    input.change_required = false;
    let outcome = evaluate_eligibility(&input, &passing_review(Some(92.0)));
    assert!(outcome.eligible, "reasons: {:?}", outcome.reasons);
}

#[test]
fn coverage_below_the_required_threshold_excludes_a_candidate() {
    let mut input = eligible_input();
    input.coverage_percent = Some(71.5);
    let outcome = evaluate_eligibility(&input, &passing_review(Some(71.5)));
    assert!(!outcome.eligible);
    assert!(outcome
        .reasons
        .iter()
        .any(|reason| matches!(reason, ExclusionReason::CoverageBelowThreshold { .. })));
}

#[test]
fn a_missing_required_review_provider_excludes_a_candidate() {
    let outcome = evaluate_eligibility(&eligible_input(), &AggregatedReview::default());
    assert!(!outcome.eligible);
    assert!(outcome
        .reasons
        .contains(&ExclusionReason::RequiredReviewMissing));
}

#[test]
fn a_blocker_policy_finding_excludes_a_candidate() {
    let mut review = passing_review(Some(92.0));
    review.reports[0].issues.push(ReviewIssue {
        provider: "test-integrity".to_string(),
        fingerprint: "abc".to_string(),
        rule_id: "existing-test-removed".to_string(),
        category: IssueCategory::TestIntegrity,
        severity: IssueSeverity::Blocker,
        file: Some("tests/test_invoice.py".to_string()),
        line: None,
        column: None,
        message: "an existing test was removed".to_string(),
        help_reference: None,
        is_new: true,
    });
    let outcome = evaluate_eligibility(&eligible_input(), &review);
    assert!(!outcome.eligible);
    assert!(outcome
        .reasons
        .iter()
        .any(|reason| matches!(reason, ExclusionReason::BlockerPolicyFinding { .. })));
}

#[test]
fn a_patch_that_does_not_apply_excludes_a_candidate() {
    let mut input = eligible_input();
    input.diff_applies = Err("the patch does not apply".to_string());
    let outcome = evaluate_eligibility(&input, &passing_review(Some(92.0)));
    assert!(!outcome.eligible);
    assert!(outcome
        .reasons
        .iter()
        .any(|reason| matches!(reason, ExclusionReason::DiffDoesNotApply { .. })));
}

#[test]
fn missing_coverage_ranks_worse_than_any_measured_coverage() {
    assert!(CoverageRank::from_percent(Some(0.0)) < CoverageRank::Missing);
    assert!(CoverageRank::from_percent(Some(100.0)) < CoverageRank::from_percent(Some(99.0)));
    assert!(CoverageRank::from_percent(Some(80.0)) < CoverageRank::Missing);
    assert_eq!(CoverageRank::from_percent(Some(88.5)).percent(), Some(88.5));
    assert_eq!(CoverageRank::Missing.percent(), None);
}

#[test]
fn the_score_tuple_orders_by_the_documented_priority() {
    let review = passing_review(Some(90.0));
    let tests = TestEvidence::default();
    let smaller = ScoreTuple::build(&candidate(1, 4, 0, 1_000), &review, &tests);
    let larger = ScoreTuple::build(&candidate(2, 40, 0, 1_000), &review, &tests);
    assert!(smaller < larger, "fewer changed lines must rank first");

    let repaired = ScoreTuple::build(&candidate(2, 4, 2, 1_000), &review, &tests);
    let unrepaired = ScoreTuple::build(&candidate(3, 4, 0, 1_000), &review, &tests);
    assert!(unrepaired < repaired, "fewer repairs must rank first");

    let fast = ScoreTuple::build(&candidate(1, 4, 0, 500), &review, &tests);
    let slow = ScoreTuple::build(&candidate(2, 4, 0, 5_000), &review, &tests);
    assert!(fast < slow, "a shorter gate duration must rank first");
}

#[test]
fn blocker_counts_dominate_every_later_component() {
    let tests = TestEvidence::default();
    let mut blocking = passing_review(Some(99.0));
    blocking.reports[0].issues.push(ReviewIssue {
        provider: "local".to_string(),
        fingerprint: "f".to_string(),
        rule_id: "blocker".to_string(),
        category: IssueCategory::Security,
        severity: IssueSeverity::Blocker,
        file: None,
        line: None,
        column: None,
        message: "a blocker".to_string(),
        help_reference: None,
        is_new: true,
    });
    let with_blocker = ScoreTuple::build(&candidate(1, 1, 0, 1), &blocking, &tests);
    let clean = ScoreTuple::build(
        &candidate(2, 9_999, 9, 999_999),
        &passing_review(Some(10.0)),
        &tests,
    );
    assert!(clean < with_blocker);
}

#[test]
fn the_candidate_identifier_is_the_final_tie_break() {
    let review = passing_review(Some(90.0));
    let tests = TestEvidence::default();
    let first = ScoreTuple::build(&candidate(1, 10, 1, 2_000), &review, &tests);
    let second = ScoreTuple::build(&candidate(2, 10, 1, 2_000), &review, &tests);
    assert!(first < second);
    assert_ne!(first.candidate_id, second.candidate_id);
}

#[test]
fn ranking_is_deterministic_and_assigns_ranks_only_to_eligible_candidates() {
    let review = passing_review(Some(90.0));
    let tests = TestEvidence::default();
    let entries = vec![
        RankedCandidate {
            candidate_id: candidate(3, 24, 2, 9_000).id,
            eligible: false,
            score: None,
            exclusion_reasons: vec![ExclusionReason::EmptyDiff],
            rank: None,
        },
        RankedCandidate {
            candidate_id: candidate(2, 19, 1, 4_000).id,
            eligible: true,
            score: Some(ScoreTuple::build(
                &candidate(2, 19, 1, 4_000),
                &review,
                &tests,
            )),
            exclusion_reasons: Vec::new(),
            rank: None,
        },
        RankedCandidate {
            candidate_id: candidate(1, 4, 0, 3_000).id,
            eligible: true,
            score: Some(ScoreTuple::build(
                &candidate(1, 4, 0, 3_000),
                &review,
                &tests,
            )),
            exclusion_reasons: Vec::new(),
            rank: None,
        },
    ];
    let first = rank_candidates(entries.clone());
    let second = rank_candidates(entries);
    assert_eq!(first.winner, second.winner);
    assert_eq!(first.winner, Some(candidate(1, 4, 0, 3_000).id));
    assert_eq!(first.entries[0].rank, Some(1));
    assert_eq!(first.entries[1].rank, Some(2));
    assert_eq!(first.entries[2].rank, None);
    assert!(first
        .rationale
        .iter()
        .any(|line| line.contains("Changed lines")));
}

#[test]
fn ranking_with_no_eligible_candidate_selects_no_winner() {
    let ranking = rank_candidates(vec![RankedCandidate {
        candidate_id: candidate(1, 0, 0, 0).id,
        eligible: false,
        score: None,
        exclusion_reasons: vec![ExclusionReason::EmptyDiff],
        rank: None,
    }]);
    assert!(ranking.winner.is_none());
    assert!(ranking
        .rationale
        .iter()
        .any(|line| line.contains("No candidate was eligible")));
}

#[test]
fn a_plan_with_every_required_heading_is_acceptable() {
    let mut document = String::new();
    for heading in REQUIRED_PLAN_HEADINGS {
        document.push_str(&format!("## {heading}\n\nContent for {heading}.\n\n"));
    }
    let validation = validate_plan_document(&document);
    assert!(validation.is_acceptable());
    assert!(validation.missing_headings.is_empty());
    assert!(validation.empty_sections.is_empty());
}

#[test]
fn a_plan_missing_a_heading_is_reported() {
    let mut document = String::new();
    for heading in REQUIRED_PLAN_HEADINGS.iter().skip(1) {
        document.push_str(&format!("## {heading}\n\nContent.\n\n"));
    }
    let validation = validate_plan_document(&document);
    assert!(!validation.is_acceptable());
    assert_eq!(validation.missing_headings, vec![REQUIRED_PLAN_HEADINGS[0]]);
}

#[test]
fn the_expected_file_list_is_extracted_from_the_plan() {
    let document = "## Files expected to change\n\n- `src/invoice.py`\n- src/rounding.py\n";
    let validation = validate_plan_document(document);
    assert_eq!(
        validation.expected_files,
        vec!["src/invoice.py".to_string(), "src/rounding.py".to_string()]
    );
}

#[test]
fn a_heading_inside_a_fenced_block_is_not_treated_as_a_section() {
    let document =
        "## Assumptions\n\n```\n## Proposed design\n```\n\nThe fenced text is content.\n";
    let validation = validate_plan_document(document);
    assert!(validation
        .missing_headings
        .contains(&"Proposed design".to_string()));
}

#[test]
fn an_approval_is_invalidated_by_any_later_plan_edit() {
    let original = ContentDigest::of_str("plan version one");
    let edited = ContentDigest::of_str("plan version two");
    let history = PlanHistory {
        versions: vec![PlanVersion {
            version: 1,
            hash: original.clone(),
            created_at: moment(),
            author: PlanAuthor::Agent,
            revision_note: None,
            byte_length: 16,
        }],
        approval: Some(PlanApproval {
            id: ApprovalId::from_uuid(Uuid::from_u128(9)),
            decision: ApprovalDecision::Approved,
            plan_version: 1,
            plan_hash: original.clone(),
            decided_at: moment(),
            local_user: "operator".to_string(),
            note: None,
        }),
    };
    assert!(history.is_approved());
    assert_eq!(history.approved_hash(), Some(&original));

    let mut edited_history = history.clone();
    edited_history.versions.push(PlanVersion {
        version: 2,
        hash: edited,
        created_at: moment(),
        author: PlanAuthor::Human,
        revision_note: None,
        byte_length: 18,
    });
    assert!(
        !edited_history.is_approved(),
        "editing the plan must invalidate the approval"
    );
}

#[test]
fn a_rejected_or_revised_plan_is_not_approved() {
    let hash = ContentDigest::of_str("plan");
    for decision in [
        ApprovalDecision::Rejected,
        ApprovalDecision::RevisionRequested,
    ] {
        let history = PlanHistory {
            versions: vec![PlanVersion {
                version: 1,
                hash: hash.clone(),
                created_at: moment(),
                author: PlanAuthor::Agent,
                revision_note: None,
                byte_length: 4,
            }],
            approval: Some(PlanApproval {
                id: ApprovalId::from_uuid(Uuid::from_u128(1)),
                decision,
                plan_version: 1,
                plan_hash: hash.clone(),
                decided_at: moment(),
                local_user: "operator".to_string(),
                note: None,
            }),
        };
        assert!(!history.is_approved());
    }
}

#[test]
fn a_repair_budget_stops_after_two_identical_failure_fingerprints() {
    let mut record = candidate(1, 10, 0, 0);
    assert!(record.has_repair_budget());
    record.observe_failure_fingerprint("same".to_string());
    assert!(record.has_repair_budget());
    record.observe_failure_fingerprint("same".to_string());
    assert!(record.has_repair_budget());
    record.observe_failure_fingerprint("same".to_string());
    assert!(
        !record.has_repair_budget(),
        "an unchanged failure fingerprint must exhaust the budget early"
    );
}

#[test]
fn a_changed_fingerprint_resets_the_early_exhaustion_counter() {
    let mut record = candidate(1, 10, 0, 0);
    record.observe_failure_fingerprint("first".to_string());
    record.observe_failure_fingerprint("first".to_string());
    record.observe_failure_fingerprint("second".to_string());
    assert_eq!(record.repeated_fingerprint_count, 0);
    assert!(record.has_repair_budget());
}
