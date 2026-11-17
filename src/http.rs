use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::get;
use axum::{Json, Router};
use futures::StreamExt;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Auth, BASIC_CHALLENGE};
use crate::hub::Hub;
use crate::model::Meta;

#[derive(Clone)]
pub struct AppState {
    pub hub: Hub,
    pub auth: Auth,
    pub meta: Arc<Meta>,
}

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

pub fn router(state: AppState) -> Router {
    let auth = state.auth.clone();
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/meta", get(meta))
        .route("/api/state", get(state_of))
        .route("/api/stream", get(stream))
        .route("/api/artifact", get(artifact))
        .fallback(static_file)
        .layer(middleware::from_fn_with_state(auth, require_auth))
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn meta(State(state): State<AppState>) -> Json<Meta> {
    Json((*state.meta).clone())
}

async fn state_of(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(state.hub.snapshot()),
    )
}

async fn artifact(State(state): State<AppState>, Query(query): Query<ArtifactQuery>) -> Response {
    match state.hub.artifact(&query.workspace, &query.id) {
        Some(body) => ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "ok": false, "error": "not found" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ArtifactQuery {
    workspace: String,
    id: String,
}

async fn stream(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let current = serde_json::to_string(&state.hub.snapshot()).unwrap_or_else(|_| "{}".to_string());
    let rx = state.hub.subscribe();
    let hub = state.hub.clone();
    let first = futures::stream::once(async move {
        Ok(axum::response::sse::Event::default()
            .event("state")
            .data(current))
    });
    let follow = BroadcastStream::new(rx).filter_map(move |item| {
        let hub = hub.clone();
        async move {
            match item {
                Ok(payload) => Some(Ok(sse_from(payload))),
                Err(BroadcastStreamRecvError::Lagged(_)) => {
                    let data =
                        serde_json::to_string(&hub.snapshot()).unwrap_or_else(|_| "{}".to_string());
                    Some(Ok(axum::response::sse::Event::default()
                        .event("state")
                        .data(data)))
                }
            }
        }
    });
    Sse::new(first.chain(follow)).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    )
}

fn sse_from(payload: String) -> axum::response::sse::Event {
    match payload.split_once('\n') {
        Some((name, data)) => axum::response::sse::Event::default().event(name).data(data),
        None => axum::response::sse::Event::default().data(payload),
    }
}

async fn require_auth(
    State(auth): State<Auth>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if !auth.required || path == "/healthz" {
        return next.run(request).await;
    }
    // Token-only mode leaves the page and /api/meta open so the in-page token
    // screen can load. Basic auth challenges every other route, including `/`.
    if !auth.basic_enabled() && (path == "/api/meta" || !path.starts_with("/api")) {
        return next.run(request).await;
    }
    if auth.allows(request.headers()) {
        return next.run(request).await;
    }
    let mut response = (
        StatusCode::UNAUTHORIZED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "ok": false, "error": "unauthorized" })),
    )
        .into_response();
    if auth.challenge_basic() {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(BASIC_CHALLENGE),
        );
    }
    response
}

async fn static_file(uri: Uri) -> Response {
    let requested = uri.path().trim_start_matches('/');
    let path = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    if let Some(file) = Assets::get(path) {
        return asset_response(path, file.data.as_ref());
    }
    if path.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get("index.html") {
        Some(file) => asset_response("index.html", file.data.as_ref()),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn asset_response(path: &str, bytes: &[u8]) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=86400"
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(bytes.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
