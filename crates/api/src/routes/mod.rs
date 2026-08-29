pub mod artifacts;
pub mod meta;
pub mod plan;
pub mod runs;
pub mod stream;

use axum::routing::{any, get, post, put};
use axum::Router;

use crate::error::ApiError;
use crate::state::ApiState;

async fn unknown_api_route(uri: axum::http::Uri) -> ApiError {
    ApiError::not_found(format!("`{}` is not an endpoint of this API", uri.path()))
        .with_remedy("Check the endpoint list in the documentation screen.")
}

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/health", get(meta::health))
        .route("/api/v1/session", post(meta::create_session))
        .route("/api/v1/graph", get(meta::graph_definition))
        .route("/api/v1/config", get(meta::configuration))
        .route("/api/v1/doctor", post(meta::doctor))
        .route("/api/v1/runs", get(runs::list_runs).post(runs::create_run))
        .route("/api/v1/runs/{run_id}", get(runs::run_detail))
        .route("/api/v1/runs/{run_id}/resume", post(runs::resume_run))
        .route("/api/v1/runs/{run_id}/cancel", post(runs::cancel_run))
        .route("/api/v1/runs/{run_id}/cleanup", post(runs::cleanup_run))
        .route(
            "/api/v1/runs/{run_id}/plan",
            get(plan::read_plan).put(plan::update_plan),
        )
        .route(
            "/api/v1/runs/{run_id}/plan/approve",
            post(plan::approve_plan),
        )
        .route("/api/v1/runs/{run_id}/plan/revise", post(plan::revise_plan))
        .route("/api/v1/runs/{run_id}/plan/reject", post(plan::reject_plan))
        .route(
            "/api/v1/runs/{run_id}/commit/approve",
            post(plan::approve_commit),
        )
        .route("/api/v1/runs/{run_id}/candidates", get(runs::candidates))
        .route(
            "/api/v1/runs/{run_id}/candidates/{candidate_id}/diff",
            get(artifacts::candidate_diff),
        )
        .route(
            "/api/v1/runs/{run_id}/integration/diff",
            get(artifacts::integration_diff),
        )
        .route(
            "/api/v1/runs/{run_id}/artifacts/{artifact_id}",
            get(artifacts::artifact),
        )
        .route("/api/v1/runs/{run_id}/events", get(runs::events))
        .route("/api/v1/runs/{run_id}/timeline", get(runs::timeline))
        .route("/api/v1/runs/{run_id}/logs", get(runs::logs))
        .route("/api/v1/runs/{run_id}/stream", get(stream::stream_events))
        .route("/api/v1/runs/{run_id}/export", post(runs::export_run))
        .route("/api/v1/plan/{run_id}/versions/{version}", put(plan::noop))
        .route("/api/{*unmatched}", any(unknown_api_route))
}
