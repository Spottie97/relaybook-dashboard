use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OpenFlags};

use super::v6;
use super::{LedgerDump, LedgerError, SUPPORTED_SCHEMA_VERSION};

/// A read-only SQLite connection. `mode=ro` plus `query_only` so a later edit
/// cannot write, and `immutable` is never set because the source is live.
pub struct Opened {
    pub conn: Connection,
    _temp: Option<TempCopy>,
}

struct TempCopy {
    path: PathBuf,
}

impl Drop for TempCopy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn open_readonly(path: &Path) -> Result<Opened, LedgerError> {
    open_readonly_timeout(path, Duration::from_secs(5))
}

pub fn open_readonly_timeout(path: &Path, busy: Duration) -> Result<Opened, LedgerError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(sqlite_uri(path, "mode=ro"), flags).map_err(classify)?;
    conn.busy_timeout(busy).map_err(classify)?;
    conn.execute_batch("PRAGMA query_only=ON;")
        .map_err(classify)?;
    Ok(Opened { conn, _temp: None })
}

/// Read a live ledger without writing the source directory.
///
/// SQLite readers in WAL mode update the `-shm` file beside the database.
/// A read-only bind mount rejects that, and creating `-shm` would be a write
/// even on a writable directory. This copies `ledger.db` and `ledger.db-wal`
/// (and a rollback `-journal`, when one is present) into a private temp
/// directory and opens that copy with `mode=ro`. The `-shm` file is a lock
/// table, not durable data, so it is not copied. SQLite rebuilds it beside
/// the copy.
///
/// A torn copy, from reading the files mid-commit, fails `quick_check` and is
/// retried. Callers that still see [`LedgerError::Busy`] or [`LedgerError::Torn`]
/// should keep the previous snapshot and poll again.
pub fn read_ledger(source: &Path) -> Result<LedgerDump, LedgerError> {
    with_readonly_copy(source, |conn| {
        ensure_intact(conn)?;
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(classify)?;
        if version != SUPPORTED_SCHEMA_VERSION {
            return Err(LedgerError::UnsupportedSchema {
                found: version,
                supported: SUPPORTED_SCHEMA_VERSION,
            });
        }
        v6::dump(conn)
    })
}

pub fn with_readonly_copy<T>(
    source: &Path,
    body: impl Fn(&Connection) -> Result<T, LedgerError>,
) -> Result<T, LedgerError> {
    let mut last = None;
    for attempt in 0..4 {
        match copy_and_open(source).and_then(|opened| body(&opened.conn)) {
            Ok(value) => return Ok(value),
            Err(err) if err.retryable() && attempt < 3 => {
                last = Some(err);
                std::thread::sleep(Duration::from_millis(40 * (1 << attempt)));
            }
            Err(err) => return Err(err),
        }
    }
    Err(last.unwrap_or(LedgerError::Torn))
}

fn copy_and_open(source: &Path) -> Result<Opened, LedgerError> {
    if !source.is_file() {
        return Err(LedgerError::Unreadable);
    }
    let temp = TempCopy::new()?;
    let copy = temp.path.join("ledger.db");
    fs::copy(source, &copy).map_err(|_| LedgerError::Unreadable)?;
    copy_sidecar(source, &copy, "-wal")?;
    copy_sidecar(source, &copy, "-journal")?;
    let mut opened = open_readonly(&copy)?;
    opened._temp = Some(temp);
    Ok(opened)
}

impl TempCopy {
    fn new() -> Result<Self, LedgerError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "relaybook-dashboard-{}-{}-{}",
            std::process::id(),
            tick,
            n
        ));
        fs::create_dir_all(&path).map_err(|_| LedgerError::Unreadable)?;
        Ok(Self { path })
    }
}

fn copy_sidecar(source: &Path, copy: &Path, suffix: &str) -> Result<(), LedgerError> {
    let from = sidecar(source, suffix);
    if from.is_file() {
        fs::copy(&from, sidecar(copy, suffix)).map_err(|_| LedgerError::Unreadable)?;
    }
    Ok(())
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn ensure_intact(conn: &Connection) -> Result<(), LedgerError> {
    let mut statement = conn.prepare("PRAGMA quick_check").map_err(classify)?;
    let mut rows = statement.query([]).map_err(classify)?;
    let mut saw_ok = false;
    while let Some(row) = rows.next().map_err(classify)? {
        let message: String = row.get(0).map_err(classify)?;
        if message == "ok" {
            saw_ok = true;
        } else {
            return Err(LedgerError::Torn);
        }
    }
    if saw_ok {
        Ok(())
    } else {
        Err(LedgerError::Torn)
    }
}

fn classify(err: rusqlite::Error) -> LedgerError {
    match &err {
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == ErrorCode::DatabaseBusy || code.code == ErrorCode::DatabaseLocked =>
        {
            LedgerError::Busy
        }
        rusqlite::Error::SqliteFailure(code, message)
            if code.code == ErrorCode::CannotOpen
                || code.extended_code == 1544
                || message
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains("readonly") =>
        {
            LedgerError::Unreadable
        }
        other => LedgerError::Sqlite(code_only(other)),
    }
}

fn code_only(err: &rusqlite::Error) -> String {
    match err {
        rusqlite::Error::SqliteFailure(code, _) => format!("code {}", code.extended_code),
        _ => "unavailable".to_string(),
    }
}

fn sqlite_uri(path: &Path, query: &str) -> String {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut encoded = String::from("file:");
    let text = absolute.to_string_lossy();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'_' | b'-' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded.push('?');
    encoded.push_str(query);
    encoded
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use rusqlite::config::DbConfig;
    use rusqlite::Connection;

    use super::*;

    fn seal(dir: &Path) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o444);
            fs::set_permissions(&path, permissions).unwrap();
        }
        let mut permissions = fs::metadata(dir).unwrap().permissions();
        permissions.set_mode(0o555);
        fs::set_permissions(dir, permissions).unwrap();
    }

    fn fingerprint(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut files = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                files.push((
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
    }

    #[test]
    fn readonly_mount_sees_uncheckpointed_wal_without_touching_source() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ledger.db");
        let secret = "wal-only-token-9f3a";
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
                .unwrap();
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE t (v TEXT NOT NULL);
                 INSERT INTO t (v) VALUES ('wal-only-token-9f3a');",
            )
            .unwrap();
        }
        let wal = dir.path().join("ledger.db-wal");
        assert!(wal.is_file(), "expected a wal file");
        let db_bytes = fs::read(&db_path).unwrap();
        let wal_bytes = fs::read(&wal).unwrap();
        assert!(
            !db_bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes()),
            "secret leaked into the main db; the test would not prove a wal read"
        );
        assert!(wal_bytes
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()));
        seal(dir.path());
        let before = fingerprint(dir.path());
        let seen = with_readonly_copy(&db_path, |conn| {
            let value: String = conn
                .query_row("SELECT v FROM t", [], |row| row.get(0))
                .map_err(classify)?;
            let insert = conn.execute("INSERT INTO t (v) VALUES ('nope')", []);
            assert!(insert.is_err(), "query_only connection accepted a write");
            Ok(value)
        })
        .expect("read");
        assert_eq!(seen, secret);
        assert_eq!(
            before,
            fingerprint(dir.path()),
            "source bytes or files changed"
        );
    }

    #[test]
    fn busy_timeout_waits_for_an_exclusive_lock() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ledger.db");
        let writer = Connection::open(&db_path).unwrap();
        writer
            .execute_batch("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('held');")
            .unwrap();
        writer.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let path = db_path.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            writer.execute_batch("COMMIT").unwrap();
        });
        let value = read_one(&path, Duration::from_secs(2)).expect("lock released");
        assert_eq!(value, "held");
        handle.join().unwrap();
    }

    #[test]
    fn busy_timeout_reports_busy_when_the_lock_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ledger.db");
        let writer = Connection::open(&db_path).unwrap();
        writer
            .execute_batch(
                "CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('held'); BEGIN EXCLUSIVE",
            )
            .unwrap();
        let err = read_one(&db_path, Duration::from_millis(150)).unwrap_err();
        assert!(matches!(err, LedgerError::Busy), "{err}");
        writer.execute_batch("COMMIT").unwrap();
    }

    fn read_one(path: &Path, busy: Duration) -> Result<String, LedgerError> {
        let opened = open_readonly_timeout(path, busy)?;
        opened
            .conn
            .query_row("SELECT v FROM t", [], |row| row.get(0))
            .map_err(classify)
    }
}
