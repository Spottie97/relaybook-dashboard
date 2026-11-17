use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;

use crate::ledger::{DecisionRow, ReportRow};
use crate::model::{AppSnapshot, ArtifactBody, NormEvent, Workspace};

#[derive(Clone)]
pub struct Hub {
    inner: Arc<Inner>,
}

struct Inner {
    snap: RwLock<AppSnapshot>,
    store: RwLock<HashMap<String, Store>>,
    tx: broadcast::Sender<String>,
}

pub struct Store {
    pub reports: Vec<ReportRow>,
    pub decisions: Vec<DecisionRow>,
    pub slugs: HashMap<i64, String>,
    pub artifact_root: Option<PathBuf>,
}

impl Hub {
    pub fn new(snapshot: AppSnapshot) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(Inner {
                snap: RwLock::new(snapshot),
                store: RwLock::new(HashMap::new()),
                tx,
            }),
        }
    }

    pub fn snapshot(&self) -> AppSnapshot {
        self.inner
            .snap
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    pub fn set_snapshot(&self, snapshot: AppSnapshot) {
        *self
            .inner
            .snap
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = snapshot;
    }

    pub fn set_store(&self, id: &str, store: Store) {
        self.inner
            .store
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(id.to_string(), store);
    }

    pub fn publish_state(&self) {
        let snapshot = self.snapshot();
        if let Ok(data) = serde_json::to_string(&snapshot) {
            let _ = self.inner.tx.send(format!("state\n{data}"));
        }
    }

    pub fn publish_log(&self, workspace: &str, event: &NormEvent) {
        let payload = serde_json::json!({ "workspace": workspace, "event": event });
        if let Ok(data) = serde_json::to_string(&payload) {
            let _ = self.inner.tx.send(format!("log\n{data}"));
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.inner.tx.subscribe()
    }

    pub fn artifact(&self, workspace: &str, id: &str) -> Option<ArtifactBody> {
        let guard = self
            .inner
            .store
            .read()
            .unwrap_or_else(|poison| poison.into_inner());
        let store = guard.get(workspace)?;
        if let Some(rest) = id.strip_prefix("report:") {
            let report_id: i64 = rest.parse().ok()?;
            let report = store.reports.iter().find(|report| report.id == report_id)?;
            let slug = report
                .task_id
                .and_then(|task| store.slugs.get(&task))
                .map(String::as_str)
                .unwrap_or("note");
            return Some(crate::artifacts::load_report(report, slug));
        }
        if let Some(rest) = id.strip_prefix("decision:") {
            let decision_id: i64 = rest.parse().ok()?;
            let decision = store
                .decisions
                .iter()
                .find(|decision| decision.id == decision_id)?;
            let slug = store
                .slugs
                .get(&decision.task_id)
                .map(String::as_str)
                .unwrap_or("task");
            return Some(crate::artifacts::load_decision(decision, slug));
        }
        if id.starts_with("file:") {
            let root = store.artifact_root.as_deref()?;
            return crate::artifacts::load_file(root, id);
        }
        None
    }
}

pub fn failed_workspace(id: &str, message: &str) -> Workspace {
    let mut workspace = crate::model::empty_workspace(id, id);
    workspace.ledger_error = Some(message.to_string());
    workspace.health_label = message.to_string();
    workspace.agents = placeholder_agents();
    workspace
}

fn placeholder_agents() -> Vec<crate::model::AgentCard> {
    [
        ("claude", "Claude", "Planner", false),
        ("cursor", "Cursor", "Builder", false),
        ("codex", "Codex", "Reviewer", true),
        ("graphify", "Graphify", "Context", false),
    ]
    .into_iter()
    .map(|(id, name, role, readonly)| crate::model::AgentCard {
        id: id.to_string(),
        name: name.to_string(),
        role: role.to_string(),
        readonly,
        status: "idle".to_string(),
        progress: None,
        latest: None,
        latest_ts: None,
        log_present: false,
        claims: Vec::new(),
    })
    .collect()
}
