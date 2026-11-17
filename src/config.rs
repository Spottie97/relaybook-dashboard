use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{ArgAction, Parser};

#[derive(Debug, Clone)]
pub struct Config {
    pub ledgers: Vec<PathBuf>,
    pub logs: Option<PathBuf>,
    pub artifacts: Option<PathBuf>,
    pub bind: SocketAddr,
    pub poll_ms: u64,
    pub demo: bool,
    pub token: Option<String>,
    pub basic_user: Option<String>,
    pub basic_password: Option<String>,
    pub log_map: Vec<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let cli = Cli::parse();
        let demo = cli.demo || env_flag("DEMO");
        let bind = resolve_bind(cli.bind, cli.port)?;
        let poll_ms = cli.poll_ms.clamp(500, 60_000);
        Ok(Self {
            ledgers: cli.ledger,
            logs: cli.logs,
            artifacts: cli.artifacts,
            bind,
            poll_ms,
            demo,
            token: nonempty(cli.token),
            basic_user: nonempty(cli.basic_user),
            basic_password: nonempty(cli.basic_password),
            log_map: cli.log_map,
        })
    }

    pub fn has_credentials(&self) -> bool {
        self.token.is_some() || (self.basic_user.is_some() && self.basic_password.is_some())
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "relaybook-dashboard",
    version,
    about = "Watch-only command center for a Relaybook ledger"
)]
struct Cli {
    /// Ledger file, ledger directory, or a directory of ledgers.
    #[arg(long = "ledger", env = "LEDGER_DIR")]
    ledger: Vec<PathBuf>,

    /// Directory of agent JSON logs.
    #[arg(long = "logs", env = "CONTEXT_DIR")]
    logs: Option<PathBuf>,

    /// Directory of plans, reviews, and other files to show as artifacts.
    #[arg(long = "artifacts", env = "ARTIFACTS_DIR")]
    artifacts: Option<PathBuf>,

    /// Socket to bind, for example 127.0.0.1:7878. Overrides PORT.
    #[arg(long = "bind", env = "BIND")]
    bind: Option<String>,

    /// Listen on 0.0.0.0:PORT when BIND is unset. Coolify sets PORT.
    #[arg(long = "port", env = "PORT")]
    port: Option<u16>,

    /// How often to re-read the ledger and logs.
    #[arg(long = "poll-ms", env = "POLL_MS", default_value_t = 1500)]
    poll_ms: u64,

    /// Map logs to an agent. Repeatable, or comma-separated. Format: agent=glob
    #[arg(long = "log-map", env = "LOG_MAP", value_delimiter = ',')]
    log_map: Vec<String>,

    /// Serve a built-in fixture and replay sample logs. No auth unless credentials are set.
    #[arg(long = "demo", action = ArgAction::SetTrue)]
    demo: bool,

    /// Bearer token. Required in normal mode unless basic auth is set.
    #[arg(long = "token", env = "DASHBOARD_TOKEN")]
    token: Option<String>,

    #[arg(long = "basic-user", env = "DASHBOARD_BASIC_USER")]
    basic_user: Option<String>,

    #[arg(long = "basic-password", env = "DASHBOARD_BASIC_PASSWORD")]
    basic_password: Option<String>,
}

fn resolve_bind(bind: Option<String>, port: Option<u16>) -> anyhow::Result<SocketAddr> {
    if let Some(bind) = bind {
        return bind
            .parse()
            .map_err(|_| anyhow::anyhow!("BIND must be a socket address such as 127.0.0.1:7878"));
    }
    if let Some(port) = port {
        return Ok(SocketAddr::from(([0, 0, 0, 0], port)));
    }
    Ok(SocketAddr::from(([127, 0, 0, 1], 7878)))
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref().map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("on")
    )
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
