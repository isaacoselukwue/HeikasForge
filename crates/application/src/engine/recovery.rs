use heikas_domain::candidate::CandidateStatus;
use heikas_domain::event::EventPayload;
use heikas_domain::identity::RunId;
use heikas_domain::run::RunStatus;
use heikas_domain::state::{replay_from, RunProjection};
use tracing::{info, warn};

use crate::engine::services::EngineServices;
use crate::error::{ApplicationError, ApplicationResult};

pub async fn recover(services: &EngineServices, run_id: RunId) -> ApplicationResult<RunProjection> {
    let verification = services.store.verify_chain(run_id).await?;
    if verification.quarantined_partial_record {
        warn!(
            run_id = %run_id,
            "a partially written event record was quarantined during recovery"
        );
    }

    let stored = services.store.load(run_id).await?;
    let mut projection = match stored {
        Some(projection) if projection.last_event_sequence <= verification.last_sequence => projection,
        Some(_) => {
            return Err(ApplicationError::CorruptEventLog {
                run: run_id,
                detail: "the stored projection is newer than the durable event log".to_string(),
            })
        }
        None => {
            let genesis_time = services.clock.now();
            RunProjection::genesis(run_id, genesis_time)
        }
    };

    let pending = services
        .store
        .read_after(run_id, projection.last_event_sequence)
        .await?;
    let replayed = replay_from(&mut projection, &pending)?;
    if replayed > 0 {
        info!(run_id = %run_id, replayed, "replayed durable events into the projection");
    }

    let open_attempts: Vec<_> = projection
        .open_attempts()
        .into_iter()
        .map(|attempt| {
            (
                attempt.node_id,
                attempt.candidate_id.clone(),
                attempt.attempt,
            )
        })
        .collect();

    if open_attempts.is_empty() && replayed == 0 {
        services.store.store(&projection).await?;
        return Ok(projection);
    }

    if !open_attempts.is_empty() {
        let event = services
            .store
            .append(
                run_id,
                EventPayload::RecoveryStarted {
                    last_applied_sequence: projection.last_event_sequence,
                    interrupted_attempts: open_attempts.len() as u32,
                },
            )
            .await?;
        projection.apply(&event)?;

        let detected_at = services.clock.now();
        for (node_id, candidate_id, attempt) in &open_attempts {
            let event = services
                .store
                .append(
                    run_id,
                    EventPayload::NodeInterrupted {
                        node_id: *node_id,
                        candidate_id: candidate_id.clone(),
                        attempt: *attempt,
                        detected_at,
                    },
                )
                .await?;
            projection.apply(&event)?;
        }

        let candidate_ids: Vec<_> = open_attempts
            .iter()
            .filter_map(|(_, candidate, _)| candidate.clone())
            .collect();
        for candidate_id in candidate_ids {
            let current = projection
                .candidate(&candidate_id)
                .map(|record| record.status);
            if let Some(status) = current {
                if !status.is_terminal() && status != CandidateStatus::Interrupted {
                    if status.transition_to(CandidateStatus::Interrupted).is_ok() {
                        let event = services
                            .store
                            .append(
                                run_id,
                                EventPayload::CandidateStatusChanged {
                                    candidate_id: candidate_id.clone(),
                                    from: status,
                                    to: CandidateStatus::Interrupted,
                                    reason: Some(
                                        "the dispatcher stopped while this candidate was active"
                                            .to_string(),
                                    ),
                                },
                            )
                            .await?;
                        projection.apply(&event)?;
                    }
                }
            }
        }

        let event = services
            .store
            .append(
                run_id,
                EventPayload::RecoveryCompleted {
                    replayed_events: replayed,
                    repaired_projections: vec!["state.json".to_string(), "manifest.json".to_string()],
                },
            )
            .await?;
        projection.apply(&event)?;
    }

    if projection.status == RunStatus::RecoveryRequired && projection.recovery_reason.is_none() {
        projection.recovery_reason = Some("the run requires manual recovery".to_string());
    }

    services.store.store(&projection).await?;
    services
        .store
        .store_metrics(run_id, &projection)
        .await?;
    Ok(projection)
}
