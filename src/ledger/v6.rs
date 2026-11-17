use rusqlite::Connection;

use super::{
    DecisionRow, DepRow, EventRow, LedgerDump, LedgerError, ReportRow, RunRow, SettingRow, StepRow,
    TaskRow,
};

pub fn dump(conn: &Connection) -> Result<LedgerDump, LedgerError> {
    Ok(LedgerDump {
        schema_version: 6,
        degraded: false,
        tasks: tasks(conn)?,
        steps: steps(conn)?,
        decisions: decisions(conn)?,
        deps: deps(conn)?,
        events: events(conn)?,
        reports: reports(conn)?,
        settings: settings(conn)?,
        runs: runs(conn)?,
    })
}

fn tasks(conn: &Connection) -> Result<Vec<TaskRow>, LedgerError> {
    let mut statement = conn
        .prepare(
            "SELECT id, slug, title, status, claimed_by, created_at, updated_at, run_id
             FROM tasks ORDER BY id",
        )
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok(TaskRow {
                id: row.get(0)?,
                slug: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                claimed_by: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                run_id: row.get(7)?,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn steps(conn: &Connection) -> Result<Vec<StepRow>, LedgerError> {
    let mut statement = conn
        .prepare(
            "SELECT id, task_id, position, description, done
             FROM steps ORDER BY task_id, position",
        )
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            let done: i64 = row.get(4)?;
            Ok(StepRow {
                id: row.get(0)?,
                task_id: row.get(1)?,
                position: row.get(2)?,
                description: row.get(3)?,
                done: done != 0,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn decisions(conn: &Connection) -> Result<Vec<DecisionRow>, LedgerError> {
    let mut statement = conn
        .prepare(
            "SELECT id, task_id, rationale, provenance, created_at
             FROM decisions ORDER BY id",
        )
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok(DecisionRow {
                id: row.get(0)?,
                task_id: row.get(1)?,
                rationale: row.get(2)?,
                provenance: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn deps(conn: &Connection) -> Result<Vec<DepRow>, LedgerError> {
    let mut statement = conn
        .prepare("SELECT task_id, depends_on_id, created_at FROM task_deps ORDER BY task_id")
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok(DepRow {
                task_id: row.get(0)?,
                depends_on_id: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn events(conn: &Connection) -> Result<Vec<EventRow>, LedgerError> {
    let mut statement = conn
        .prepare("SELECT id, ts, kind, payload FROM events ORDER BY id")
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            let payload: String = row.get(3)?;
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, payload))
        })
        .map_err(map_err)?;
    let mut events = Vec::new();
    for row in rows {
        let (id, ts, kind, payload) = row.map_err(map_err)?;
        let parsed = serde_json::from_str(&payload).unwrap_or(serde_json::Value::String(payload));
        events.push(EventRow {
            id,
            ts,
            kind,
            payload: parsed,
        });
    }
    Ok(events)
}

fn reports(conn: &Connection) -> Result<Vec<ReportRow>, LedgerError> {
    let mut statement = conn
        .prepare(
            "SELECT id, task_id, kind, author, body, bytes, created_at, reviewed_at, verdict, review_note
             FROM reports ORDER BY id",
        )
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ReportRow {
                id: row.get(0)?,
                task_id: row.get(1)?,
                kind: row.get(2)?,
                author: row.get(3)?,
                body: row.get(4)?,
                bytes: row.get(5)?,
                created_at: row.get(6)?,
                reviewed_at: row.get(7)?,
                verdict: row.get(8)?,
                review_note: row.get(9)?,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn settings(conn: &Connection) -> Result<Vec<SettingRow>, LedgerError> {
    let mut statement = conn
        .prepare("SELECT key, value, updated_at, updated_by FROM settings ORDER BY key")
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok(SettingRow {
                key: row.get(0)?,
                value: row.get(1)?,
                updated_at: row.get(2)?,
                updated_by: row.get(3)?,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn runs(conn: &Connection) -> Result<Vec<RunRow>, LedgerError> {
    let mut statement = conn
        .prepare(
            "SELECT id, prd_path, prd_sha256, branch, stage, cycle, agent_calls, est_cost_usd,
                    status, stop_reason, started_at, failure_streak, current_slug, complete_requested
             FROM runs ORDER BY id",
        )
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            let complete: i64 = row.get(13)?;
            Ok(RunRow {
                id: row.get(0)?,
                prd_path: row.get(1)?,
                prd_sha256: row.get(2)?,
                branch: row.get(3)?,
                stage: row.get(4)?,
                cycle: row.get(5)?,
                agent_calls: row.get(6)?,
                est_cost_usd: row.get(7)?,
                status: row.get(8)?,
                stop_reason: row.get(9)?,
                started_at: row.get(10)?,
                failure_streak: row.get(11)?,
                current_slug: row.get(12)?,
                complete_requested: complete != 0,
            })
        })
        .map_err(map_err)?;
    collect(rows)
}

fn collect<T>(
    rows: impl IntoIterator<Item = Result<T, rusqlite::Error>>,
) -> Result<Vec<T>, LedgerError> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_err)?);
    }
    Ok(out)
}

fn map_err(err: rusqlite::Error) -> LedgerError {
    match err {
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::DatabaseBusy
                || code.code == rusqlite::ErrorCode::DatabaseLocked =>
        {
            LedgerError::Busy
        }
        other => LedgerError::Sqlite(format!("code {}", sqlite_code(&other))),
    }
}

fn sqlite_code(err: &rusqlite::Error) -> i32 {
    match err {
        rusqlite::Error::SqliteFailure(code, _) => code.extended_code,
        _ => 0,
    }
}
