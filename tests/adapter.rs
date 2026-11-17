use std::fs;

use rusqlite::{params, Connection};

use relaybook_dashboard::derive::project;
use relaybook_dashboard::ledger::{
    apply_historical_migrations, fixture_current, read_ledger, SUPPORTED_SCHEMA_VERSION,
};

#[test]
fn a_v1_ledger_migrated_with_the_real_scripts_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    {
        let conn = Connection::open(&path).unwrap();
        apply_historical_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO tasks (slug, title, status, claimed_by, created_at, updated_at, run_id)
             VALUES ('auth', 'Add auth', 'claimed', 'cursor', '2026-11-17T00:00:00Z', '2026-11-17T00:01:00Z', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO steps (task_id, position, description, done) VALUES (1, 1, 'write the gate', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO decisions (task_id, rationale, provenance, created_at)
             VALUES (1, 'keep the slug', 'claude-code', '2026-11-17T00:00:01Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events (ts, kind, payload) VALUES ('2026-11-17T00:00:02Z', 'claimed', '{\"slug\":\"auth\",\"by\":\"cursor\"}')",
            [],
        )
        .unwrap();
        let body = r#"{"changed":["added the gate"],"verified":["cargo test"],"not_verified":[],"deviations":[],"review":[]}"#;
        conn.execute(
            "INSERT INTO reports (task_id, kind, author, body, bytes, created_at, reviewed_at, verdict, review_note)
             VALUES (1, 'task', 'cursor', ?1, ?2, '2026-11-17T00:02:00Z', '2026-11-17T00:03:00Z', 'accepted', 'looks right')",
            params![body, body.len() as i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value, updated_at, updated_by) VALUES ('mode', 'auto', '2026-11-17T00:00:00Z', 'cli')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO runs (prd_path, prd_sha256, branch, stage, cycle, agent_calls, est_cost_usd, status, started_at, failure_streak, current_slug, complete_requested)
             VALUES ('docs/prd.md', 'abc', 'watch-only', 'execute', 2, 4, 0.5, 'active', '2026-11-17T00:00:00Z', 1, 'auth', 0)",
            [],
        )
        .unwrap();
    }
    let before = fs::read(&path).unwrap();
    let dump = read_ledger(&path).expect("read");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(dump.schema_version, SUPPORTED_SCHEMA_VERSION);
    assert_eq!(dump.tasks[0].slug, "auth");
    assert_eq!(dump.tasks[0].status, "claimed");
    assert_eq!(dump.tasks[0].claimed_by.as_deref(), Some("cursor"));
    assert_eq!(dump.tasks[0].run_id, Some(1));
    assert!(dump.steps[0].done);
    assert_eq!(dump.decisions[0].rationale, "keep the slug");
    assert_eq!(dump.events[0].kind, "claimed");
    assert_eq!(dump.reports[0].verdict.as_deref(), Some("accepted"));
    assert_eq!(dump.reports[0].review_note.as_deref(), Some("looks right"));
    assert_eq!(dump.settings[0].key, "mode");
    assert_eq!(dump.runs[0].stage, "execute");
    assert_eq!(dump.runs[0].failure_streak, 1);
    assert_eq!(dump.runs[0].current_slug.as_deref(), Some("auth"));
    assert!(!dump.runs[0].complete_requested);
}

#[test]
fn a_new_ledger_uses_the_current_schema_constant() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    {
        let conn = Connection::open(&path).unwrap();
        fixture_current(&conn).unwrap();
        conn.execute(
            "INSERT INTO tasks (slug, title, status, created_at, updated_at) VALUES ('greet', 'Say hello', 'open', '2026-11-17T00:00:00Z', '2026-11-17T00:00:00Z')",
            [],
        )
        .unwrap();
    }
    let dump = read_ledger(&path).unwrap();
    assert_eq!(dump.tasks[0].title, "Say hello");
    assert!(dump.tasks[0].claimed_by.is_none());
}

#[test]
fn unknown_schema_versions_read_tasks_and_events_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE tasks (
                extra_note TEXT,
                title TEXT NOT NULL,
                id INTEGER PRIMARY KEY,
                slug TEXT NOT NULL,
                status TEXT,
                claimed_by TEXT
            );
            CREATE TABLE events (
                payload TEXT,
                kind TEXT NOT NULL,
                id INTEGER,
                ts TEXT
            );
            CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                body TEXT NOT NULL
            );
            INSERT INTO tasks (extra_note, title, id, slug, status, claimed_by)
            VALUES ('ignore-me', 'Ship the next schema', 4, 'ship-v8', 'claimed', 'cursor');
            INSERT INTO events (payload, kind, id, ts)
            VALUES ('{\"slug\":\"ship-v8\"}', 'task_created', 9, '2026-11-17T00:00:00Z');
            INSERT INTO reports (body) VALUES ('must stay hidden');
            ",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 8).unwrap();
    }
    let before = fs::read(&path).unwrap();
    let dump = read_ledger(&path).expect("degraded read");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(dump.degraded);
    assert_eq!(dump.schema_version, 8);
    assert!(dump.steps.is_empty());
    assert!(dump.reports.is_empty());
    assert!(dump.runs.is_empty());
    assert!(dump.settings.is_empty());
    assert_eq!(dump.tasks.len(), 1);
    assert_eq!(dump.tasks[0].id, 4);
    assert_eq!(dump.tasks[0].slug, "ship-v8");
    assert_eq!(dump.tasks[0].title, "Ship the next schema");
    assert_eq!(dump.tasks[0].status, "claimed");
    assert_eq!(dump.tasks[0].claimed_by.as_deref(), Some("cursor"));
    assert!(dump.tasks[0].run_id.is_none());
    assert_eq!(dump.events[0].id, 9);
    assert_eq!(dump.events[0].kind, "task_created");
    assert_eq!(dump.events[0].payload["slug"], "ship-v8");

    let view = project("ledger", &dump, &[], &[], 0, &[]);
    assert_eq!(
        view.workspace.schema_warning.as_deref(),
        Some("unsupported schema v8")
    );
    assert!(view.workspace.ledger_ok);
    assert!(!view.workspace.health_ok);
    assert_eq!(view.workspace.tasks[0].slug, "ship-v8");
    assert_eq!(
        view.workspace
            .agents
            .iter()
            .find(|agent| agent.id == "cursor")
            .unwrap()
            .status,
        "running"
    );
}

#[test]
fn schema_v7_keeps_recognized_columns_and_skips_new_ones() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE tasks (
                id INTEGER PRIMARY KEY,
                slug TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                claimed_by TEXT,
                created_at TEXT,
                updated_at TEXT,
                priority TEXT
            );
            CREATE TABLE events (
                id INTEGER PRIMARY KEY,
                ts TEXT,
                kind TEXT,
                payload TEXT
            );
            INSERT INTO tasks (slug, title, status, claimed_by, created_at, updated_at, priority)
            VALUES ('adapt', 'Adapt the reader', 'open', NULL, '2026-11-17T01:00:00Z', '2026-11-17T01:00:00Z', 'high');
            INSERT INTO events (ts, kind, payload)
            VALUES ('2026-11-17T01:00:00Z', 'task_created', 'not-json');
            ",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();
    }
    let dump = read_ledger(&path).unwrap();
    assert!(dump.degraded);
    assert_eq!(dump.schema_version, 7);
    assert_eq!(dump.tasks[0].slug, "adapt");
    assert_eq!(dump.tasks[0].created_at, "2026-11-17T01:00:00Z");
    assert!(dump.tasks[0].claimed_by.is_none());
    assert_eq!(dump.events[0].kind, "task_created");
    assert_eq!(dump.events[0].payload, "not-json");
    let view = project("ledger", &dump, &[], &[], 0, &[]);
    assert_eq!(
        view.workspace.schema_warning.as_deref(),
        Some("unsupported schema v7")
    );
    assert_eq!(view.workspace.health_label, "unsupported schema v7");
}
