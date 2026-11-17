use std::collections::HashMap;

use serde_json::Value;

use crate::ledger::{setting, DecisionRow, LedgerDump, ReportRow};
use crate::model::{
    AgentCard, ArtifactRef, CheckItem, Mission, NormEvent, RunView, SettingsView, Stats, StepView,
    TaskView, TimelineItem, Workspace,
};

const AGENTS: &[(&str, &str, &str, bool)] = &[
    ("claude", "Claude", "Planner", false),
    ("cursor", "Cursor", "Builder", false),
    ("codex", "Codex", "Reviewer", true),
    ("graphify", "Graphify", "Context", false),
];

pub struct Derived {
    pub workspace: Workspace,
    pub reports: Vec<ReportRow>,
    pub decisions: Vec<DecisionRow>,
    pub task_titles: HashMap<i64, String>,
}

pub fn project(
    id: &str,
    dump: &LedgerDump,
    logs: &[NormEvent],
    log_agents: &[String],
    unmapped: usize,
    files: &[ArtifactRef],
) -> Derived {
    let tasks = task_views(dump);
    let runs = run_views(dump, &tasks);
    let settings = settings_view(dump);
    let mission = mission(&tasks, &runs);
    let unreviewed = dump
        .reports
        .iter()
        .any(|report| report.reviewed_at.is_none());
    let agents = agent_cards(&tasks, &runs, logs, log_agents, unreviewed);
    let logs_found = log_agents.len();
    let online = agents
        .iter()
        .filter(|agent| agent.log_present || agent.status != "idle")
        .count();
    let active_missions = runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "active" | "waiting" | "paused"))
        .count()
        .max(usize::from(
            mission
                .as_ref()
                .is_some_and(|item| item.status == "running"),
        ));
    let session_active = runs
        .iter()
        .any(|run| matches!(run.status.as_str(), "active" | "waiting" | "paused"))
        || tasks.iter().any(|task| task.status == "claimed");
    let mut artifacts = artifact_refs(dump, &tasks);
    artifacts.extend(files.iter().cloned());
    artifacts.sort_by(|left, right| right.modified.cmp(&left.modified));
    artifacts.truncate(12);
    let timeline = timeline(&tasks, &runs, logs);
    let task_titles = tasks
        .iter()
        .map(|task| (task.id, task.title.clone()))
        .collect();
    let schema_warning = if dump.degraded {
        Some(crate::ledger::degraded::banner(dump.schema_version))
    } else {
        None
    };
    let health_label = if let Some(warning) = &schema_warning {
        warning.clone()
    } else if logs_found > 0 {
        "All systems operational".to_string()
    } else {
        "Ledger readable, no logs found".to_string()
    };
    Derived {
        workspace: Workspace {
            id: id.to_string(),
            label: id.to_string(),
            schema_version: Some(dump.schema_version),
            schema_warning,
            ledger_ok: true,
            ledger_error: None,
            stale: false,
            logs_found,
            unmapped_logs: unmapped,
            health_ok: !dump.degraded,
            health_label,
            settings,
            mission,
            stats: Stats {
                agents_online: online,
                active_missions,
            },
            agents,
            session_active,
            session_label: if session_active {
                "Session active".to_string()
            } else {
                "No active session".to_string()
            },
            timeline,
            tasks,
            runs,
            artifacts,
            logs: logs.to_vec(),
        },
        reports: dump.reports.clone(),
        decisions: dump.decisions.clone(),
        task_titles,
    }
}

fn task_views(dump: &LedgerDump) -> Vec<TaskView> {
    let slugs: HashMap<i64, String> = dump
        .tasks
        .iter()
        .map(|task| (task.id, task.slug.clone()))
        .collect();
    dump.tasks
        .iter()
        .map(|task| {
            let steps: Vec<StepView> = dump
                .steps
                .iter()
                .filter(|step| step.task_id == task.id)
                .map(|step| StepView {
                    position: step.position,
                    description: step.description.clone(),
                    done: step.done,
                })
                .collect();
            let depends_on = dump
                .deps
                .iter()
                .filter(|dep| dep.task_id == task.id)
                .filter_map(|dep| slugs.get(&dep.depends_on_id).cloned())
                .collect();
            TaskView {
                id: task.id,
                slug: task.slug.clone(),
                title: task.title.clone(),
                status: task.status.clone(),
                claimed_by: task.claimed_by.clone(),
                run_id: task.run_id,
                created_at: task.created_at.clone(),
                updated_at: task.updated_at.clone(),
                depends_on,
                steps_done: steps.iter().filter(|step| step.done).count() as i64,
                steps_total: steps.len() as i64,
                steps,
            }
        })
        .collect()
}

fn run_views(dump: &LedgerDump, tasks: &[TaskView]) -> Vec<RunView> {
    dump.runs
        .iter()
        .map(|run| {
            let task_slugs = tasks
                .iter()
                .filter(|task| task.run_id == Some(run.id))
                .map(|task| task.slug.clone())
                .collect();
            let waiting_on = match run.status.as_str() {
                "waiting" => Some("approval".to_string()),
                "paused" => Some(
                    run.stop_reason
                        .clone()
                        .unwrap_or_else(|| "paused".to_string()),
                ),
                _ => None,
            };
            RunView {
                id: run.id,
                prd_path: run.prd_path.clone(),
                branch: run.branch.clone(),
                stage: run.stage.clone(),
                cycle: run.cycle,
                agent_calls: run.agent_calls,
                est_cost_usd: run.est_cost_usd,
                status: run.status.clone(),
                stop_reason: run.stop_reason.clone(),
                started_at: run.started_at.clone(),
                failure_streak: run.failure_streak,
                current_slug: run.current_slug.clone(),
                complete_requested: run.complete_requested,
                waiting_on,
                task_slugs,
            }
        })
        .collect()
}

fn settings_view(dump: &LedgerDump) -> SettingsView {
    SettingsView {
        mode: setting(dump, "mode").unwrap_or("manual").to_string(),
        gates: setting(dump, "gates").unwrap_or("plan").to_string(),
        max_cycles: number_setting(dump, "max_cycles", 20),
        max_reopens_per_task: number_setting(dump, "max_reopens_per_task", 2),
        max_agent_calls: number_setting(dump, "max_agent_calls", 60),
        cursor_timeout_min: number_setting(dump, "cursor_timeout_min", 30),
        est_cost_cap_usd: setting(dump, "est_cost_cap_usd").map(str::to_string),
    }
}

fn number_setting(dump: &LedgerDump, key: &str, default: i64) -> i64 {
    setting(dump, key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn mission(tasks: &[TaskView], runs: &[RunView]) -> Option<Mission> {
    let run = runs
        .iter()
        .rev()
        .find(|run| matches!(run.status.as_str(), "active" | "waiting" | "paused"))
        .or_else(|| {
            runs.iter()
                .rev()
                .find(|run| matches!(run.status.as_str(), "complete" | "stopped"))
        });
    if let Some(run) = run {
        let title = run
            .current_slug
            .as_deref()
            .and_then(|slug| tasks.iter().find(|task| task.slug == slug))
            .map(|task| task.title.clone())
            .unwrap_or_else(|| file_name(&run.prd_path));
        return Some(Mission {
            title,
            status: mission_status(&run.status),
            stage: Some(run.stage.clone()),
            run_id: Some(run.id),
            task_slug: run.current_slug.clone(),
        });
    }
    if let Some(task) = tasks.iter().rev().find(|task| task.status == "claimed") {
        return Some(Mission {
            title: task.title.clone(),
            status: "running".to_string(),
            stage: None,
            run_id: task.run_id,
            task_slug: Some(task.slug.clone()),
        });
    }
    tasks
        .iter()
        .rev()
        .find(|task| task.status == "done")
        .map(|task| Mission {
            title: task.title.clone(),
            status: "done".to_string(),
            stage: None,
            run_id: task.run_id,
            task_slug: Some(task.slug.clone()),
        })
}

fn mission_status(status: &str) -> String {
    match status {
        "active" => "running",
        "complete" => "done",
        "waiting" | "paused" | "stopped" => "waiting",
        _ => "waiting",
    }
    .to_string()
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn agent_cards(
    tasks: &[TaskView],
    runs: &[RunView],
    logs: &[NormEvent],
    log_agents: &[String],
    unreviewed: bool,
) -> Vec<AgentCard> {
    let active = runs
        .iter()
        .rev()
        .find(|run| matches!(run.status.as_str(), "active" | "waiting" | "paused"));
    AGENTS
        .iter()
        .map(|(id, name, role, readonly)| {
            let mine: Vec<&TaskView> = tasks
                .iter()
                .filter(|task| owner_is(id, task.claimed_by.as_deref()))
                .collect();
            let latest = logs
                .iter()
                .rev()
                .find(|event| event.agent == *id && event.kind != "transcript");
            let log_present = log_agents.iter().any(|agent| agent == id) || latest.is_some();
            let mut status = agent_status(id, &mine, active, latest);
            if *id == "codex" && unreviewed && status == "idle" {
                status = "waiting".to_string();
            }
            AgentCard {
                id: (*id).to_string(),
                name: (*name).to_string(),
                role: (*role).to_string(),
                readonly: *readonly,
                status,
                progress: progress(&mine),
                latest: latest.map(|event| event.summary.clone()),
                latest_ts: latest.map(|event| event.ts.clone()),
                log_present,
                claims: mine
                    .iter()
                    .filter(|task| task.status == "claimed")
                    .map(|task| task.slug.clone())
                    .collect(),
            }
        })
        .collect()
}

fn progress(mine: &[&TaskView]) -> Option<u8> {
    if mine.is_empty() {
        return None;
    }
    let steps: Vec<&StepView> = mine.iter().flat_map(|task| task.steps.iter()).collect();
    if !steps.is_empty() {
        let done = steps.iter().filter(|step| step.done).count();
        return Some(((done * 100) / steps.len()) as u8);
    }
    let done = mine.iter().filter(|task| task.status == "done").count();
    Some(((done * 100) / mine.len()) as u8)
}

fn agent_status(
    id: &str,
    mine: &[&TaskView],
    run: Option<&RunView>,
    latest: Option<&NormEvent>,
) -> String {
    if latest.is_some_and(|event| {
        event.kind == "error"
            || (event.kind == "gate" && event.detail.get("ok") == Some(&Value::Bool(false)))
    }) {
        return "error".to_string();
    }
    if mine.iter().any(|task| task.status == "claimed") {
        return "running".to_string();
    }
    if let Some(run) = run {
        if matches!(run.status.as_str(), "waiting" | "paused") && waiting_agent(id, &run.stage) {
            return "waiting".to_string();
        }
    }
    if !mine.is_empty() && mine.iter().all(|task| task.status == "done") {
        return "complete".to_string();
    }
    "idle".to_string()
}

fn waiting_agent(id: &str, stage: &str) -> bool {
    let stage = stage.to_ascii_lowercase();
    if stage.contains("review") {
        id == "codex"
    } else if stage.contains("plan") {
        id == "claude"
    } else {
        false
    }
}

fn owner_is(agent: &str, claimed_by: Option<&str>) -> bool {
    let Some(owner) = claimed_by else {
        return false;
    };
    let owner = owner.to_ascii_lowercase();
    match agent {
        "claude" => owner.contains("claude"),
        "cursor" => owner.contains("cursor"),
        "codex" => owner.contains("codex"),
        "graphify" => owner.contains("graphify"),
        "orca" => owner.contains("orca"),
        _ => false,
    }
}

fn artifact_refs(dump: &LedgerDump, tasks: &[TaskView]) -> Vec<ArtifactRef> {
    let mut refs = Vec::new();
    for report in &dump.reports {
        let slug = report
            .task_id
            .and_then(|id| tasks.iter().find(|task| task.id == id))
            .map(|task| task.slug.as_str())
            .unwrap_or("note");
        let verdict = report.verdict.as_deref().unwrap_or("unreviewed");
        refs.push(ArtifactRef {
            id: format!("report:{}", report.id),
            name: format!("{slug}.report.json"),
            kind: "report".to_string(),
            detail: format!("{verdict} · {} B", report.bytes),
            bytes: report.bytes.max(0) as u64,
            modified: report
                .reviewed_at
                .clone()
                .unwrap_or_else(|| report.created_at.clone()),
        });
    }
    for decision in &dump.decisions {
        let slug = tasks
            .iter()
            .find(|task| task.id == decision.task_id)
            .map(|task| task.slug.as_str())
            .unwrap_or("task");
        refs.push(ArtifactRef {
            id: format!("decision:{}", decision.id),
            name: format!("{slug}.decision.md"),
            kind: "decision".to_string(),
            detail: format!("decision · {}", decision.provenance),
            bytes: decision.rationale.len() as u64,
            modified: decision.created_at.clone(),
        });
    }
    refs
}

fn timeline(tasks: &[TaskView], runs: &[RunView], logs: &[NormEvent]) -> Vec<TimelineItem> {
    let mut items = Vec::new();
    let run = runs.iter().rev().find(|run| {
        matches!(
            run.status.as_str(),
            "active" | "waiting" | "paused" | "complete"
        )
    });
    let relevant: Vec<&TaskView> = if let Some(run) = run {
        let linked: Vec<&TaskView> = tasks
            .iter()
            .filter(|task| task.run_id == Some(run.id))
            .collect();
        if linked.is_empty() {
            tasks.iter().collect()
        } else {
            linked
        }
    } else {
        tasks.iter().collect()
    };
    if !relevant.is_empty() {
        let checks = relevant
            .iter()
            .map(|task| CheckItem {
                label: format!("{}  {}", task.slug, task.title),
                state: match task.status.as_str() {
                    "done" => "done",
                    "claimed" => "active",
                    _ => "pending",
                }
                .to_string(),
            })
            .collect();
        items.push(TimelineItem {
            id: "checks".to_string(),
            ts: relevant
                .iter()
                .map(|task| task.updated_at.as_str())
                .max()
                .unwrap_or("")
                .to_string(),
            actor: "Orca".to_string(),
            kind: "checklist".to_string(),
            text: "Delegated work".to_string(),
            detail: None,
            collapsible: false,
            checks,
        });
    }
    for event in logs {
        if event.agent != "claude" || event.kind == "transcript" {
            continue;
        }
        let kind = if event.kind == "thinking" {
            "thinking"
        } else {
            "message"
        };
        items.push(TimelineItem {
            id: format!("log-{}", event.seq),
            ts: event.ts.clone(),
            actor: "Claude".to_string(),
            kind: kind.to_string(),
            text: event.summary.clone(),
            detail: event
                .detail
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string),
            collapsible: kind == "thinking" || event.summary.chars().count() > 80,
            checks: Vec::new(),
        });
    }
    items.sort_by(|left, right| left.ts.cmp(&right.ts));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{StepRow, TaskRow};

    fn task(
        id: i64,
        slug: &str,
        status: &str,
        by: Option<&str>,
        steps: &[(i64, bool)],
    ) -> (TaskRow, Vec<StepRow>) {
        let row = TaskRow {
            id,
            slug: slug.to_string(),
            title: slug.to_string(),
            status: status.to_string(),
            claimed_by: by.map(str::to_string),
            created_at: "2026-11-17T00:00:00Z".to_string(),
            updated_at: "2026-11-17T00:00:00Z".to_string(),
            run_id: None,
        };
        let steps = steps
            .iter()
            .map(|(position, done)| StepRow {
                id: *position,
                task_id: id,
                position: *position,
                description: format!("step {position}"),
                done: *done,
            })
            .collect();
        (row, steps)
    }

    #[test]
    fn progress_hides_without_claims_and_uses_steps_when_present() {
        let (cursor, steps) = task(
            1,
            "adapter",
            "claimed",
            Some("cursor"),
            &[(1, true), (2, true), (3, false), (4, false), (5, false)],
        );
        let (claude, claude_steps) = task(2, "plan", "done", Some("claude-code"), &[(1, true)]);
        let mut dump = empty_dump();
        dump.tasks = vec![cursor, claude];
        dump.steps = steps.into_iter().chain(claude_steps).collect();
        let derived = project("demo", &dump, &[], &[], 0, &[]);
        let cursor = derived
            .workspace
            .agents
            .iter()
            .find(|agent| agent.id == "cursor")
            .unwrap();
        let codex = derived
            .workspace
            .agents
            .iter()
            .find(|agent| agent.id == "codex")
            .unwrap();
        let claude = derived
            .workspace
            .agents
            .iter()
            .find(|agent| agent.id == "claude")
            .unwrap();
        assert_eq!(cursor.status, "running");
        assert_eq!(cursor.progress, Some(40));
        assert_eq!(claude.status, "complete");
        assert_eq!(claude.progress, Some(100));
        assert_eq!(codex.status, "idle");
        assert_eq!(codex.progress, None);
    }

    #[test]
    fn codex_card_uses_the_review_and_skips_the_run_transcript() {
        let transcript = NormEvent {
            seq: 2,
            ts: "2026-11-17T18:00:00Z".to_string(),
            agent: "codex".to_string(),
            kind: "transcript".to_string(),
            summary: "m1 · run transcript".to_string(),
            detail: serde_json::json!({ "text": "bash -lc 'cargo test'" }),
            source: "codex-m1-run.log".to_string(),
        };
        let review = NormEvent {
            seq: 1,
            ts: "2026-11-17T17:00:00Z".to_string(),
            agent: "codex".to_string(),
            kind: "review".to_string(),
            summary: "m1 · Verdict: accept".to_string(),
            detail: serde_json::json!({ "text": "The copy does not write." }),
            source: "m1-codex-r1.md".to_string(),
        };
        let derived = project(
            "demo",
            &empty_dump(),
            &[review, transcript],
            &["codex".to_string()],
            0,
            &[],
        );
        let codex = derived
            .workspace
            .agents
            .iter()
            .find(|agent| agent.id == "codex")
            .unwrap();
        assert_eq!(codex.latest.as_deref(), Some("m1 · Verdict: accept"));
        assert!(derived
            .workspace
            .timeline
            .iter()
            .all(|item| item.actor != "Codex"));
    }

    fn empty_dump() -> LedgerDump {
        LedgerDump {
            schema_version: 6,
            degraded: false,
            tasks: Vec::new(),
            steps: Vec::new(),
            decisions: Vec::new(),
            deps: Vec::new(),
            events: Vec::new(),
            reports: Vec::new(),
            settings: Vec::new(),
            runs: Vec::new(),
        }
    }
}
