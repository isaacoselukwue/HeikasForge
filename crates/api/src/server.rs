use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::http::{header, HeaderValue};
use axum::middleware;
use axum::Router;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_infrastructure::Runtime;
use tokio::net::TcpListener;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::info;

use crate::assets;
use crate::guard::guard;
use crate::routes;
use crate::state::ApiState;

pub const MAXIMUM_REQUEST_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Default)]
pub struct ServerOptions {
    pub port: u16,
    pub bind_all_interfaces: bool,
    pub public_origin: Option<String>,
    pub demonstration_mode: bool,
}

pub struct RunningServer {
    pub address: SocketAddr,
    pub bootstrap_url: String,
    pub state: ApiState,
    handle: tokio::task::JoinHandle<()>,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl RunningServer {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.handle.await;
    }

    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }
}

pub fn build_router(state: ApiState) -> Router {
    let security_headers = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    Router::new()
        .merge(routes::router())
        .fallback(assets::serve)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .layer(RequestBodyLimitLayer::new(MAXIMUM_REQUEST_BYTES))
        .layer(security_headers)
        .with_state(state)
}

pub async fn start(runtime: Runtime, options: ServerOptions) -> ApplicationResult<RunningServer> {
    if options.bind_all_interfaces && options.public_origin.is_none() {
        return Err(ApplicationError::InvalidConfiguration(
            "binding every interface requires an explicit public origin so that the cross-site request forgery and host checks remain enforceable. Supply --public-origin http://<host>:<port>."
                .to_string(),
        ));
    }
    let state = ApiState::new(runtime, options.demonstration_mode);
    let address = SocketAddr::new(
        if options.bind_all_interfaces {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        },
        options.port,
    );
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| ApplicationError::Storage(format!("could not bind {address}: {error}")))?;
    let bound = listener
        .local_addr()
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
    let origin = options
        .public_origin
        .clone()
        .unwrap_or_else(|| format!("http://{bound}"));
    state.set_origin(origin.clone()).await;

    let bootstrap_url = format!("{origin}/#token={}", state.sessions.bootstrap_token());
    let router = build_router(state.clone());
    let (shutdown, receiver) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = receiver.await;
        });
        if let Err(error) = server.await {
            tracing::error!(error = %error, "the local interface server stopped");
        }
    });

    info!(address = %bound, "the local interface is listening");
    Ok(RunningServer {
        address: bound,
        bootstrap_url,
        state,
        handle,
        shutdown,
    })
}
