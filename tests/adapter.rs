use std::fs;

use rusqlite::{params, Connection};

use relaybook_dashboard::ledger::{
    apply_historical_migrations, fixture_current, read_ledger, LedgerError,
    SUPPORTED_SCHEMA_VERSION,
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
fn a_future_schema_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    {
        let conn = Connection::open(&path).unwrap();
        fixture_current(&conn).unwrap();
        conn.pragma_update(None, "user_version", 8).unwrap();
    }
    let err = read_ledger(&path).unwrap_err();
    match err {
        LedgerError::UnsupportedSchema { found, supported } => {
            assert_eq!(found, 8);
            assert_eq!(supported, 6);
        }
        other => panic!("unexpected {other}"),
    }
}
