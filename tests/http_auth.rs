use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use relaybook_dashboard::auth::{Auth, BASIC_CHALLENGE};
use relaybook_dashboard::http::{router, AppState};
use relaybook_dashboard::hub::Hub;
use relaybook_dashboard::model::{AppSnapshot, Meta};

fn app_with(token: Option<&str>, user: Option<&str>, password: Option<&str>) -> axum::Router {
    let hub = Hub::new(AppSnapshot {
        server_time: "2026-11-17T00:00:00Z".to_string(),
        poll_ms: 1500,
        demo: false,
        changed: Vec::new(),
        workspaces: Vec::new(),
    });
    let auth = Auth::new(
        token.map(str::to_string),
        user.map(str::to_string),
        password.map(str::to_string),
        true,
    );
    let meta = Arc::new(Meta {
        demo: false,
        poll_ms: 1500,
        bind: "127.0.0.1:7878".to_string(),
        auth_basic: auth.basic_enabled(),
        auth_token: auth.token_enabled(),
        auth_required: true,
        schema_supported: 6,
        read_only: true,
        log_rules: Vec::new(),
    });
    router(AppState { hub, auth, meta })
}

fn app() -> axum::Router {
    app_with(Some("secret"), None, None)
}

fn app_basic() -> axum::Router {
    app_with(Some("secret"), Some("ops"), Some("pw"))
}

fn basic(user: &str, password: &str) -> String {
    format!("Basic {}", b64(&format!("{user}:{password}")))
}

fn b64(text: &str) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let b0 = bytes[index];
        let b1 = bytes.get(index + 1).copied().unwrap_or(0);
        let b2 = bytes.get(index + 2).copied().unwrap_or(0);
        let left = bytes.len() - index;
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if left == 1 {
            out.push('=');
            out.push('=');
        } else if left == 2 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
            out.push('=');
        } else {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        }
        index += 3;
    }
    out
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
    assert!(denied.headers().get("www-authenticate").is_none());

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

    let page = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(page.status(), StatusCode::UNAUTHORIZED);

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

#[tokio::test]
async fn basic_auth_challenges_the_page_and_accepts_the_browser_or_a_bearer() {
    let page = app_basic()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        page.headers()
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap(),
        BASIC_CHALLENGE
    );

    let asset = app_basic()
        .oneshot(
            Request::builder()
                .uri("/assets/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        asset
            .headers()
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap(),
        BASIC_CHALLENGE
    );

    let meta = app_basic()
        .oneshot(
            Request::builder()
                .uri("/api/meta")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(meta.status(), StatusCode::UNAUTHORIZED);

    let health = app_basic()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    assert!(health.headers().get("www-authenticate").is_none());

    let wrong = app_basic()
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .header("authorization", basic("ops", "no"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong
            .headers()
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap(),
        BASIC_CHALLENGE
    );

    let with_basic = app_basic()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("authorization", basic("ops", "pw"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(with_basic.status(), StatusCode::OK);

    let state = app_basic()
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .header("authorization", basic("ops", "pw"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(state.status(), StatusCode::OK);

    let bearer = app_basic()
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bearer.status(), StatusCode::OK);

    let stream = app_basic()
        .oneshot(
            Request::builder()
                .uri("/api/stream")
                .header("authorization", basic("ops", "pw"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stream.status(), StatusCode::OK);
    let content_type = stream
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.starts_with("text/event-stream"),
        "{content_type}"
    );
    let mut body = stream.into_body();
    let frame = tokio::time::timeout(Duration::from_secs(2), body.frame())
        .await
        .expect("sse timed out")
        .expect("sse ended")
        .expect("sse frame");
    let bytes = frame.into_data().unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("event:") || text.contains("data:"), "{text}");

    let denied_stream = app_basic()
        .oneshot(
            Request::builder()
                .uri("/api/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied_stream.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        denied_stream
            .headers()
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap(),
        BASIC_CHALLENGE
    );
}
