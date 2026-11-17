use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::ledger::fixture_current as apply_current_schema;
use crate::timefmt::format_unix;

pub struct Demo {
    pub root: PathBuf,
    pub ledger_dir: PathBuf,
    pub context_dir: PathBuf,
    pub artifacts_dir: PathBuf,
}

pub fn prepare() -> anyhow::Result<Demo> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "relaybook-dashboard-demo-{}-{nanos}",
        std::process::id()
    ));
    let ledger_dir = root.join("ledgers");
    let context_dir = root.join("context");
    let artifacts_dir = context_dir.join("artifacts");
    fs::create_dir_all(ledger_dir.join("orchestrator"))?;
    fs::create_dir_all(ledger_dir.join("archive"))?;
    fs::create_dir_all(&artifacts_dir)?;
    seed_active(&ledger_dir.join("orchestrator").join("ledger.db"))?;
    seed_archive(&ledger_dir.join("archive").join("ledger.db"))?;
    write_initial_logs(&context_dir)?;
    fs::write(artifacts_dir.join("plan.md"), PLAN_MD)?;
    fs::write(artifacts_dir.join("adapter.rs"), ADAPTER_RS)?;
    Ok(Demo {
        root,
        ledger_dir,
        context_dir,
        artifacts_dir,
    })
}

pub fn spawn_replay(context_dir: PathBuf) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(45)).await;
        let script = replay_lines();
        let mut index = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let (file, line) = &script[index % script.len()];
            let path = context_dir.join(file);
            if let Ok(mut handle) = OpenOptions::new().create(true).append(true).open(&path) {
                let stamped = line.replace("{{ts}}", &now());
                let _ = writeln!(handle, "{stamped}");
            }
            index += 1;
        }
    });
}

fn seed_active(path: &Path) -> anyhow::Result<()> {
    let conn = Connection::open(path)?;
    conn.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
        true,
    )?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")?;
    apply_current_schema(&conn)?;
    let t0 = ago(900);
    let t1 = ago(600);
    let t2 = ago(120);
    conn.execute(
        "INSERT INTO runs (prd_path, prd_sha256, branch, stage, cycle, agent_calls, est_cost_usd, status, started_at, failure_streak, current_slug, complete_requested)
         VALUES ('docs/prd.md', 'demo', 'watch-only', 'execute', 2, 6, 1.4, 'active', ?1, 0, 'ledger-adapter', 0)",
        params![t0],
    )?;
    insert_task(
        &conn,
        "plan-dashboard",
        "Plan the watch-only command center",
        "done",
        Some("claude-code"),
        Some(1),
        &t0,
        &t1,
    )?;
    insert_task(
        &conn,
        "ledger-adapter",
        "Read the ledger without writing it",
        "claimed",
        Some("cursor"),
        Some(1),
        &t0,
        &t2,
    )?;
    insert_task(
        &conn,
        "log-parsers",
        "Normalize Claude, Cursor, and Codex logs",
        "open",
        None,
        Some(1),
        &t0,
        &t0,
    )?;
    insert_task(
        &conn,
        "review-adapter",
        "Review the read-only adapter",
        "open",
        None,
        Some(1),
        &t0,
        &t0,
    )?;
    insert_task(
        &conn,
        "token-budget",
        "Record the context budget",
        "done",
        Some("cursor"),
        Some(1),
        &t0,
        &t1,
    )?;
    steps(
        &conn,
        1,
        &[
            ("Write the objective", true),
            ("Name the agents", true),
            ("Stop before implementation", true),
        ],
    )?;
    steps(
        &conn,
        2,
        &[
            ("Open the database read-only", true),
            ("Check user_version", true),
            ("Copy the WAL aside", true),
            ("Map claims to agent cards", false),
            ("Keep the poll loop quiet on a torn read", false),
        ],
    )?;
    conn.execute(
        "INSERT INTO task_deps (task_id, depends_on_id, created_at) VALUES (3, 2, ?1), (4, 2, ?1)",
        params![t0],
    )?;
    conn.execute(
        "INSERT INTO decisions (task_id, rationale, provenance, created_at) VALUES (1, 'The dashboard reads Relaybook. It does not keep a second store.', 'claude-code', ?1)",
        params![t0],
    )?;
    let accepted = r#"{"changed":["Planned the command center"],"verified":["Ledger tables match schema v6"],"not_verified":[],"deviations":[],"review":["Keep the UI watch-only"]}"#;
    conn.execute(
        "INSERT INTO reports (task_id, kind, author, body, bytes, created_at, reviewed_at, verdict) VALUES (1, 'task', 'cursor', ?1, ?2, ?3, ?4, 'accepted')",
        params![accepted, accepted.len() as i64, t1, t1],
    )?;
    let pending = r#"{"changed":["Noted the context budget"],"verified":["Counted the planner block"],"not_verified":["A full auto run"],"deviations":[],"review":["Codex still owes a pass"]}"#;
    conn.execute(
        "INSERT INTO reports (task_id, kind, author, body, bytes, created_at) VALUES (5, 'task', 'cursor', ?1, ?2, ?3)",
        params![pending, pending.len() as i64, t2],
    )?;
    for (key, value) in [
        ("mode", "auto"),
        ("gates", "plan,review"),
        ("max_cycles", "20"),
        ("max_agent_calls", "60"),
    ] {
        conn.execute(
            "INSERT INTO settings (key, value, updated_at, updated_by) VALUES (?1, ?2, ?3, 'cli')",
            params![key, value, t0],
        )?;
    }
    conn.execute(
        "INSERT INTO events (ts, kind, payload) VALUES (?1, 'task_created', '{\"slug\":\"plan-dashboard\",\"title\":\"Plan the watch-only command center\"}')",
        params![t0],
    )?;
    conn.execute(
        "INSERT INTO events (ts, kind, payload) VALUES (?1, 'claimed', '{\"slug\":\"ledger-adapter\",\"by\":\"cursor\"}')",
        params![t2],
    )?;
    conn.execute(
        "INSERT INTO events (ts, kind, payload) VALUES (?1, 'run_linked', '{\"slug\":\"ledger-adapter\",\"run_id\":1}')",
        params![t0],
    )?;
    Ok(())
}

fn seed_archive(path: &Path) -> anyhow::Result<()> {
    let conn = Connection::open(path)?;
    apply_current_schema(&conn)?;
    let t0 = ago(86_400);
    conn.execute(
        "INSERT INTO runs (prd_path, prd_sha256, branch, stage, cycle, agent_calls, est_cost_usd, status, started_at, failure_streak, current_slug, complete_requested)
         VALUES ('docs/archive.md', 'demo', 'archive', 'done', 1, 3, 0.4, 'complete', ?1, 0, 'ship-note', 1)",
        params![t0],
    )?;
    insert_task(
        &conn,
        "ship-note",
        "Ship the archive note",
        "done",
        Some("cursor"),
        Some(1),
        &t0,
        &t0,
    )?;
    steps(&conn, 1, &[("Write the note", true)])?;
    Ok(())
}

fn insert_task(
    conn: &Connection,
    slug: &str,
    title: &str,
    status: &str,
    by: Option<&str>,
    run_id: Option<i64>,
    created: &str,
    updated: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO tasks (slug, title, status, claimed_by, created_at, updated_at, run_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![slug, title, status, by, created, updated, run_id],
    )?;
    Ok(())
}

fn steps(conn: &Connection, task_id: i64, items: &[(&str, bool)]) -> anyhow::Result<()> {
    for (index, (text, done)) in items.iter().enumerate() {
        conn.execute(
            "INSERT INTO steps (task_id, position, description, done) VALUES (?1, ?2, ?3, ?4)",
            params![task_id, (index as i64) + 1, text, i64::from(*done)],
        )?;
    }
    Ok(())
}

fn write_initial_logs(context: &Path) -> anyhow::Result<()> {
    fs::write(context.join("orca-orchestrator.jsonl"), orca_log())?;
    fs::write(context.join("claude-planner.jsonl"), claude_log())?;
    fs::write(context.join("cursor-builder.jsonl"), cursor_log())?;
    fs::write(context.join("codex-reviewer.jsonl"), codex_log())?;
    fs::write(context.join("graphify-context.jsonl"), graphify_log())?;
    Ok(())
}

fn orca_log() -> String {
    format!(
        "{}\n{}\n",
        line(
            "orca",
            ago(800),
            "message",
            "Objective: ship a watch-only view of the ledger. Cursor implements. Codex only reads."
        ),
        line(
            "orca",
            ago(500),
            "status",
            "Delegated ledger-adapter to Cursor. Holding review until the diff is quiet."
        ),
    )
}

fn claude_log() -> String {
    format!(
        "{}\n{}\n{}\n",
        json_claude(
            &ago(700),
            "thinking",
            "The ledger is the source of truth. A second store would drift."
        ),
        json_claude(
            &ago(680),
            "text",
            "Plan is in the ledger. Cursor has the adapter. I am not claiming the build."
        ),
        format!(
            r#"{{"type":"result","timestamp":"{}","subtype":"success","is_error":false,"result":"Plan accepted."}}"#,
            ago(660)
        ),
    )
}

fn cursor_log() -> String {
    format!(
        "{}\n{}\n",
        format!(
            r#"{{"type":"assistant","timestamp":"{}","message":{{"content":[{{"type":"text","text":"Opening the ledger read-only and checking user_version."}}]}}}}"#,
            ago(90)
        ),
        format!(
            r#"{{"type":"tool_call","timestamp":"{}","subtype":"started","tool_call":{{"shellToolCall":{{"args":{{"command":"cargo test --test adapter"}}}}}}}}"#,
            ago(40)
        ),
    )
}

fn codex_log() -> String {
    format!(
        "{}\n",
        format!(
            r#"{{"type":"item.completed","timestamp":"{}","item":{{"type":"reasoning","text":"The copy must not write the source directory. I will read the diff only."}}}}"#,
            ago(300)
        )
    )
}

fn graphify_log() -> String {
    format!(
        "{}\n",
        format!(
            r#"{{"type":"query","timestamp":"{}","query":"schema adapter","summary":"schema.rs owns SCHEMA_VERSION"}}"#,
            ago(200)
        )
    )
}

fn json_claude(ts: &str, kind: &str, text: &str) -> String {
    if kind == "thinking" {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"content":[{{"type":"thinking","thinking":"{text}"}}]}}}}"#
        )
    } else {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"content":[{{"type":"text","text":"{text}"}}]}}}}"#
        )
    }
}

fn line(kind: &str, ts: String, event_type: &str, summary: &str) -> String {
    let _ = kind;
    serde_json::json!({
        "type": event_type,
        "timestamp": ts,
        "summary": summary,
    })
    .to_string()
}

fn replay_lines() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "cursor-builder.jsonl",
            r#"{"type":"assistant","timestamp":"{{ts}}","message":{"content":[{"type":"text","text":"Reading the next migration edge."}]}}"#,
        ),
        (
            "cursor-builder.jsonl",
            r#"{"type":"tool_call","timestamp":"{{ts}}","subtype":"completed","tool_call":{"shellToolCall":{"args":{"command":"cargo test --test adapter"},"result":{"output":"ok"}}}}"#,
        ),
        (
            "graphify-context.jsonl",
            r#"{"type":"status","timestamp":"{{ts}}","summary":"Index is current. No new files."}"#,
        ),
        (
            "codex-reviewer.jsonl",
            r#"{"type":"item.completed","timestamp":"{{ts}}","item":{"type":"agent_message","text":"Still waiting on the unreviewed report. No files edited."}}"#,
        ),
    ]
}

fn ago(secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_unix(now - secs as i64)
}

fn now() -> String {
    ago(0)
}

const PLAN_MD: &str = "# Watch-only command center\n\nOne objective. The ledger stays the source of truth.\n\n- Claude plans.\n- Cursor implements.\n- Codex reviews and does not edit.\n- Graphify answers where the code lives.\n\nNothing in this view writes the ledger.\n";

const ADAPTER_RS: &str = "// The reader copies the database and the WAL into a private temp directory.\n// mode=ro and query_only stay on. immutable stays off.\npub fn read_ledger(source: &Path) -> Result<LedgerDump, LedgerError> {\n    with_readonly_copy(source, dump_v6)\n}\n";
