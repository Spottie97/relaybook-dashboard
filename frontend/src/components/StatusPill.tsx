export function StatusPill({ status }: { status: string }) {
  const label = status === "running" ? "Running" : status === "waiting" ? "Waiting" : status === "done" || status === "complete" ? (status === "done" ? "Done" : "Complete") : status === "error" ? "Error" : "Idle";
  return <span className={`pill ${status}`}>{label.toUpperCase()}</span>;
}
