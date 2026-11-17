use serde::Serialize;
use serde_json::Value;

/// Normalized line from Claude Code, Cursor CLI, Codex CLI, Graphify, or Orca.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NormEvent {
    pub seq: u64,
    pub ts: String,
    pub agent: String,
    pub kind: String,
    pub summary: String,
    pub detail: Value,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub server_time: String,
    pub poll_ms: u64,
    pub demo: bool,
    pub changed: Vec<String>,
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub id: String,
    pub label: String,
    pub schema_version: Option<i64>,
    pub schema_warning: Option<String>,
    pub ledger_ok: bool,
    pub ledger_error: Option<String>,
    pub stale: bool,
    pub logs_found: usize,
    pub unmapped_logs: usize,
    pub health_ok: bool,
    pub health_label: String,
    pub settings: SettingsView,
    pub mission: Option<Mission>,
    pub stats: Stats,
    pub agents: Vec<AgentCard>,
    pub session_active: bool,
    pub session_label: String,
    pub timeline: Vec<TimelineItem>,
    pub tasks: Vec<TaskView>,
    pub runs: Vec<RunView>,
    pub artifacts: Vec<ArtifactRef>,
    pub logs: Vec<NormEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub mode: String,
    pub gates: String,
    pub max_cycles: i64,
    pub max_reopens_per_task: i64,
    pub max_agent_calls: i64,
    pub cursor_timeout_min: i64,
    pub est_cost_cap_usd: Option<String>,
}

impl Default for SettingsView {
    fn default() -> Self {
        Self {
            mode: "manual".to_string(),
            gates: "plan".to_string(),
            max_cycles: 20,
            max_reopens_per_task: 2,
            max_agent_calls: 60,
            cursor_timeout_min: 30,
            est_cost_cap_usd: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Mission {
    pub title: String,
    pub status: String,
    pub stage: Option<String>,
    pub run_id: Option<i64>,
    pub task_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub agents_online: usize,
    pub active_missions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCard {
    pub id: String,
    pub name: String,
    pub role: String,
    pub readonly: bool,
    pub status: String,
    pub progress: Option<u8>,
    pub latest: Option<String>,
    pub latest_ts: Option<String>,
    pub log_present: bool,
    pub claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineItem {
    pub id: String,
    pub ts: String,
    pub actor: String,
    pub kind: String,
    pub text: String,
    pub detail: Option<String>,
    pub collapsible: bool,
    pub checks: Vec<CheckItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckItem {
    pub label: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepView {
    pub position: i64,
    pub description: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub claimed_by: Option<String>,
    pub run_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub depends_on: Vec<String>,
    pub steps_done: i64,
    pub steps_total: i64,
    pub steps: Vec<StepView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunView {
    pub id: i64,
    pub prd_path: String,
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
    pub waiting_on: Option<String>,
    pub task_slugs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactRef {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub detail: String,
    pub bytes: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactBody {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub language: String,
    pub content: String,
    pub truncated: bool,
    pub bytes: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    pub demo: bool,
    pub poll_ms: u64,
    pub bind: String,
    pub auth_basic: bool,
    pub auth_token: bool,
    pub auth_required: bool,
    pub schema_supported: i64,
    pub read_only: bool,
    pub log_rules: Vec<LogRuleView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogRuleView {
    pub agent: String,
    pub glob: String,
}

pub fn empty_workspace(id: &str, label: &str) -> Workspace {
    Workspace {
        id: id.to_string(),
        label: label.to_string(),
        schema_version: None,
        schema_warning: None,
        ledger_ok: false,
        ledger_error: Some("Ledger has not been read yet".to_string()),
        stale: false,
        logs_found: 0,
        unmapped_logs: 0,
        health_ok: false,
        health_label: "Ledger has not been read yet".to_string(),
        settings: SettingsView::default(),
        mission: None,
        stats: Stats {
            agents_online: 0,
            active_missions: 0,
        },
        agents: Vec::new(),
        session_active: false,
        session_label: "No active session".to_string(),
        timeline: Vec::new(),
        tasks: Vec::new(),
        runs: Vec::new(),
        artifacts: Vec::new(),
        logs: Vec::new(),
    }
}
