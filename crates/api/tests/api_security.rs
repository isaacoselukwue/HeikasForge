use std::net::{IpAddr, Ipv4Addr};

use heikas_api::server::{start, RunningServer, ServerOptions};
use heikas_infrastructure::{build_runtime, StoreLayout};
use reqwest::header::{HeaderMap, HeaderValue, ORIGIN};
use reqwest::{Client, StatusCode};
use serde_json::json;
use tempfile::TempDir;

struct Harness {
    _home: TempDir,
    server: RunningServer,
    client: Client,
}

impl Harness {
    fn origin(&self) -> String {
        self.server.origin()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin())
    }

    fn bootstrap_token(&self) -> String {
        self.server.state.sessions.bootstrap_token().to_string()
    }

    async fn establish(&self) -> String {
        let response = self
            .client
            .post(self.url("/api/v1/session"))
            .header("x-heikas-bootstrap", self.bootstrap_token())
            .send()
            .await
            .expect("the session request completes");
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("the body decodes");
        body["csrf_token"]
            .as_str()
            .expect("a token is returned")
            .to_string()
    }

    async fn shutdown(self) {
        self.server.shutdown().await;
    }
}

async fn harness() -> Harness {
    let home = TempDir::new().expect("a temporary home");
    let runtime =
        build_runtime(StoreLayout::new(home.path().to_path_buf())).expect("the runtime builds");
    let server = start(runtime, ServerOptions::default())
        .await
        .expect("the server starts");
    let client = Client::builder()
        .cookie_store(true)
        .build()
        .expect("the client builds");
    Harness {
        _home: home,
        server,
        client,
    }
}

fn origin_headers(origin: &str, csrf: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        ORIGIN,
        HeaderValue::from_str(origin).expect("a valid origin"),
    );
    if let Some(token) = csrf {
        headers.insert(
            "x-heikas-csrf",
            HeaderValue::from_str(token).expect("a valid token"),
        );
    }
    headers
}

#[tokio::test]
async fn the_server_binds_to_the_loopback_interface_by_default() {
    let harness = harness().await;
    assert_eq!(
        harness.server.address.ip(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "the interface must not be remotely reachable by default"
    );
    assert!(harness.server.bootstrap_url.contains("#token="));
    harness.shutdown().await;
}

#[tokio::test]
async fn health_is_reachable_without_a_session() {
    let harness = harness().await;
    let response = harness
        .client
        .get(harness.url("/api/v1/health"))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("the body decodes");
    assert_eq!(body["status"], "ready");
    harness.shutdown().await;
}

#[tokio::test]
async fn a_protected_route_is_refused_without_a_session() {
    let harness = harness().await;
    let response = harness
        .client
        .get(harness.url("/api/v1/runs"))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json().await.expect("the body decodes");
    assert_eq!(body["code"], "unauthorised");
    harness.shutdown().await;
}

#[tokio::test]
async fn an_incorrect_bootstrap_token_is_refused() {
    let harness = harness().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/session"))
        .header("x-heikas-bootstrap", "0".repeat(64))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    harness.shutdown().await;
}

#[tokio::test]
async fn a_missing_bootstrap_token_is_refused() {
    let harness = harness().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/session"))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    harness.shutdown().await;
}

#[tokio::test]
async fn a_session_grants_read_access_and_sets_a_same_site_cookie() {
    let harness = harness().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/session"))
        .header("x-heikas-bootstrap", harness.bootstrap_token())
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let cookies: Vec<String> = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap_or_default().to_string())
        .collect();
    let session_cookie = cookies
        .iter()
        .find(|cookie| cookie.starts_with("heikas_session="))
        .expect("a session cookie is issued");
    assert!(session_cookie.contains("HttpOnly"));
    assert!(session_cookie.contains("SameSite=Strict"));
    assert!(cookies
        .iter()
        .any(|cookie| cookie.starts_with("heikas_csrf=")));

    let runs = harness
        .client
        .get(harness.url("/api/v1/runs"))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(runs.status(), StatusCode::OK);
    harness.shutdown().await;
}

#[tokio::test]
async fn a_mutating_request_without_a_cross_site_token_is_refused() {
    let harness = harness().await;
    harness.establish().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/doctor"))
        .headers(origin_headers(&harness.origin(), None))
        .json(&json!({ "repository_path": null }))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    harness.shutdown().await;
}

#[tokio::test]
async fn a_mutating_request_with_a_foreign_origin_is_refused() {
    let harness = harness().await;
    let csrf = harness.establish().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/doctor"))
        .headers(origin_headers("http://evil.invalid", Some(&csrf)))
        .json(&json!({ "repository_path": null }))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    harness.shutdown().await;
}

#[tokio::test]
async fn a_mutating_request_without_an_origin_is_refused() {
    let harness = harness().await;
    let csrf = harness.establish().await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-heikas-csrf",
        HeaderValue::from_str(&csrf).expect("a valid token"),
    );
    let response = harness
        .client
        .post(harness.url("/api/v1/doctor"))
        .headers(headers)
        .json(&json!({ "repository_path": null }))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    harness.shutdown().await;
}

#[tokio::test]
async fn a_correctly_authorised_mutating_request_is_accepted() {
    let harness = harness().await;
    let csrf = harness.establish().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/doctor"))
        .headers(origin_headers(&harness.origin(), Some(&csrf)))
        .json(&json!({ "repository_path": null }))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("the body decodes");
    assert!(body["checks"].as_array().expect("checks").len() > 1);
    harness.shutdown().await;
}

#[tokio::test]
async fn an_invalid_run_identifier_is_rejected_at_the_boundary() {
    let harness = harness().await;
    harness.establish().await;
    for candidate in [
        "not-a-uuid",
        "%2e%2e%2fetc%2fpasswd",
        "0198f5b0",
        "..;/etc/passwd",
    ] {
        let response = harness
            .client
            .get(harness.url(&format!("/api/v1/runs/{candidate}")))
            .send()
            .await
            .expect("the request completes");
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::NOT_FOUND,
            "`{candidate}` must be rejected, received {}",
            response.status()
        );
        let body: serde_json::Value = response
            .json()
            .await
            .expect("a structured error is returned");
        assert!(
            body["code"].is_string(),
            "the boundary must answer with a typed error"
        );
    }
    harness.shutdown().await;
}

#[tokio::test]
async fn an_unknown_api_route_never_serves_the_interface_shell() {
    let harness = harness().await;
    harness.establish().await;
    for path in ["/api/v1/absent", "/api/v2/runs", "/api/anything"] {
        let response = harness
            .client
            .get(harness.url(path))
            .send()
            .await
            .expect("the request completes");
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let status = response.status();
        let text = response.text().await.expect("the body reads");
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "`{path}` must answer with a not found status"
        );
        assert!(
            content_type.contains("application/json"),
            "`{path}` must answer with a typed error"
        );
        assert!(
            !text.contains("id=\"application\""),
            "`{path}` must not return the interface shell"
        );
    }
    harness.shutdown().await;
}

#[tokio::test]
async fn the_static_fallback_never_reads_the_host_filesystem() {
    let harness = harness().await;
    for path in [
        "/etc/passwd",
        "/../../etc/passwd",
        "/%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        "/assets/../../../../etc/passwd",
    ] {
        let response = harness
            .client
            .get(harness.url(path))
            .send()
            .await
            .expect("the request completes");
        let text = response.text().await.expect("the body reads");
        assert!(
            !text.contains("root:x:0:0"),
            "`{path}` must never expose a host file"
        );
        assert!(
            !text.contains("/bin/bash"),
            "`{path}` must never expose a host file"
        );
    }
    harness.shutdown().await;
}

#[tokio::test]
async fn an_invalid_artifact_identifier_is_rejected() {
    let harness = harness().await;
    harness.establish().await;
    let run = uuid::Uuid::now_v7();
    let response = harness
        .client
        .get(harness.url(&format!(
            "/api/v1/runs/{run}/artifacts/..%2f..%2fetc%2fpasswd"
        )))
        .send()
        .await
        .expect("the request completes");
    assert!(
        response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::NOT_FOUND,
        "a traversal attempt must be rejected, received {}",
        response.status()
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn an_oversized_request_body_is_rejected() {
    let harness = harness().await;
    let csrf = harness.establish().await;
    let payload = json!({
        "repository_path": "/tmp",
        "task_markdown": "x".repeat(2_000_000),
        "include_dirty": false,
        "demonstration_mode": false
    });
    let outcome = harness
        .client
        .post(harness.url("/api/v1/runs"))
        .headers(origin_headers(&harness.origin(), Some(&csrf)))
        .json(&payload)
        .send()
        .await;

    match outcome {
        Ok(response) => assert!(
            response.status() == StatusCode::PAYLOAD_TOO_LARGE
                || response.status() == StatusCode::BAD_REQUEST,
            "an oversized body must be rejected, received {}",
            response.status()
        ),
        Err(error) => assert!(
            error.is_request() || error.is_body(),
            "an oversized body must be refused rather than accepted, received {error}"
        ),
    }

    let runs: serde_json::Value = harness
        .client
        .get(harness.url("/api/v1/runs"))
        .send()
        .await
        .expect("the listing completes")
        .json()
        .await
        .expect("the listing decodes");
    assert_eq!(
        runs["runs"].as_array().expect("a run array").len(),
        0,
        "an oversized body must never create a run"
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn an_empty_task_is_rejected_with_an_actionable_message() {
    let harness = harness().await;
    let csrf = harness.establish().await;
    let response = harness
        .client
        .post(harness.url("/api/v1/runs"))
        .headers(origin_headers(&harness.origin(), Some(&csrf)))
        .json(&json!({
            "repository_path": "/tmp",
            "task_markdown": "   ",
            "include_dirty": false,
            "demonstration_mode": false
        }))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.expect("the body decodes");
    assert!(body["message"]
        .as_str()
        .expect("a message")
        .contains("must not be empty"));
    harness.shutdown().await;
}

#[tokio::test]
async fn repeated_mutating_requests_are_rate_limited() {
    let harness = harness().await;
    let csrf = harness.establish().await;
    let mut limited = false;
    for _ in 0..60 {
        let response = harness
            .client
            .post(harness.url("/api/v1/runs"))
            .headers(origin_headers(&harness.origin(), Some(&csrf)))
            .json(&json!({
                "repository_path": "/tmp",
                "task_markdown": "",
                "include_dirty": false,
                "demonstration_mode": false
            }))
            .send()
            .await
            .expect("the request completes");
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
    }
    assert!(limited, "state-changing requests must be rate limited");
    harness.shutdown().await;
}

#[tokio::test]
async fn responses_carry_the_documented_security_headers() {
    let harness = harness().await;
    let response = harness
        .client
        .get(harness.url("/api/v1/health"))
        .send()
        .await
        .expect("the request completes");
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        headers
            .get("referrer-policy")
            .and_then(|value| value.to_str().ok()),
        Some("no-referrer")
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn the_interface_shell_declares_a_strict_content_security_policy() {
    let harness = harness().await;
    let response = harness
        .client
        .get(harness.url("/"))
        .send()
        .await
        .expect("the request completes");
    let policy = response
        .headers()
        .get("content-security-policy")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if let Some(policy) = policy {
        assert!(policy.contains("default-src 'none'"));
        assert!(policy.contains("script-src 'self'"));
        assert!(policy.contains("frame-ancestors 'none'"));
        assert!(!policy.contains("unsafe-inline"));
    }
    harness.shutdown().await;
}

#[tokio::test]
async fn the_graph_definition_is_public_and_stable() {
    let harness = harness().await;
    let response = harness
        .client
        .get(harness.url("/api/v1/graph"))
        .send()
        .await
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("the body decodes");
    assert_eq!(body["nodes"].as_array().expect("nodes").len(), 14);
    assert!(!body["edges"].as_array().expect("edges").is_empty());
    harness.shutdown().await;
}
