import { Link, useParams } from "react-router-dom";

import { StatusPill } from "../components/StatusPill";
import { useFeed } from "../feed";

export function SessionsPage() {
  const { workspace } = useFeed();
  const params = useParams();
  const runs = workspace?.runs ?? [];
  const selected = runs.find((run) => String(run.id) === params.id) ?? runs[runs.length - 1];
  return (
    <>
      <h1>Sessions</h1>
      <p className="sub">Auto-mode runs stored on the ledger.</p>
      {runs.length === 0 ? <div className="empty">No runs in this ledger.</div> : null}
      <div className="split" style={{ marginTop: 14 }}>
        <div className="list">
          {runs.map((run) => (
            <Link key={run.id} className="row" to={`/sessions/${run.id}`}>
              <b>run {run.id}</b>
              <span>{run.prd_path}<br /><small>{run.branch} · cycle {run.cycle}</small></span>
              <StatusPill status={run.status === "active" ? "running" : run.status === "complete" ? "done" : "waiting"} />
            </Link>
          ))}
        </div>
        {selected ? (
          <div className="viewer">
            <div className="kicker">stage {selected.stage}</div>
            <h2 style={{ marginTop: 8 }}>{selected.prd_path}</h2>
            <p className="sub">calls {selected.agent_calls} · est ${selected.est_cost_usd.toFixed(2)} · failures {selected.failure_streak}</p>
            {selected.waiting_on ? <p>Waiting on {selected.waiting_on}.</p> : null}
            <div className="list" style={{ marginTop: 12 }}>
              {selected.task_slugs.map((slug) => {
                const task = workspace?.tasks.find((item) => item.slug === slug);
                return (
                  <div key={slug} className="row">
                    <b>{slug}</b>
                    <span>{task?.title ?? slug}</span>
                    <small>{task?.status}</small>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}
      </div>
    </>
  );
}
