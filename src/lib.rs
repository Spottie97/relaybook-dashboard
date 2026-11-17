pub mod artifacts;
pub mod auth;
pub mod config;
pub mod demo;
pub mod derive;
pub mod feed;
pub mod http;
pub mod hub;
pub mod ledger;
pub mod logs;
pub mod model;
pub mod timefmt;

use std::sync::Arc;

use crate::auth::Auth;
use crate::config::Config;
use crate::http::AppState;
use crate::hub::Hub;
use crate::model::{AppSnapshot, Meta};

pub fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let mut config = Config::from_env()?;
    let demo_hold = if config.demo {
        let demo = demo::prepare()?;
        config.ledgers = vec![demo.ledger_dir.clone()];
        config.logs = Some(demo.context_dir.clone());
        config.artifacts = Some(demo.artifacts_dir.clone());
        tracing::info!("demo mode is using a temporary fixture ledger");
        if config.bind.ip().is_unspecified() && !config.has_credentials() {
            tracing::warn!("demo mode is unauthenticated and bound to an unspecified address");
        }
        Some(demo)
    } else {
        None
    };
    if !config.demo && !config.has_credentials() {
        anyhow::bail!(
            "refusing to listen without authentication. Set DASHBOARD_TOKEN or both DASHBOARD_BASIC_USER and DASHBOARD_BASIC_PASSWORD. --demo is the only unauthenticated mode."
        );
    }
    if config.ledgers.is_empty() {
        anyhow::bail!("pass --ledger or set LEDGER_DIR to a ledger file or a directory of ledgers");
    }
    let rules = crate::logs::rules_from_config(&config.log_map).map_err(anyhow::Error::msg)?;
    let enforce = !config.demo || config.has_credentials();
    let auth = Auth::new(
        config.token.clone(),
        config.basic_user.clone(),
        config.basic_password.clone(),
        enforce,
    );
    let meta = Arc::new(Meta {
        demo: config.demo,
        poll_ms: config.poll_ms,
        bind: config.bind.to_string(),
        auth_basic: auth.basic_enabled(),
        auth_token: auth.token_enabled(),
        auth_required: auth.required,
        schema_supported: ledger::SUPPORTED_SCHEMA_VERSION,
        read_only: true,
        log_rules: rules
            .iter()
            .map(|rule| crate::model::LogRuleView {
                agent: rule.agent.clone(),
                glob: rule.pattern.clone(),
            })
            .collect(),
    });
    let hub = Hub::new(AppSnapshot {
        server_time: timefmt::now_rfc3339(),
        poll_ms: config.poll_ms,
        demo: config.demo,
        changed: Vec::new(),
        workspaces: Vec::new(),
    });
    let state = AppState {
        hub: hub.clone(),
        auth,
        meta,
    };
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        if let Some(demo) = demo_hold {
            demo::spawn_replay(demo.context_dir.clone());
            // Keep the fixture directory until the process exits.
            Box::leak(Box::new(demo));
        }
        feed::spawn(
            hub,
            config.ledgers.clone(),
            config.logs.clone(),
            config.artifacts.clone(),
            config.log_map.clone(),
            config.poll_ms,
            config.demo,
        );
        let listener = tokio::net::TcpListener::bind(config.bind).await?;
        tracing::info!(%config.bind, "relaybook-dashboard listening (read-only)");
        axum::serve(listener, http::router(state))
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
