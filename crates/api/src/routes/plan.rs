use axum::extract::{Path, State};
use axum::Json;
use heikas_domain::plan::PlanHistory;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::routes::runs::{parse_run_id, AcknowledgementResponse};
use crate::state::ApiState;

const MAXIMUM_PLAN_BYTES: usize = 524_288;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PlanResponse {
    pub version: Option<u32>,
    pub markdown: Option<String>,
    pub history: PlanHistory,
    pub approved: bool,
    pub validation: Option<heikas_domain::plan::PlanValidation>,
    pub candidate_work_started: bool,
}

pub async fn read_plan(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<PlanResponse>> {
    let run_id = parse_run_id(&run_id)?;
    let projection = state.runtime.service.projection(run_id).await?;
    let current = state.runtime.service.plan_markdown(run_id).await?;
    let validation = current
        .as_ref()
        .map(|(_, markdown)| heikas_domain::plan::validate_plan_document(markdown));
    Ok(Json(PlanResponse {
        version: current.as_ref().map(|(version, _)| *version),
        markdown: current.map(|(_, markdown)| markdown),
        approved: projection.plan.is_approved(),
        candidate_work_started: projection.candidates.iter().any(|candidate| {
            candidate.status != heikas_domain::candidate::CandidateStatus::Pending
        }),
        history: projection.plan,
        validation,
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdatePlanRequest {
    pub markdown: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UpdatePlanResponse {
    pub version: u32,
    pub approval_invalidated: bool,
}

pub async fn update_plan(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<UpdatePlanRequest>,
) -> ApiResult<Json<UpdatePlanResponse>> {
    let run_id = parse_run_id(&run_id)?;
    if request.markdown.trim().is_empty() {
        return Err(ApiError::bad_request("the plan must not be empty"));
    }
    if request.markdown.len() > MAXIMUM_PLAN_BYTES {
        return Err(ApiError::bad_request(format!(
            "the plan exceeds the {MAXIMUM_PLAN_BYTES} byte limit"
        )));
    }
    let previous = state.runtime.service.projection(run_id).await?;
    let approved_before = previous.plan.is_approved();
    let version = state
        .runtime
        .service
        .update_plan(run_id, &request.markdown)
        .await?;
    Ok(Json(UpdatePlanResponse {
        version,
        approval_invalidated: approved_before,
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApprovePlanRequest {
    pub markdown: Option<String>,
    pub note: Option<String>,
}

pub async fn approve_plan(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<ApprovePlanRequest>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    state
        .runtime
        .service
        .approve_plan(run_id, request.markdown, request.note)
        .await?;
    state.spawn_dispatch(run_id).await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: "the plan was approved and candidate work was queued".to_string(),
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RevisePlanRequest {
    pub note: String,
}

pub async fn revise_plan(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<RevisePlanRequest>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    if request.note.trim().is_empty() {
        return Err(ApiError::bad_request(
            "a revision request must explain what should change",
        ));
    }
    state
        .runtime
        .service
        .revise_plan(run_id, request.note)
        .await?;
    state.spawn_dispatch(run_id).await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: "a new plan version was requested".to_string(),
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RejectPlanRequest {
    pub reason: Option<String>,
}

pub async fn reject_plan(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<RejectPlanRequest>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    state
        .runtime
        .service
        .reject_plan(run_id, request.reason)
        .await?;
    state.spawn_dispatch(run_id).await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: "the plan was rejected and the run will end without source changes".to_string(),
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApproveCommitRequest {
    pub note: Option<String>,
}

pub async fn approve_commit(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<ApproveCommitRequest>,
) -> ApiResult<Json<AcknowledgementResponse>> {
    let run_id = parse_run_id(&run_id)?;
    state
        .runtime
        .service
        .approve_commit(run_id, request.note)
        .await?;
    state.spawn_dispatch(run_id).await?;
    Ok(Json(AcknowledgementResponse {
        accepted: true,
        detail: "the commit was approved and the run will finish".to_string(),
    }))
}

pub async fn noop() -> ApiResult<Json<AcknowledgementResponse>> {
    Err(ApiError::not_found("this route is not implemented"))
}
