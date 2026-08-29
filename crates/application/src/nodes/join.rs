use heikas_domain::candidate::CandidateStatus;
use heikas_domain::event::EventPayload;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::CandidateId;
use heikas_domain::node::StatePatch;
use heikas_domain::review::AggregatedReview;
use heikas_domain::run::RunStatus;
use heikas_domain::score::{
    evaluate_eligibility, rank_candidates, EligibilityInput, ExclusionReason, RankedCandidate,
    ScoreTuple,
};
use heikas_domain::test_evidence::TestEvidence;
use serde_json::json;

use crate::engine::context::{NodeContext, NodeOutput};
use crate::error::ApplicationResult;
use crate::model::attempt::AttemptEvidence;
use crate::nodes::support::{baseline, integration_worktree};

pub async fn execute(context: &NodeContext<'_>) -> ApplicationResult<NodeOutput> {
    let services = context.services();
    let configuration = context.configuration();
    let baseline_commit = baseline(context)?;
    let minimum_coverage = configuration.minimum_line_coverage();

    let mut entries = Vec::new();
    let mut events = Vec::new();

    for record in &context.projection.candidates {
        let tests = services
            .store
            .read_test_evidence(context.run.run_id, Some(&record.id))
            .await?
            .unwrap_or_default();
        let review = services
            .store
            .read_review(context.run.run_id, Some(&record.id))
            .await?
            .unwrap_or_default();
        let patch = services
            .store
            .read_diff(context.run.run_id, &record.id)
            .await
            .unwrap_or_default();

        let diff_applies = if patch.is_empty() {
            Ok(())
        } else {
            check_patch(context, &patch).await?
        };

        let coverage = review
            .line_coverage_percent()
            .or(tests.line_coverage_percent);

        let input = EligibilityInput {
            candidate_status: record.status,
            required_tests_passed: tests.all_required_passed(),
            failed_test_commands: tests.failed_required(),
            missing_test_commands: tests.missing_required(),
            diff_is_empty: patch.is_empty(),
            change_required: true,
            diff_applies,
            coverage_percent: coverage,
            minimum_line_coverage: minimum_coverage,
            repairs_used: record.repairs_used,
            repair_budget: record.repair_budget,
            repair_budget_exhausted: record.repairs_used >= record.repair_budget
                && !tests.all_required_passed(),
            time_budget_exceeded: None,
        };

        let outcome = evaluate_eligibility(&input, &review);
        let score = build_score(record, &review, &tests);

        if outcome.eligible {
            events.push(EventPayload::CandidateScored {
                candidate_id: record.id.clone(),
                score: score.clone(),
            });
        } else {
            events.push(EventPayload::CandidateExcluded {
                candidate_id: record.id.clone(),
                reasons: outcome.reasons.clone(),
            });
        }

        entries.push(RankedCandidate {
            candidate_id: record.id.clone(),
            eligible: outcome.eligible,
            score: if outcome.eligible { Some(score) } else { None },
            exclusion_reasons: outcome.reasons,
            rank: None,
        });
    }

    let ranking = rank_candidates(entries);
    services
        .store
        .write_ranking(context.run.run_id, &ranking)
        .await?;
    events.push(EventPayload::RankingComputed {
        ranking: ranking.clone(),
    });

    let input = json!({
        "candidates": context.projection.candidates.len(),
        "minimum_line_coverage": minimum_coverage,
        "baseline_commit": baseline_commit.as_str(),
    });
    let evidence = AttemptEvidence::with_input(input)
        .with_streams(serde_json::to_vec_pretty(&ranking)?, Vec::new());

    let metrics = json!({
        "eligible": ranking.entries.iter().filter(|entry| entry.eligible).count(),
        "ineligible": ranking.entries.iter().filter(|entry| !entry.eligible).count(),
        "winner": ranking.winner.as_ref().map(CandidateId::to_string),
        "rationale": ranking.rationale,
    });

    match ranking.winner.clone() {
        Some(winner) => {
            events.push(EventPayload::WinnerSelected {
                candidate_id: winner.clone(),
                rank: 1,
            });
            Ok(NodeOutput::succeeded(Some(NodeId::IntegrateWinner))
                .with_events(events)
                .with_patch(StatePatch {
                    winner: Some(winner),
                    ..StatePatch::default()
                })
                .with_metrics(metrics)
                .with_evidence(evidence))
        }
        None => Ok(NodeOutput::succeeded(None)
            .with_events(events)
            .with_patch(StatePatch {
                run_status: Some(RunStatus::Exhausted),
                ..StatePatch::default()
            })
            .with_metrics(metrics)
            .with_evidence(evidence)
            .with_warning("no candidate satisfied every required gate")),
    }
}

fn build_score(
    record: &heikas_domain::candidate::CandidateRecord,
    review: &AggregatedReview,
    tests: &TestEvidence,
) -> ScoreTuple {
    ScoreTuple::build(record, review, tests)
}

async fn check_patch(
    context: &NodeContext<'_>,
    patch: &[u8],
) -> ApplicationResult<Result<(), String>> {
    let worktree = integration_worktree(context).await?;
    if !worktree.exists() {
        return Ok(Ok(()));
    }
    context
        .services()
        .git
        .check_patch_applies(&worktree, patch)
        .await
}

pub fn exclusion_summaries(reasons: &[ExclusionReason]) -> Vec<String> {
    reasons.iter().map(ExclusionReason::summary).collect()
}

pub fn terminal_candidate_status(eligible: bool) -> CandidateStatus {
    if eligible {
        CandidateStatus::Eligible
    } else {
        CandidateStatus::Ineligible
    }
}
