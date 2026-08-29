use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct EmbeddedAssets;

const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
img-src 'self' data:; font-src 'self'; connect-src 'self'; media-src 'self'; base-uri 'none'; \
form-action 'none'; frame-ancestors 'none'; object-src 'none'";

const MISSING_BUNDLE_NOTICE: &str =
    "The graphical interface bundle is not embedded in this build. Run `pnpm --dir apps/web build` and rebuild the executable.";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };
    match EmbeddedAssets::get(candidate) {
        Some(file) => build_response(candidate, file.data.into_owned()),
        None => match EmbeddedAssets::get("index.html") {
            Some(file) => build_response("index.html", file.data.into_owned()),
            None => (StatusCode::NOT_FOUND, MISSING_BUNDLE_NOTICE).into_response(),
        },
    }
}

fn build_response(path: &str, bytes: Vec<u8>) -> Response {
    let media_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&media_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    if path == "index.html" {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

pub fn interface_is_embedded() -> bool {
    EmbeddedAssets::get("index.html").is_some()
}
