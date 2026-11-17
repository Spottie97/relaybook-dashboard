import { useEffect, useState, type FormEvent } from "react";
import { Link, NavLink, Route, Routes, useLocation } from "react-router-dom";

import { LiveLog } from "./components/LiveLog";
import { Orchestrator } from "./components/Orchestrator";
import { StatusPill } from "./components/StatusPill";
import { Hint } from "./components/Hint";
import { FeedProvider, useFeed } from "./feed";
import { clock, pad } from "./format";
import { AgentsPage } from "./pages/Agents";
import { ArtifactsPage } from "./pages/Artifacts";
import { SessionsPage } from "./pages/Sessions";
import { SettingsPage } from "./pages/Settings";
import { OverviewExtras } from "./pages/Overview";

export function App() {
  return (
    <FeedProvider>
      <Shell />
    </FeedProvider>
  );
}

function Shell() {
  const feed = useFeed();
  const location = useLocation();
  const overview = location.pathname === "/";
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  if (!feed.meta) {
    return <div className="gate"><p>Connecting to the feed…</p></div>;
  }
  if (feed.meta.auth_required && !feed.meta.auth_basic && !feed.creds) {
    return <Gate />;
  }

  const health = feed.workspace?.health_label ?? "Waiting for a ledger";
  const healthClass = !feed.workspace
    ? "warn"
    : feed.workspace.schema_warning
      ? "warn"
      : !feed.workspace.ledger_ok || feed.workspace.stale
        ? "bad"
        : feed.workspace.logs_found === 0
          ? "warn"
          : "ok";
  const crumb = overview ? "command-center" : location.pathname.replace("/", "");

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">&gt;_ ORCHESTRATOR</div>
        <div className="crumb">workspace / <b>{crumb}</b></div>
        <div className="top-right">
          <span className="live"><i className={`dot ${feed.connected ? "pulse" : ""}`} /> LIVE</span>
          <span>{clock(now)}</span>
        </div>
      </header>
      <div className={overview ? "body with-rail" : "body"}>
        <aside className="side">
          <label className="workspace">
            <div className="kicker">Workspace</div>
            <select
              value={feed.workspaceId}
              onChange={(event) => feed.setWorkspaceId(event.target.value)}
            >
              {(feed.snapshot?.workspaces ?? []).map((workspace) => (
                <option key={workspace.id} value={workspace.id}>{workspace.label}</option>
              ))}
            </select>
          </label>
          <nav className="nav">
            <NavLink to="/" end>Overview</NavLink>
            <NavLink to="/sessions">Sessions</NavLink>
            <NavLink to="/agents">Agents</NavLink>
            <NavLink to="/artifacts">Artifacts</NavLink>
            <NavLink to="/settings">Settings</NavLink>
          </nav>
          <div className={`health ${healthClass}`}>
            <i className="dot" />
            <span>{health}</span>
          </div>
        </aside>
        <main className="main">
          {feed.workspace?.schema_warning ? <div className="banner">{feed.workspace.schema_warning}</div> : null}
          {feed.banner ? <div className="banner">{feed.banner}</div> : null}
          <Routes>
            <Route path="/" element={<Overview />} />
            <Route path="/sessions" element={<SessionsPage />} />
            <Route path="/sessions/:id" element={<SessionsPage />} />
            <Route path="/agents" element={<AgentsPage />} />
            <Route path="/agents/:id" element={<AgentsPage />} />
            <Route path="/artifacts" element={<ArtifactsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Routes>
        </main>
        {overview ? <Orchestrator /> : null}
      </div>
    </div>
  );
}

function Overview() {
  const { workspace } = useFeed();
  if (!workspace) return <div className="empty">Point the dashboard at a Relaybook ledger.</div>;
  const stats = workspace.stats;
  return (
    <>
      <div className="head">
        <div>
          <h1>Command center</h1>
          <p className="sub">One objective. Multiple agents. Full visibility.</p>
        </div>
        <div className="stats">
          <div className="stat"><b>{pad(stats.agents_online)}</b><span>agents online</span></div>
          <div className="stat"><b>{pad(stats.active_missions)}</b><span>active mission</span></div>
        </div>
      </div>
      <section className="mission">
        <div>
          <div className="kicker">Active mission</div>
          <div className="title">{workspace.mission?.title ?? "No active mission"}</div>
          {workspace.mission?.stage ? <div className="meta">stage {workspace.mission.stage}</div> : null}
        </div>
        {workspace.mission ? <StatusPill status={workspace.mission.status} /> : null}
        <div className="spacer" />
        <Hint label="Coming in v2">
          <button className="ghost" type="button" disabled>Pause</button>
        </Hint>
      </section>
      {workspace.ledger_error && !workspace.ledger_ok ? <div className="banner">{workspace.ledger_error}</div> : null}
      <section className="section">
        <h2>Agent activity</h2>
        <AgentGrid />
      </section>
      <section className="section">
        <h2>Live execution</h2>
        <LiveLog />
      </section>
      <OverviewExtras />
    </>
  );
}

function AgentGrid() {
  const { workspace, logs } = useFeed();
  const review = [...logs].reverse().find((event) => event.agent === "codex" && event.kind === "review");
  const reviewText = typeof review?.detail.text === "string" ? review.detail.text : "";
  return (
    <div className="grid">
      {(workspace?.agents ?? []).map((agent) => (
        <Link key={agent.id} className="card" to={`/agents/${agent.id}`} style={{ textDecoration: "none", color: "inherit" }}>
          <div className="card-top">
            <div className="mark">{agent.name.slice(0, 1)}</div>
            <div>
              <h3>{agent.name}</h3>
              <div className="role">{agent.role}</div>
            </div>
            {agent.readonly ? <span className="badge">READ ONLY</span> : null}
            <span style={{ marginLeft: "auto" }}><StatusPill status={agent.status} /></span>
          </div>
          {agent.progress === null ? null : (
            <>
              <div className="pct">{agent.progress}%</div>
              <div className="bar"><i style={{ width: `${agent.progress}%` }} /></div>
            </>
          )}
          <p className="latest">Latest: {agent.latest ?? "—"} {agent.latest_ts ? agent.latest_ts.slice(11, 19) : ""}</p>
          {agent.id === "codex" && reviewText ? <pre className="review">{reviewText}</pre> : null}
        </Link>
      ))}
    </div>
  );
}

function Gate() {
  const feed = useFeed();
  const [token, setToken] = useState("");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const creds = feed.meta?.auth_token && token
      ? { token }
      : { user, password };
    feed.setCreds(creds);
    setError(null);
  }

  return (
    <div className="gate">
      <form onSubmit={submit}>
        <div className="brand">&gt;_ ORCHESTRATOR</div>
        <h1>Read-only access</h1>
        <p className="sub">This dashboard never writes the ledger. Credentials stay in this tab.</p>
        {feed.meta?.auth_token ? (
          <label className="field">Token
            <input value={token} onChange={(event) => setToken(event.target.value)} type="password" autoComplete="off" />
          </label>
        ) : null}
        {feed.meta?.auth_basic ? (
          <>
            <label className="field">User
              <input value={user} onChange={(event) => setUser(event.target.value)} autoComplete="username" />
            </label>
            <label className="field">Password
              <input value={password} onChange={(event) => setPassword(event.target.value)} type="password" autoComplete="current-password" />
            </label>
          </>
        ) : null}
        {error ? <div className="banner">{error}</div> : null}
        <button type="submit">Continue</button>
      </form>
    </div>
  );
}
