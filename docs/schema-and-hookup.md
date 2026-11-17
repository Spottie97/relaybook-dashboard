# Schema, logs, and hookup

## Relaybook schema this build reads

Public `main` of [relaybook](https://github.com/Spottie97/relaybook) at `c25c29f55613228889099b59e1847ea6bf1c68b7` (tag v0.3.3, 2026-11-12) sets `SCHEMA_VERSION` to **6** in `crates/core/src/schema.rs`. `PRAGMA user_version` is that number. The published ledger doc still says the same thing. Version 6 is the fully mapped reader (`src/ledger/v6.rs`). Any other `user_version`, including the unpushed v7 and v8 ledgers, still opens. The UI stays up in degraded mode: tasks and events only, columns chosen with `PRAGMA table_info`, and a banner that reads `unsupported schema vN`. A mapped reader is one new module and one arm in `read_ledger`.

The SQL under `fixtures/relaybook/` is copied from that commit (the `SCHEMA` constant and the migration strings, plus the v4 to v5 `ALTER`s in `migrate.rs`). Tests build a database by running those statements, then read it back through the same adapter the server uses.

### Tables and columns

| Table | Columns used |
| --- | --- |
| `tasks` | `id`, `slug`, `title`, `status`, `claimed_by`, `created_at`, `updated_at`, `run_id` |
| `steps` | `id`, `task_id`, `position`, `description`, `done` |
| `decisions` | `id`, `task_id`, `rationale`, `provenance`, `created_at` |
| `task_deps` | `task_id`, `depends_on_id`, `created_at` |
| `events` | `id`, `ts`, `kind`, `payload` |
| `reports` | `id`, `task_id`, `kind`, `author`, `body`, `bytes`, `created_at`, `reviewed_at`, `verdict`, `review_note` |
| `settings` | `key`, `value`, `updated_at`, `updated_by` |
| `runs` | `id`, `prd_path`, `prd_sha256`, `branch`, `stage`, `cycle`, `agent_calls`, `est_cost_usd`, `status`, `stop_reason`, `started_at`, `failure_streak`, `current_slug`, `complete_requested` |

There is no claims table. A claim is `tasks.claimed_by` plus an `events` row with `kind = 'claimed'`. There is no reviews table. A review is `reports.verdict` (`accepted` or `reopened`) and `review_note`. There is no release sign-off table in schema 6. Sign-off in this UI is a run whose `status` is `complete`.

Status values read from the checks in the schema: task `open|claimed|done|blocked`, event kinds `task_created|claimed|progress|decision|status_changed|dependency_changed|task_renamed|run_linked`, report `task|note`, verdict `accepted|reopened`, run `active|waiting|paused|stopped|complete`.

Settings keys read when present: `mode`, `gates`, `max_cycles`, `max_reopens_per_task`, `max_agent_calls`, `cursor_timeout_min`, `est_cost_cap_usd`. Missing keys use Relaybook's defaults.

### How the cards are derived

- Cursor (and anyone else with `claimed_by`) progress is checked steps over total steps on their tasks. With no steps, it is done tasks over their tasks. No claims means the bar is hidden.
- Claude matches a `claimed_by` containing `claude`.
- Codex is marked waiting when a report has `reviewed_at` null, or when the active run stage contains `review` and the run is `waiting` or `paused`. Codex has no claims in the usual workflow, so its bar stays hidden.
- Graphify's bar stays hidden unless something was claimed by `graphify`.
- The active mission prefers a run in `active`, `waiting`, or `paused`. The title is the `current_slug` task, otherwise the PRD file name.
- Agents online counts the four cards that have a log file or a non-idle status.

## Log formats

One normalized event: `{ ts, agent, kind, summary, detail }` with `kind` of `thinking`, `message`, `tool_call`, `tool_result`, `status`, or `error`.

Assumptions, checked by the parser tests:

- **Claude Code** `--output-format stream-json`. `assistant.message.content[]` blocks of `thinking`, `text`, and `tool_use`. `user` tool results. `result` with `is_error`. `stream_event` partials are dropped. `system` is a status line.
- **Cursor CLI** `--output-format stream-json`. Same assistant text blocks, plus `tool_call` with `subtype` `started` or `completed` and one nested `*ToolCall` object (`args` / `result`).
- **Codex CLI** `--json`. Current `codex exec` events: `thread.started`, `turn.started`, `turn.completed`, `item.started`, `item.completed` with `reasoning`, `agent_message`, or `command_execution`, and top-level `error`. Older lines shaped `{ "msg": { "type": ... } }` are accepted too (`agent_message`, `agent_reasoning`, `exec_command_begin`, `exec_command_end`).
- **Graphify** has no stable stream-json flag. Lines are JSON objects with `type` of `query`, `status`, or `error`, plus `summary`. A non-JSON line is a message.
- **Orca** lines are JSON with `type` (`message`, `thinking`, `status`, `delegate`, `error`) and `summary`.

Timestamps come from `timestamp`, `ts`, or `timestamp_ms` on the line. Otherwise the dashboard uses the time it read the line. Default file mapping, overridable with `--log-map agent=glob`:

| Agent | Glob |
| --- | --- |
| orca | `**/*orca*` |
| graphify | `**/*graphify*` |
| codex | `**/*codex*` |
| cursor | `**/*cursor*` |
| claude | `**/*claude*` |

The first match wins. Other files are counted as unmapped and are not parsed.

## Read-only WAL

The container bind-mounts the ledger directory `:ro`. SQLite readers in WAL mode update the `-shm` file next to the database, which a read-only mount rejects, and which would be a write even if the mount were writable. `immutable=1` is not used, because that would ignore new commits.

Each poll copies `ledger.db` and, when present, `ledger.db-wal` and `ledger.db-journal` into a private temp directory and opens the copy with `mode=ro`, `PRAGMA query_only=ON`, and a 5 second busy timeout. `-shm` is not copied. It is a lock table. SQLite rebuilds it beside the copy, which is how uncheckpointed WAL frames stay visible. A torn copy fails `PRAGMA quick_check` and is retried. If the ledger stays busy, the UI keeps the last good snapshot and says so.

The source directory is only read (`open`/`read`). Tests seal the directory `0555` and the files `0444`, leave a row only in the WAL, and assert the bytes do not change.

Relaybook checkpoints when a command exits, so the main database file is usually current between commands. The copy still includes the WAL so a poll during a command can see it. If the `-wal` file is not readable, the snapshot can lag until the writer checkpoints. The container user (uid 10001 unless `user:` is set) needs read permission on the database and the `-wal` file.

## Hookup

1. On the host, `LEDGER_DIR` is the directory whose children are ledgers (`ledger.db` or `.relaybook/ledger.db` in each child). `CONTEXT_DIR` is the directory of agent JSON logs. Both are mounted read-only. Do not point them at a path you cannot share with this container's user.
2. Set `DASHBOARD_TOKEN` or both `DASHBOARD_BASIC_USER` and `DASHBOARD_BASIC_PASSWORD`.
3. Deploy the compose file from Coolify with the Docker Compose build pack. Optionally set `HOST_BIND_IP` to limit which interface gets the port. Open `http://<host>:7878` from the LAN or the tailnet.
4. Open the UI, pick a workspace, and check the schema line in Settings. Schema 6 gets the full reader. Any other version opens in degraded mode (tasks and events only) with a banner.

`--demo` builds a throwaway ledger and log set so the UI can run without those mounts. It is the only mode that starts without credentials.
