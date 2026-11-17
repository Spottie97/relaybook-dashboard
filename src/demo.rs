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
        let mut index = 0u64;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = replay_tick(&context_dir, index);
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
    let claude_ts = ago(700);
    write_result(
        context,
        "claude-rev3-run.json",
        &claude_ts,
        "18",
        "Plan accepted. The ledger stays the source of truth, and Cursor implements the read-only adapter.",
        18_200,
        4,
        0.18,
        "demo-claude-rev3",
    )?;
    fs::write(
        context.join("claude-rev3-run.stderr"),
        "note: sidecar, not the summary\n",
    )?;
    let cursor_ts = ago(90);
    write_result(
        context,
        "m1-cursor-impl.json",
        &cursor_ts,
        "40",
        "PY",
        40_000,
        6,
        0.42,
        "demo-cursor-m1",
    )?;
    fs::write(
        context.join("m1-cursor-impl.result.md"),
        "Opened the ledger read-only and checked user_version. The WAL is copied aside before the read.\n",
    )?;
    fs::write(
        context.join("m1-cursor-impl-prompt.md"),
        "Implement the read-only adapter. Do not write the ledger.\n",
    )?;
    let fix_ts = ago(400);
    write_result(
        context,
        "r2-cursor-fix1.json",
        &fix_ts,
        "12",
        "Fixed the torn read on the copied database.",
        12_000,
        2,
        0.08,
        "demo-cursor-r2",
    )?;
    fs::write(
        context.join("r2-cursor-fix1.result.md"),
        "Fixed the torn read on the copied database.\n",
    )?;
    let review = "# Verdict: accept\n\nThe copy does not write the source directory. No files were edited.\n";
    fs::write(context.join("m1-codex-r1.md"), review)?;
    write_meta(context, "m1-codex-r1", &ago(30), "8", "0")?;
    fs::write(
        context.join("codex-m1-run.log"),
        "exec\nbash -lc 'cargo test'\nreading the diff\n",
    )?;
    write_meta(context, "codex-m1-run", &ago(120), "20", "0")?;
    fs::write(
        context.join("m1-graphify-postcommit.log"),
        "post-commit index updated\n",
    )?;
    fs::write(
        context.join("m1-graphify-update.log"),
        "scanning\n\n---\n\nschema.rs owns SCHEMA_VERSION\nindex refreshed\n",
    )?;
    fs::write(
        context.join("m1-cargo-test.log"),
        "running 4 tests\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
    )?;
    write_meta(context, "m1-cargo-test", &ago(200), "6", "0")?;
    fs::write(context.join("m1-verify.log"), "fmt check ok\n")?;
    write_meta(context, "m1-verify", &ago(210), "2", "0")?;
    Ok(())
}

fn write_result(
    context: &Path,
    name: &str,
    end_utc: &str,
    elapsed_s: &str,
    result: &str,
    duration_ms: u64,
    num_turns: u64,
    cost: f64,
    session_id: &str,
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": result,
        "duration_ms": duration_ms,
        "num_turns": num_turns,
        "total_cost_usd": cost,
        "session_id": session_id,
        "usage": {"input_tokens": 800, "output_tokens": 200}
    });
    fs::write(context.join(name), serde_json::to_string_pretty(&body)?)?;
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    write_meta(context, stem, end_utc, elapsed_s, "0")?;
    Ok(())
}

fn write_meta(
    context: &Path,
    stem: &str,
    end_utc: &str,
    elapsed_s: &str,
    exit: &str,
) -> anyhow::Result<()> {
    fs::write(
        context.join(format!("{stem}.meta")),
        format!("exit={exit}\nend_utc={end_utc}\nelapsed_s={elapsed_s}\n"),
    )?;
    Ok(())
}

fn replay_tick(dir: &Path, index: u64) -> std::io::Result<()> {
    match index % 4 {
        0 => {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("m1-graphify-update.log"))?;
            writeln!(file, "index touched {index}")?;
        }
        1 => {
            fs::write(
                dir.join("m1-cursor-fix2.json"),
                "{\"type\":\"result\",\"result\":",
            )?;
        }
        2 => {
            let ts = now();
            let body = serde_json::json!({
                "type": "result",
                "subtype": "success",
                "is_error": false,
                "result": "Fix applied. The read stays on the copied database.",
                "duration_ms": 2200,
                "num_turns": 1,
                "total_cost_usd": 0.04,
                "session_id": "demo-cursor-fix2"
            });
            fs::write(dir.join("m1-cursor-fix2.json"), body.to_string())?;
            fs::write(
                dir.join("m1-cursor-fix2.meta"),
                format!("exit=0\nend_utc={ts}\nelapsed_s=2\n"),
            )?;
            fs::write(
                dir.join("m1-cursor-fix2.result.md"),
                "Fix applied. The read stays on the copied database.\n",
            )?;
        }
        _ => {
            let ts = now();
            fs::write(
                dir.join("m1-codex-r2.md"),
                "# Verdict: accept\n\nSecond pass. Still read-only. No files edited.\n",
            )?;
            fs::write(
                dir.join("m1-codex-r2.meta"),
                format!("exit=0\nend_utc={ts}\nelapsed_s=5\n"),
            )?;
        }
    }
    Ok(())
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
