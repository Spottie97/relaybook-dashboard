pub(crate) mod degraded;
mod discover;
mod fixture;
mod open;
mod v6;

pub use discover::{discover, LedgerSource};
pub use fixture::{apply_current_schema as fixture_current, apply_historical_migrations};
pub use open::{open_readonly, read_ledger, Opened};

use serde_json::Value;

/// `PRAGMA user_version` this build can read.
///
/// Public Relaybook main at `c25c29f55613228889099b59e1847ea6bf1c68b7`
/// (tag v0.3.3) sets `SCHEMA_VERSION` to 6 in `crates/core/src/schema.rs`.
/// A newer ledger needs a new reader module and one match arm in [`read_ledger`].
pub const SUPPORTED_SCHEMA_VERSION: i64 = 6;

#[derive(Debug, Clone)]
pub struct LedgerDump {
    pub schema_version: i64,
    /// True when `user_version` is not a fully mapped schema. Only tasks and
    /// events are filled, from whatever columns `PRAGMA table_info` reports.
    pub degraded: bool,
    pub tasks: Vec<TaskRow>,
    pub steps: Vec<StepRow>,
    pub decisions: Vec<DecisionRow>,
    pub deps: Vec<DepRow>,
    pub events: Vec<EventRow>,
    pub reports: Vec<ReportRow>,
    pub settings: Vec<SettingRow>,
    pub runs: Vec<RunRow>,
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub claimed_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub run_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct StepRow {
    pub id: i64,
    pub task_id: i64,
    pub position: i64,
    pub description: String,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub struct DecisionRow {
    pub id: i64,
    pub task_id: i64,
    pub rationale: String,
    pub provenance: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct DepRow {
    pub task_id: i64,
    pub depends_on_id: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct EventRow {
    pub id: i64,
    pub ts: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub id: i64,
    pub task_id: Option<i64>,
    pub kind: String,
    pub author: String,
    pub body: String,
    pub bytes: i64,
    pub created_at: String,
    pub reviewed_at: Option<String>,
    pub verdict: Option<String>,
    pub review_note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub updated_at: String,
    pub updated_by: String,
}

#[derive(Debug, Clone)]
pub struct RunRow {
    pub id: i64,
    pub prd_path: String,
    pub prd_sha256: String,
    pub branch: String,
    pub stage: String,
    pub cycle: i64,
    pub agent_calls: i64,
    pub est_cost_usd: f64,
    pub status: String,
    pub stop_reason: Option<String>,
    pub started_at: String,
    pub failure_streak: i64,
    pub current_slug: Option<String>,
    pub complete_requested: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger is busy")]
    Busy,
    #[error("ledger snapshot was torn")]
    Torn,
    #[error("ledger file is not readable")]
    Unreadable,
    #[error("ledger could not be read ({0})")]
    Sqlite(String),
}

impl LedgerError {
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Busy | Self::Torn)
    }

    /// Text safe to show in the UI. SQLite messages sometimes contain paths.
    pub fn public_message(&self) -> String {
        match self {
            Self::Busy => "ledger is busy, will retry".to_string(),
            Self::Torn => "ledger snapshot was torn, will retry".to_string(),
            Self::Unreadable => "ledger file is not readable".to_string(),
            Self::Sqlite(_) => "ledger could not be read".to_string(),
        }
    }
}

pub fn setting<'a>(dump: &'a LedgerDump, key: &str) -> Option<&'a str> {
    dump.settings
        .iter()
        .find(|row| row.key == key)
        .map(|row| row.value.as_str())
}
