use rusqlite::Connection;

use super::{EventRow, LedgerDump, LedgerError, TaskRow};

const TASK_COLUMNS: &[&str] = &[
    "id",
    "slug",
    "title",
    "status",
    "claimed_by",
    "created_at",
    "updated_at",
    "run_id",
];

const EVENT_COLUMNS: &[&str] = &["id", "ts", "kind", "payload"];

/// Best-effort read for a `user_version` this build does not map fully.
///
/// Only `tasks` and `events` are queried. Columns come from `PRAGMA table_info`,
/// so a renamed or extra column does not hide the ones we still recognize.
/// Steps, reports, runs, and the other tables stay empty until a real adapter
/// is added next to `v6`.
pub fn dump(conn: &Connection, version: i64) -> Result<LedgerDump, LedgerError> {
    Ok(LedgerDump {
        schema_version: version,
        degraded: true,
        tasks: tasks(conn)?,
        steps: Vec::new(),
        decisions: Vec::new(),
        deps: Vec::new(),
        events: events(conn)?,
        reports: Vec::new(),
        settings: Vec::new(),
        runs: Vec::new(),
    })
}

pub fn banner(version: i64) -> String {
    format!("unsupported schema v{version}")
}

fn tasks(conn: &Connection) -> Result<Vec<TaskRow>, LedgerError> {
    let selected = present(conn, "tasks", TASK_COLUMNS)?;
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {} FROM \"tasks\"",
        selected
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut statement = conn.prepare(&sql).map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            let mut values = Vec::new();
            for index in 0..selected.len() {
                values.push(row.get::<_, rusqlite::types::Value>(index)?);
            }
            Ok(values)
        })
        .map_err(map_err)?;
    let mut tasks = Vec::new();
    for (fallback_id, row) in rows.enumerate() {
        let values = row.map_err(map_err)?;
        let cell = |name: &str| value_at(&selected, &values, name);
        let id = cell("id")
            .and_then(as_i64)
            .unwrap_or(fallback_id as i64 + 1);
        let slug = cell("slug")
            .and_then(as_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| format!("task-{id}"));
        let title = cell("title")
            .and_then(as_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| slug.clone());
        let status = cell("status")
            .and_then(as_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "open".to_string());
        tasks.push(TaskRow {
            id,
            slug,
            title,
            status,
            claimed_by: cell("claimed_by")
                .and_then(as_text)
                .filter(|text| !text.is_empty()),
            created_at: cell("created_at").and_then(as_text).unwrap_or_default(),
            updated_at: cell("updated_at").and_then(as_text).unwrap_or_default(),
            run_id: cell("run_id").and_then(as_i64),
        });
    }
    Ok(tasks)
}

fn events(conn: &Connection) -> Result<Vec<EventRow>, LedgerError> {
    let selected = present(conn, "events", EVENT_COLUMNS)?;
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {} FROM \"events\"",
        selected
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut statement = conn.prepare(&sql).map_err(map_err)?;
    let rows = statement
        .query_map([], |row| {
            let mut values = Vec::new();
            for index in 0..selected.len() {
                values.push(row.get::<_, rusqlite::types::Value>(index)?);
            }
            Ok(values)
        })
        .map_err(map_err)?;
    let mut events = Vec::new();
    for (fallback_id, row) in rows.enumerate() {
        let values = row.map_err(map_err)?;
        let cell = |name: &str| value_at(&selected, &values, name);
        let payload = match cell("payload").and_then(as_text) {
            Some(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
            None => serde_json::Value::Null,
        };
        events.push(EventRow {
            id: cell("id")
                .and_then(as_i64)
                .unwrap_or(fallback_id as i64 + 1),
            ts: cell("ts").and_then(as_text).unwrap_or_default(),
            kind: cell("kind")
                .and_then(as_text)
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            payload,
        });
    }
    Ok(events)
}

fn present(conn: &Connection, table: &str, wanted: &[&str]) -> Result<Vec<String>, LedgerError> {
    let known = column_names(conn, table)?;
    Ok(wanted
        .iter()
        .filter(|name| known.iter().any(|column| column == *name))
        .map(|name| (*name).to_string())
        .collect())
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, LedgerError> {
    // `table` is one of the two literals above. Quote it anyway.
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .map_err(map_err)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(map_err)?;
    let mut names = Vec::new();
    for row in rows {
        let name = row.map_err(map_err)?;
        if name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !name.is_empty()
        {
            names.push(name);
        }
    }
    Ok(names)
}

fn value_at<'a>(
    selected: &[String],
    values: &'a [rusqlite::types::Value],
    name: &str,
) -> Option<&'a rusqlite::types::Value> {
    selected
        .iter()
        .position(|column| column == name)
        .and_then(|index| values.get(index))
}

fn as_text(value: &rusqlite::types::Value) -> Option<String> {
    match value {
        rusqlite::types::Value::Text(text) => Some(text.clone()),
        rusqlite::types::Value::Integer(number) => Some(number.to_string()),
        rusqlite::types::Value::Real(number) => Some(number.to_string()),
        rusqlite::types::Value::Blob(_) => None,
        rusqlite::types::Value::Null => None,
    }
}

fn as_i64(value: &rusqlite::types::Value) -> Option<i64> {
    match value {
        rusqlite::types::Value::Integer(number) => Some(*number),
        rusqlite::types::Value::Text(text) => text.parse().ok(),
        _ => None,
    }
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
