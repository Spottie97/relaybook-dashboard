# v2 actions

v1 is watch-only. The server has no route that writes the ledger, runs a command, starts an agent, or touches git. v2 may add the actions below. Every one of them must be a subprocess of the `relaybook` binary in the ledger's project directory. The dashboard still opens SQLite read-only. A button is not allowed to `INSERT`, `UPDATE`, or checkpoint the database, and it is not allowed to push, merge, or deploy.

The CLI already enforces claim races, report size limits, dependency checks, and the plan/review gates. Calling it keeps those gates. A non-zero exit is shown and is not retried as a success.

## Approve a checkpoint

When a run is `waiting`, the human answer is:

```sh
relaybook approve
```

That is the only continue action. The disabled Pause button in v1 becomes `relaybook run stop` in v2. Resume, if it is offered, is `relaybook run --resume`. Neither command is a process kill, and neither publishes the branch.

## Review work

Unreviewed rows in `reports` are the review queue.

```sh
relaybook inbox
relaybook report accept <slug>
relaybook report accept --note <id>
relaybook task reopen <slug> --reason "<why>"
```

Accept and reopen are the only review verdicts. v2 does not invent a third verdict, and it does not mark a task done on the operator's behalf. `relaybook task done` stays an executor command.

## Answer a check request

A check request is one of two ledger states:

- a run with `status = waiting`, answered by `relaybook approve`
- a report with `reviewed_at` null, answered by `relaybook report accept` or `relaybook task reopen`

There is no side channel for a free-form "yes" that skips the gate. If the ledger is not waiting and the inbox is empty, the composer refuses the send.

## Message Orca or Cursor

Relaybook has no chat table. A message is one of the commands the CLI already allows, so the next agent sees it in `relaybook context` or `relaybook inbox`:

- guidance for the executor: `relaybook task create`, `relaybook step add`, `relaybook decision log`
- a note back to the planner: `relaybook report note --by <operator> --text "..."`

v2 does not write prompt files, does not append to the JSON logs, and does not spawn `claude`, `cursor`, `codex`, `orca`, or any other agent. If the text does not map to one of those commands, the UI keeps it unsent.

## What stays impossible

- direct SQL writes, including a private outbox table
- spawning or signalling agent processes
- `git push`, merge, tag, or deploy
- bypassing `relaybook gates`
- editing a report or event in place (events are append-only in Relaybook)
