# relaybook-dashboard

Watch-only command center for a [Relaybook](https://github.com/Spottie97/relaybook) ledger. One process serves the UI and a live feed. It reads tasks, runs, reports, and agent JSON logs. It does not write them, start agents, or touch git.

The binary, image, and compose service are all named `relaybook-dashboard`.

## Deploy on Coolify

Use Coolify's Docker Compose build pack: it builds `Dockerfile` and runs `docker-compose.yml`. The image listens on `0.0.0.0:7878` inside the container. The compose file publishes that port only on `TS_BIND_IP`, the host's Tailscale IPv4 (`tailscale ip -4`), so the dashboard is reachable over the tailnet and not on the LAN or the internet. Do not attach a Coolify domain, because the Coolify proxy listens on every interface. The service is capped at 0.5 CPU and 256 MB of memory.

Set these in Coolify before the first deploy:

| Variable | Required | Purpose |
| --- | --- | --- |
| `TS_BIND_IP` | yes | Host Tailscale IPv4 to publish the port on. `HOST_PORT` overrides the host port (default 7878). |
| `LEDGER_DIR` | yes | Host directory of ledgers. Each child may be a `ledger.db`, a directory containing `ledger.db`, or a project with `.relaybook/ledger.db`. |
| `CONTEXT_DIR` | yes | Host directory of agent JSON logs. |
| `DASHBOARD_TOKEN` | one of these | Bearer token for the UI and `/api/*`. |
| `DASHBOARD_BASIC_USER` and `DASHBOARD_BASIC_PASSWORD` | one of these | HTTP basic auth. |

Both directories are mounted read-only (`:ro`). The process exits if neither auth method is set. `/healthz` stays unauthenticated so the compose healthcheck can call it. It returns `{"ok":true}` and no ledger data.

Optional: `POLL_MS` (default 1500, clamped to 500–60000), `ARTIFACTS_DIR` (default in compose is `/data/context/artifacts`), `LOG_MAP` (`claude=**/*claude*,cursor=**/*cursor*`).

The image user is uid `10001`. If the mounted files are not world-readable, set the compose `user:` to a uid that can read them. The process never needs write access to those mounts.

WAL notes are in [docs/schema-and-hookup.md](docs/schema-and-hookup.md). Short version: readers do not open the source database, because SQLite would update `-shm` beside it. Each poll copies the database and the `-wal` file into a temp directory and opens that copy with `mode=ro`.

```sh
docker compose up --build
```

needs `LEDGER_DIR`, `CONTEXT_DIR`, and a credential in the environment. An empty `.env` is not enough.

## Local demo

No ledger and no credentials:

```sh
cargo run -- --demo
```

Then open `http://127.0.0.1:7878`. Demo replays sample Claude, Cursor, Codex, Graphify, and Orca logs against a fixture built from the real schema. Add credentials if you want the lock screen even in demo.

Build the UI first, once:

```sh
cd frontend && npm ci && npm run build && cd ..
cargo build --release
```

Rust 1.88. The release binary embeds `frontend/dist`.

Flags and environment:

| Flag | Environment | Default |
| --- | --- | --- |
| `--ledger` | `LEDGER_DIR` | required outside demo |
| `--logs` | `CONTEXT_DIR` | none |
| `--artifacts` | `ARTIFACTS_DIR` | none |
| `--bind` | `BIND` | `127.0.0.1:7878` |
| `--port` | `PORT` | if `BIND` is unset, listen on `0.0.0.0:$PORT` |
| `--poll-ms` | `POLL_MS` | `1500` |
| `--log-map` | `LOG_MAP` | built-in globs |
| `--demo` | `DEMO=1` | off |
| `--token` | `DASHBOARD_TOKEN` | unset |
| `--basic-user` / `--basic-password` | `DASHBOARD_BASIC_USER` / `DASHBOARD_BASIC_PASSWORD` | unset |

The container image sets `BIND=0.0.0.0:7878`. A binary you start yourself stays on loopback until you set `BIND` or `PORT`.

Inside the agents container the usual paths are passed as flags, not baked in:

```sh
relaybook-dashboard \
  --ledger /work/ledger \
  --logs /work/context \
  --artifacts /work/context/artifacts \
  --bind 0.0.0.0:7878
```

`--ledger /work/ledger` is a directory of workspaces. One entry appears in the selector per ledger.

## SSH tunnel

Coolify is the normal way in. If you instead run the binary on loopback, tunnel to that loopback:

```sh
ssh -L 7878:127.0.0.1:7878 <user>@<host>
```

Then open `http://127.0.0.1:7878` on your machine. Do not publish the port on the host when the process is bound to loopback.

## Tests

```sh
cargo test
cd frontend && npm run build
```

`cargo test` covers the schema adapter (a database built by the real v1–v6 migrations), a read-only WAL mount that must not change source bytes, log parsers for each CLI, and auth on `/healthz` versus `/api/state`.

## v2

Messaging, pause, and review approval are specified in [docs/v2-actions.md](docs/v2-actions.md). They are not implemented. They have to call the Relaybook CLI so its gates still apply.

## License

MIT. The fixture SQL is copied from Relaybook (Apache-2.0) so tests can build a real ledger. See the headers under `fixtures/relaybook/`.
