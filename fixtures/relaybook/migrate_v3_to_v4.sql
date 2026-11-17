-- Copied from Relaybook (Apache-2.0), https://github.com/Spottie97/relaybook
-- crates/core/src/schema.rs at c25c29f55613228889099b59e1847ea6bf1c68b7 (v0.3.3, schema user_version 6).
-- Used to build fixture databases. The dashboard never writes a real ledger.
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
    id INTEGER PRIMARY KEY,
    prd_path TEXT NOT NULL,
    prd_sha256 TEXT NOT NULL,
    branch TEXT NOT NULL,
    stage TEXT NOT NULL,
    cycle INTEGER NOT NULL,
    agent_calls INTEGER NOT NULL,
    est_cost_usd REAL NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('active', 'waiting', 'paused', 'stopped', 'complete')),
    stop_reason TEXT,
    started_at TEXT NOT NULL
);
