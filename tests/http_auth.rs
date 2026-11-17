use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use relaybook_dashboard::auth::Auth;
use relaybook_dashboard::http::{router, AppState};
use relaybook_dashboard::hub::Hub;
use relaybook_dashboard::model::{AppSnapshot, Meta};

fn app() -> axum::Router {
    let hub = Hub::new(AppSnapshot {
        server_time: "2026-11-17T00:00:00Z".to_string(),
        poll_ms: 1500,
        demo: false,
        changed: Vec::new(),
        workspaces: Vec::new(),
    });
    let auth = Auth::new(Some("secret".to_string()), None, None, true);
    let meta = Arc::new(Meta {
        demo: false,
        poll_ms: 1500,
        bind: "127.0.0.1:7878".to_string(),
        auth_basic: false,
        auth_token: true,
        auth_required: true,
        schema_supported: 6,
        read_only: true,
        log_rules: Vec::new(),
    });
    router(AppState { hub, auth, meta })
}

#[tokio::test]
async fn health_is_open_and_the_api_rejects_anonymous_callers() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(std::str::from_utf8(&bytes).unwrap().contains("\"ok\":true"));

    let denied = app()
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let allowed = app()
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);

    let write = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/state")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(write.status(), StatusCode::METHOD_NOT_ALLOWED);
}
