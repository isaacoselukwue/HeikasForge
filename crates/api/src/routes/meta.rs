use std::path::PathBuf;

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use heikas_application::model::doctor::DoctorReport;
use heikas_domain::graph::{graph_edges, NodeId};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::session::{BOOTSTRAP_HEADER, CSRF_COOKIE, SESSION_COOKIE};
use crate::state::ApiState;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub demonstration_mode: bool,
    pub active_dispatches: usize,
}

pub async fn health(State(state): State<ApiState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ready",
        version: env!("CARGO_PKG_VERSION"),
        demonstration_mode: state.demonstration_mode,
        active_dispatches: state.active_dispatches().await,
    })
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionRequest {
    pub bootstrap_token: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SessionResponse {
    pub csrf_token: String,
    pub demonstration_mode: bool,
}

pub async fn create_session(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Option<Json<SessionRequest>>,
) -> ApiResult<Response> {
    let presented = headers
        .get(BOOTSTRAP_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| payload.and_then(|Json(body)| body.bootstrap_token))
        .ok_or_else(|| {
            ApiError::unauthorised("a bootstrap token is required to establish a session")
        })?;

    let session = state
        .sessions
        .exchange(&presented)
        .await
        .ok_or_else(|| ApiError::unauthorised("the bootstrap token was not accepted"))?;

    let mut response = Json(SessionResponse {
        csrf_token: session.csrf_token.clone(),
        demonstration_mode: state.demonstration_mode,
    })
    .into_response();

    let session_cookie = format!(
        "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200",
        session.id
    );
    let csrf_cookie = format!(
        "{CSRF_COOKIE}={}; Path=/; SameSite=Strict; Max-Age=43200",
        session.csrf_token
    );
    let headers = response.headers_mut();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "cookie", "the session cookie could not be encoded"))?,
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "cookie", "the token cookie could not be encoded"))?,
    );
    Ok(response)
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GraphNodeDefinition {
    pub id: String,
    pub label: String,
    pub scope: String,
    pub class: String,
    pub read_only: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GraphDefinition {
    pub nodes: Vec<GraphNodeDefinition>,
    pub edges: Vec<GraphEdgeDefinition>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GraphEdgeDefinition {
    pub from: String,
    pub to: String,
    pub label: String,
}

pub async fn graph_definition() -> Json<GraphDefinition> {
    Json(GraphDefinition {
        nodes: NodeId::ALL
            .into_iter()
            .map(|node| GraphNodeDefinition {
                id: node.as_str().to_string(),
                label: node.label().to_string(),
                scope: match node.scope() {
                    heikas_domain::graph::NodeScope::Run => "run".to_string(),
                    heikas_domain::graph::NodeScope::Candidate => "candidate".to_string(),
                },
                class: format!("{:?}", node.class()).to_lowercase(),
                read_only: node.is_read_only(),
            })
            .collect(),
        edges: graph_edges()
            .into_iter()
            .map(|edge| GraphEdgeDefinition {
                from: edge.from.as_str().to_string(),
                to: edge.to.as_str().to_string(),
                label: edge.label.to_string(),
            })
            .collect(),
    })
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConfigurationResponse {
    pub heikas_home: String,
    pub user_configuration_path: String,
    pub demonstration_mode: bool,
    pub default_candidate_count: u8,
    pub maximum_candidate_count: u8,
    pub agent_drivers: Vec<AgentDriverDescription>,
    pub quality_profiles: Vec<String>,
    pub commit_policies: Vec<String>,
    pub recent_repositories: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AgentDriverDescription {
    pub id: String,
    pub label: String,
    pub requires_paid_account: bool,
    pub demonstration_only: bool,
}

pub async fn configuration(State(state): State<ApiState>) -> ApiResult<Json<ConfigurationResponse>> {
    let facts = state.runtime.host.facts().await?;
    let user_configuration = state.runtime.configuration.user_configuration_path().await?;
    let summaries = state.runtime.service.list_runs().await?;
    let mut recent_repositories: Vec<String> = Vec::new();
    for summary in summaries.iter().take(30) {
        if !recent_repositories.contains(&summary.repository_path) {
            recent_repositories.push(summary.repository_path.clone());
        }
    }
    Ok(Json(ConfigurationResponse {
        heikas_home: facts.heikas_home.display().to_string(),
        user_configuration_path: user_configuration.display().to_string(),
        demonstration_mode: state.demonstration_mode,
        default_candidate_count: heikas_domain::budget::DEFAULT_CANDIDATES,
        maximum_candidate_count: heikas_domain::budget::MAXIMUM_CANDIDATES,
        agent_drivers: heikas_application::configuration::AgentDriverKind::ALL
            .into_iter()
            .map(|kind| AgentDriverDescription {
                id: kind.as_str().to_string(),
                label: kind.label().to_string(),
                requires_paid_account: kind.requires_paid_account(),
                demonstration_only: kind.is_demonstration_only(),
            })
            .collect(),
        quality_profiles: vec!["standard".to_string(), "strict".to_string()],
        commit_policies: vec![
            "manual".to_string(),
            "automatic".to_string(),
            "none".to_string(),
        ],
        recent_repositories,
    }))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DoctorRequest {
    pub repository_path: Option<String>,
}

pub async fn doctor(
    State(state): State<ApiState>,
    Json(request): Json<DoctorRequest>,
) -> ApiResult<Json<DoctorReport>> {
    let path = request.repository_path.map(PathBuf::from);
    let report = state
        .runtime
        .service
        .diagnose(path.as_deref())
        .await?;
    Ok(Json(report))
}
