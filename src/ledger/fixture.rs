use rusqlite::Connection;

/// Apply Relaybook's published migrations from a v1 ledger to user_version 6.
///
/// The SQL files are copied from `crates/core/src/schema.rs` and the v4 to v5
/// column adds in `migrate.rs` at commit `c25c29f`. This function is for tests
/// and does not run against a ledger the dashboard is watching.
pub fn apply_historical_migrations(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(include_str!("../../fixtures/relaybook/v1_base.sql"))?;
    conn.pragma_update(None, "user_version", 1)?;
    conn.execute_batch(include_str!(
        "../../fixtures/relaybook/migrate_v1_to_v2.sql"
    ))?;
    conn.execute_batch(include_str!(
        "../../fixtures/relaybook/migrate_v2_to_v3.sql"
    ))?;
    conn.execute_batch(include_str!(
        "../../fixtures/relaybook/migrate_v3_to_v4.sql"
    ))?;
    conn.execute_batch(include_str!(
        "../../fixtures/relaybook/migrate_v4_to_v5.sql"
    ))?;
    conn.execute_batch(include_str!(
        "../../fixtures/relaybook/migrate_v5_to_v6.sql"
    ))?;
    conn.pragma_update(None, "user_version", 6)?;
    Ok(())
}

pub fn apply_current_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(include_str!("../../fixtures/relaybook/schema_v6.sql"))?;
    conn.pragma_update(None, "user_version", 6)?;
    Ok(())
}
