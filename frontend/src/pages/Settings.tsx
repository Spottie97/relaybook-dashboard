import { useFeed } from "../feed";

export function SettingsPage() {
  const { meta, workspace } = useFeed();
  return (
    <>
      <h1>Settings</h1>
      <p className="sub">Read-only. v1 does not change mode, gates, or the ledger.</p>
      <div className="viewer" style={{ marginTop: 14 }}>
        <p>Schema this build reads: {meta?.schema_supported}. Ledger reports: {workspace?.schema_version ?? "—" }.</p>
        <p>Poll every {meta?.poll_ms} ms. Bound on {meta?.bind}.</p>
        <p>Auth: {meta?.auth_required ? "required" : "off (demo)"}{meta?.auth_token ? " · token" : ""}{meta?.auth_basic ? " · basic" : ""}.</p>
        <p>Mode {workspace?.settings.mode} · gates {workspace?.settings.gates}.</p>
        <p>Limits: cycles {workspace?.settings.max_cycles}, reopens {workspace?.settings.max_reopens_per_task}, calls {workspace?.settings.max_agent_calls}, cursor timeout {workspace?.settings.cursor_timeout_min}m.</p>
        <h2>Log rules</h2>
        <ul>
          {(meta?.log_rules ?? []).map((rule) => (
            <li key={rule.agent}>{rule.agent} = {rule.glob}</li>
          ))}
        </ul>
        <p className="note">Pause, messaging, and review approval come in v2 and call the Relaybook CLI. See docs/v2-actions.md.</p>
      </div>
    </>
  );
}
