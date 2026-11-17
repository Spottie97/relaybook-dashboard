-- Copied from Relaybook (Apache-2.0), https://github.com/Spottie97/relaybook
-- crates/core/src/schema.rs at c25c29f55613228889099b59e1847ea6bf1c68b7 (v0.3.3, schema user_version 6).
-- Used to build fixture databases. The dashboard never writes a real ledger.
CREATE TABLE IF NOT EXISTS reports (
    id INTEGER PRIMARY KEY,
    task_id INTEGER REFERENCES tasks(id),
    kind TEXT NOT NULL CHECK (kind IN ('task', 'note')),
    author TEXT NOT NULL,
    body TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    reviewed_at TEXT,
    verdict TEXT CHECK (verdict IS NULL OR verdict IN ('accepted', 'reopened')),
    review_note TEXT
);
