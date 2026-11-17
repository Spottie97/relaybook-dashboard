use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use crate::artifacts::list_files;
use crate::derive::project;
use crate::hub::{failed_workspace, Hub, Store};
use crate::ledger::{discover, read_ledger, LedgerDump, LedgerError, LedgerSource};
use crate::logs::{rules_from_config, TailSet};
use crate::model::{AppSnapshot, NormEvent, Workspace};
use crate::timefmt::now_rfc3339;

const LOG_CAP: usize = 400;

pub fn spawn(
    hub: Hub,
    ledgers: Vec<PathBuf>,
    logs: Option<PathBuf>,
    artifacts: Option<PathBuf>,
    log_map: Vec<String>,
    poll_ms: u64,
    demo: bool,
) {
    tokio::spawn(async move {
        let rules = match rules_from_config(&log_map) {
            Ok(rules) => rules,
            Err(err) => {
                tracing::error!("{err}");
                Vec::new()
            }
        };
        let mut tails = TailSet::new(rules);
        let mut logs_buf: VecDeque<NormEvent> = VecDeque::new();
        let mut previous: HashMap<String, Workspace> = HashMap::new();
        let mut signatures: HashMap<String, String> = HashMap::new();
        loop {
            let tick = TickInput {
                ledgers: ledgers.clone(),
                logs: logs.clone(),
                artifacts: artifacts.clone(),
            };
            let read = tokio::task::spawn_blocking(move || read_sources(&tick))
                .await
                .unwrap_or_else(|_| Vec::new());
            let fresh = tails.poll(logs.as_deref());
            for event in fresh {
                logs_buf.push_back(event.clone());
                hub.publish_log(&event.agent, &event);
            }
            while logs_buf.len() > LOG_CAP {
                logs_buf.pop_front();
            }
            let log_slice: Vec<NormEvent> = logs_buf.iter().cloned().collect();
            let log_agents = agents_seen(&tails);
            let mut changed = Vec::new();
            let mut workspaces = Vec::new();
            if read.is_empty() {
                let mut workspace = failed_workspace(
                    "ledger",
                    "No ledger found. Set LEDGER_DIR or pass --ledger.",
                );
                workspace.logs = log_slice.clone();
                workspaces.push(workspace);
            }
            for (source, result) in read {
                let (workspace, store) = match result {
                    Ok(dump) => {
                        let files = artifacts.as_deref().map(list_files).unwrap_or_default();
                        let derived = project(
                            &source.id,
                            &dump,
                            &log_slice,
                            &log_agents,
                            tails.unmapped,
                            &files,
                        );
                        let store = Store {
                            reports: derived.reports,
                            decisions: derived.decisions,
                            slugs: derived.task_titles,
                            artifact_root: artifacts.clone(),
                        };
                        (derived.workspace, Some(store))
                    }
                    Err(err) if err.retryable() => {
                        if let Some(prev) = previous.get(&source.id) {
                            let mut workspace = prev.clone();
                            workspace.stale = true;
                            workspace.health_ok = false;
                            workspace.ledger_ok = true;
                            workspace.health_label = err.public_message();
                            workspace.ledger_error = Some(err.public_message());
                            workspace.logs = log_slice.clone();
                            tracing::warn!(workspace = %source.id, "ledger read will retry");
                            (workspace, None)
                        } else {
                            (error_workspace(&source, &err, &log_slice), None)
                        }
                    }
                    Err(err) => (error_workspace(&source, &err, &log_slice), None),
                };
                let signature = signature_of(&workspace);
                if signatures.get(&source.id) != Some(&signature) {
                    changed.push(source.id.clone());
                    signatures.insert(source.id.clone(), signature);
                }
                if store.is_some() {
                    previous.insert(source.id.clone(), workspace.clone());
                }
                if let Some(store) = store {
                    hub.set_store(&source.id, store);
                }
                workspaces.push(workspace);
            }
            workspaces.sort_by(|left, right| left.label.cmp(&right.label));
            let snapshot = AppSnapshot {
                server_time: now_rfc3339(),
                poll_ms,
                demo,
                changed,
                workspaces,
            };
            let publish = !snapshot.changed.is_empty();
            hub.set_snapshot(snapshot);
            if publish {
                hub.publish_state();
            }
            tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        }
    });
}

struct TickInput {
    ledgers: Vec<PathBuf>,
    logs: Option<PathBuf>,
    artifacts: Option<PathBuf>,
}

fn read_sources(input: &TickInput) -> Vec<(LedgerSource, Result<LedgerDump, LedgerError>)> {
    let _ = &input.logs;
    let _ = &input.artifacts;
    let sources = discover(&input.ledgers);
    sources
        .into_iter()
        .map(|source| {
            let result = read_ledger(&source.path);
            (source, result)
        })
        .collect()
}

fn error_workspace(source: &LedgerSource, err: &LedgerError, logs: &[NormEvent]) -> Workspace {
    let mut workspace = failed_workspace(&source.id, &err.public_message());
    workspace.logs = logs.to_vec();
    if matches!(err, LedgerError::UnsupportedSchema { .. }) {
        if let LedgerError::UnsupportedSchema { found, .. } = err {
            workspace.schema_version = Some(*found);
        }
    }
    workspace
}

fn signature_of(workspace: &Workspace) -> String {
    let mut copy = workspace.clone();
    copy.logs.clear();
    serde_json::to_string(&copy).unwrap_or_default()
}

fn agents_seen(tails: &TailSet) -> Vec<String> {
    tails.known_agents()
}
