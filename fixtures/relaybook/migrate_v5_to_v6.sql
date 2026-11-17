-- Copied from Relaybook (Apache-2.0), https://github.com/Spottie97/relaybook
-- crates/core/src/schema.rs at c25c29f55613228889099b59e1847ea6bf1c68b7 (v0.3.3, schema user_version 6).
-- Used to build fixture databases. The dashboard never writes a real ledger.
CREATE TABLE events_v6 (
    id INTEGER PRIMARY KEY,
    ts TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (
        kind IN (
            'task_created',
            'claimed',
            'progress',
            'decision',
            'status_changed',
            'dependency_changed',
            'task_renamed',
            'run_linked'
        )
    ),
    payload TEXT NOT NULL
);

INSERT INTO events_v6 (id, ts, kind, payload)
SELECT id, ts, kind, payload FROM events;

DROP TABLE events;

ALTER TABLE events_v6 RENAME TO events;

CREATE UNIQUE INDEX IF NOT EXISTS events_progress_once
ON events (
    json_extract(payload, '$.slug'),
    json_extract(payload, '$.commit')
)
WHERE kind = 'progress'
  AND json_extract(payload, '$.commit') IS NOT NULL;
