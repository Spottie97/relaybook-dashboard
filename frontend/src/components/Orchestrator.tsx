import { useState } from "react";

import { useFeed } from "../feed";
import { hm } from "../format";

export function Orchestrator() {
  const { workspace } = useFeed();
  const [open, setOpen] = useState<string | null>(null);
  const items = workspace?.timeline ?? [];
  return (
    <aside className="rail">
      <div className="rail-head">
        <div>
          <div className="who">&gt;_ Orchestrator</div>
          <div className="role">{workspace?.session_label ?? "No active session"}</div>
        </div>
        {workspace?.session_active ? <span className="live" style={{ marginLeft: "auto" }}><i className="dot pulse" /> </span> : null}
      </div>
      <div className="rail-scroll">
        {items.length === 0 ? <div className="empty">No orchestration events yet.</div> : null}
        {items.map((item) => (
          <article key={item.id} className="bubble">
            <div className="who">{item.actor} · {hm(item.ts)}</div>
            {item.kind === "checklist" ? (
              item.checks.map((check) => (
                <div key={check.label} className={`check ${check.state}`}>
                  <i>{check.state === "done" ? "✓" : "•"}</i>
                  <span>{check.label}</span>
                </div>
              ))
            ) : (
              <>
                <p>{item.collapsible && open !== item.id ? `${item.text.slice(0, 80)}…` : item.text}</p>
                {item.collapsible ? (
                  <button type="button" className="toggle" onClick={() => setOpen(open === item.id ? null : item.id)}>
                    {open === item.id ? "Hide" : "Show"} {item.kind}
                  </button>
                ) : null}
                {open === item.id && item.detail ? <p>{item.detail}</p> : null}
              </>
            )}
          </article>
        ))}
      </div>
      <form className="composer" onSubmit={(event) => event.preventDefault()}>
        <input disabled placeholder="> Send a message or command" />
        <div className="note">Messaging comes in v2. It will go through the Relaybook CLI.</div>
      </form>
    </aside>
  );
}
