export function clock(date: Date): string {
  const day = date.toLocaleString("en-GB", { day: "2-digit" });
  const month = date.toLocaleString("en-GB", { month: "short" });
  const year = date.getFullYear();
  const time = date.toLocaleTimeString("en-GB", { hour12: false });
  return `${day} ${month} ${year}  ${time}`;
}

export function hm(ts: string | null | undefined): string {
  if (!ts) return "";
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) {
    const match = ts.match(/(\d{2}:\d{2}:\d{2})/);
    return match ? match[1] : ts;
  }
  return date.toLocaleTimeString("en-GB", { hour12: false });
}

export function pad(value: number): string {
  return String(value).padStart(2, "0");
}

export function agentColor(agent: string): string {
  switch (agent) {
    case "claude":
      return "#c4b5fd";
    case "cursor":
      return "#7dd3fc";
    case "codex":
      return "#f5b942";
    case "graphify":
      return "#4ade80";
    case "orca":
      return "#e7ecef";
    default:
      return "#8b969e";
  }
}

export function roleLabel(agent: string): string {
  switch (agent) {
    case "claude":
      return "Planner";
    case "cursor":
      return "Builder";
    case "codex":
      return "Reviewer";
    case "graphify":
      return "Context";
    case "orca":
      return "Orchestrator";
    default:
      return agent;
  }
}
