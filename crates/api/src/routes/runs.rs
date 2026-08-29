use std::path::PathBuf;
use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::Json;
use heikas_application::model::detail::{EventPage, RunDetail};
use heikas_application::model::observability::LogPage;
use heikas_application::model::request::{CancelRequest, CreateRunRequest, ExportRequest};
use heikas_application::model::run_summary::{CandidateView, RunSummary, TimelineEntry};
use heikas_domain::identity::RunId;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::ApiState;

const MAXIMUM_TASK_BYTES: usize = 131_072;

pub fn parse_run_id(value: &str) -> ApiResult<RunId> {
    RunId::from_str(value)
        .map_err(|_| ApiError::bad_request(format!("`{value}` is not a valid run identifier")))
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RunListResponse {
    pub runs: Vec<RunSummary>,
}

pub async fn list_runs(State(state): State<ApiState>) -> ApiResult<Json<RunListResponse>> {
    Ok(Json(RunListResponse {
        runs: state.runtime.service.list_runs().await?,
    }))
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreateRunResponse {
    pub run_id: RunId,
}

pub async fn create_run(
    State(state): State<ApiState>,
    Json(mut request): Json<CreateRunRequest>,
) -> ApiResult<Json<CreateRunResponse>> {
    if request.task_markdown.trim().is_empty() {
        return Err(ApiError::bad_request("the task description must not be empty"));
    }
    if request.task_markdown.len() > MAXIMUM_TASK_BYTES {
        return Err(ApiError::bad_request(format!(
            "the task description exceeds the {MAXIMUM_TASK_BYTES} byte limit"
        )));
    }
    if state.demonstration_mode {
        request.demonstration_mode = true;
    }
    let run_id = state.runtime.service.create_run(request).await?;
    state.spawn_dispatch(run_id).await?;
    Ok(Json(CreateRunResponse { run_id }))
}

pub async fn run_detail(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<RunDetail>> {
    let run_id = parse_run_id(&run_id)?;
    Ok(Json(state.runtime.service.run_detail(run_id).await?))
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AcknowledgementResponse {
    pub accepted: bool,
    pub detail: String,
}

pub async fn resume_run(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    let projection = state.runtime.service.projection(run_id).await?;
    if projection.status.is_terminal() {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "run_terminal",
            format!("the run is already {}", projection.status.label()),
        ));
    }
    state.spawn_dispatch(run_id).await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: "the run was queued for dispatch".to_string(),
    }))
}

pub async fn cancel_run(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<CancelRequest>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    state
        .runtime
        .service
        .cancel(run_id, request.reason)
        .await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: "cancellation was recorded and propagated".to_string(),
    }))
}

pub async fn cleanup_run(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    let removed = state.runtime.service.cleanup(run_id).await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: format!("{} worktree directories were removed", removed.len()),
    }))
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CandidateListResponse {
    pub candidates: Vec<CandidateView>,
}

pub async fn candidates(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<CandidateListResponse>> {
    let run_id = parse_run_id(&run_id)?;
    Ok(Json(CandidateListResponse {
        candidates: state.runtime.service.candidates(run_id).await?,
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EventQuery {
    pub after: Option<u64>,
    pub limit: Option<usize>,
}

pub async fn events(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Query(query): Query<EventQuery>,
) -> ApiResult<Json<EventPage>> {
    let run_id = parse_run_id(&run_id)?;
    let limit = query.limit.unwrap_or(500).clamp(1, 2_000);
    Ok(Json(
        state
            .runtime
            .service
            .events(run_id, query.after.unwrap_or(0), limit)
            .await?,
    ))
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TimelineResponse {
    pub entries: Vec<TimelineEntry>,
}

pub async fn timeline(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<TimelineResponse>> {
    let run_id = parse_run_id(&run_id)?;
    Ok(Json(TimelineResponse {
        entries: state.runtime.service.timeline(run_id).await?,
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LogQuery {
    pub offset: Option<u64>,
    pub limit: Option<usize>,
}

pub async fn logs(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Query(query): Query<LogQuery>,
) -> ApiResult<Json<LogPage>> {
    let run_id = parse_run_id(&run_id)?;
    let limit = query.limit.unwrap_or(500).clamp(1, 2_000);
    let offset = query.offset.unwrap_or(0);
    let reader = state.runtime.log_reader();
    let records = reader.read(run_id, offset, limit).await?;
    let total = reader.count(run_id).await?;
    Ok(Json(LogPage {
        run_id,
        offset,
        total,
        records,
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportBody {
    pub output_path: Option<String>,
    pub include_worktrees: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ExportResponse {
    pub archive_path: String,
    pub byte_length: u64,
    pub entry_count: u64,
    pub redacted: bool,
}

pub async fn export_run(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(body): Json<ExportBody>,
) -> ApiResult<Json<ExportResponse>> {
    let run_id = parse_run_id(&run_id)?;
    let output_path = match body.output_path {
        Some(path) => PathBuf::from(path),
        None => state.runtime.layout.exports_directory(run_id),
    };
    let outcome = state
        .runtime
        .service
        .export(
            run_id,
            ExportRequest {
                output_path,
                include_worktrees: body.include_worktrees.unwrap_or(false),
            },
        )
        .await?;
    Ok(Json(ExportResponse {
        archive_path: outcome.archive_path.display().to_string(),
        byte_length: outcome.byte_length,
        entry_count: outcome.entry_count,
        redacted: outcome.redacted,
    }))
}
