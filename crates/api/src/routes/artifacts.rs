use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use heikas_domain::identity::{CandidateId, ContentDigest};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::routes::runs::parse_run_id;
use crate::state::ApiState;

const MAXIMUM_INLINE_BYTES: u64 = 4_194_304;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RangeQuery {
    pub offset: Option<u64>,
    pub length: Option<u64>,
}

pub async fn candidate_diff(
    State(state): State<ApiState>,
    Path((run_id, candidate_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let run_id = parse_run_id(&run_id)?;
    let candidate = CandidateId::from_str(&candidate_id).map_err(|_| {
        ApiError::bad_request(format!(
            "`{candidate_id}` is not a valid candidate identifier"
        ))
    })?;
    let bytes = state
        .runtime
        .service
        .candidate_diff(run_id, &candidate)
        .await?;
    Ok(text_response(bytes, "text/x-diff"))
}

pub async fn integration_diff(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
) -> ApiResult<Response> {
    let run_id = parse_run_id(&run_id)?;
    let bytes = state.runtime.service.integration_diff(run_id).await?;
    Ok(text_response(bytes, "text/x-diff"))
}

pub async fn artifact(
    State(state): State<ApiState>,
    Path((run_id, artifact_id)): Path<(String, String)>,
    Query(range): Query<RangeQuery>,
) -> ApiResult<Response> {
    let run_id = parse_run_id(&run_id)?;
    let digest = ContentDigest::from_str(&artifact_id).map_err(|_| {
        ApiError::bad_request(format!(
            "`{artifact_id}` is not a valid artefact identifier"
        ))
    })?;
    let bytes = match (range.offset, range.length) {
        (Some(offset), Some(length)) => {
            if length > MAXIMUM_INLINE_BYTES {
                return Err(ApiError::bad_request(format!(
                    "a range request may not exceed {MAXIMUM_INLINE_BYTES} bytes"
                )));
            }
            state
                .runtime
                .service
                .artifact_range(run_id, &digest, offset, length)
                .await?
        }
        _ => {
            let bytes = state.runtime.service.artifact(run_id, &digest).await?;
            if bytes.len() as u64 > MAXIMUM_INLINE_BYTES {
                return Err(ApiError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "artifact_too_large",
                    format!(
                        "the artefact is {} bytes, request a range instead",
                        bytes.len()
                    ),
                ));
            }
            bytes
        }
    };
    Ok(text_response(bytes, "text/plain; charset=utf-8"))
}

fn text_response(bytes: Vec<u8>, media_type: &str) -> Response {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(media_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (StatusCode::OK, headers, bytes).into_response()
}
