import { Link, useParams } from "react-router-dom";

import { StatusPill } from "../components/StatusPill";
import { useFeed } from "../feed";
import { hm } from "../format";

export function AgentsPage() {
  const { workspace, logs } = useFeed();
  const params = useParams();
  const agents = workspace?.agents ?? [];
  const selected = agents.find((agent) => agent.id === params.id) ?? agents[0];
  const lines = selected ? logs.filter((event) => event.agent === selected.id).slice(-12) : [];
  return (
    <>
      <h1>Agents</h1>
      <p className="sub">Roles come from claims and the log files, not from a second roster.</p>
      <div className="split" style={{ marginTop: 14 }}>
        <div className="list">
          {agents.map((agent) => (
            <Link key={agent.id} className="agent-row" to={`/agents/${agent.id}`}>
              <span>
                <b>{agent.name}</b>
                <small>{agent.role}{agent.readonly ? " · read only" : ""}</small>
              </span>
              <StatusPill status={agent.status} />
            </Link>
          ))}
        </div>
        {selected ? (
          <div className="viewer">
            <div className="kicker">{selected.role}</div>
            <h2 style={{ marginTop: 8 }}>{selected.name}</h2>
            <p className="sub">{selected.claims.length ? `Claimed ${selected.claims.join(", ")}` : "No current claim"}</p>
            {selected.progress === null ? <p className="sub">Progress hidden. No claimed steps to measure.</p> : <p className="sub">{selected.progress}% of claimed steps</p>}
            <h3 style={{ fontSize: 12, letterSpacing: "0.12em", textTransform: "uppercase", color: "#6d787f" }}>Recent lines</h3>
            {lines.length === 0 ? <div className="empty">No lines for this agent yet.</div> : null}
            {lines.map((event) => (
              <p key={event.seq}><small>{hm(event.ts)}</small> {event.summary}</p>
            ))}
            <h3 style={{ fontSize: 12, letterSpacing: "0.12em", textTransform: "uppercase", color: "#6d787f" }}>Tasks</h3>
            {(workspace?.tasks ?? []).filter((task) => (task.claimed_by ?? "").toLowerCase().includes(selected.id === "claude" ? "claude" : selected.id)).map((task) => (
              <div key={task.slug} style={{ marginTop: 8 }}>
                <b>{task.slug}</b> <small>{task.status} · {task.steps_done}/{task.steps_total}</small>
                {task.steps.map((step) => (
                  <div key={step.position} className={`check ${step.done ? "done" : "pending"}`}>
                    <i>{step.done ? "✓" : "•"}</i>
                    <span>{step.description}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        ) : <div className="empty">No agents in view.</div>}
      </div>
    </>
  );
}
