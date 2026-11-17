export type NormEvent = {
  seq: number;
  ts: string;
  agent: string;
  kind: "thinking" | "message" | "tool_call" | "tool_result" | "status" | "error" | string;
  summary: string;
  detail: Record<string, unknown>;
  source: string;
};

export type Step = { position: number; description: string; done: boolean };

export type Task = {
  id: number;
  slug: string;
  title: string;
  status: string;
  claimed_by: string | null;
  run_id: number | null;
  created_at: string;
  updated_at: string;
  depends_on: string[];
  steps_done: number;
  steps_total: number;
  steps: Step[];
};

export type Run = {
  id: number;
  prd_path: string;
  branch: string;
  stage: string;
  cycle: number;
  agent_calls: number;
  est_cost_usd: number;
  status: string;
  stop_reason: string | null;
  started_at: string;
  failure_streak: number;
  current_slug: string | null;
  complete_requested: boolean;
  waiting_on: string | null;
  task_slugs: string[];
};

export type AgentCard = {
  id: string;
  name: string;
  role: string;
  readonly: boolean;
  status: string;
  progress: number | null;
  latest: string | null;
  latest_ts: string | null;
  log_present: boolean;
  claims: string[];
};

export type CheckItem = { label: string; state: string };

export type TimelineItem = {
  id: string;
  ts: string;
  actor: string;
  kind: string;
  text: string;
  detail: string | null;
  collapsible: boolean;
  checks: CheckItem[];
};

export type ArtifactRef = {
  id: string;
  name: string;
  kind: string;
  detail: string;
  bytes: number;
  modified: string;
};

export type ArtifactBody = ArtifactRef & {
  language: string;
  content: string;
  truncated: boolean;
};

export type Mission = {
  title: string;
  status: string;
  stage: string | null;
  run_id: number | null;
  task_slug: string | null;
};

export type Workspace = {
  id: string;
  label: string;
  schema_version: number | null;
  schema_warning: string | null;
  ledger_ok: boolean;
  ledger_error: string | null;
  stale: boolean;
  logs_found: number;
  unmapped_logs: number;
  health_ok: boolean;
  health_label: string;
  settings: {
    mode: string;
    gates: string;
    max_cycles: number;
    max_reopens_per_task: number;
    max_agent_calls: number;
    cursor_timeout_min: number;
    est_cost_cap_usd: string | null;
  };
  mission: Mission | null;
  stats: { agents_online: number; active_missions: number };
  agents: AgentCard[];
  session_active: boolean;
  session_label: string;
  timeline: TimelineItem[];
  tasks: Task[];
  runs: Run[];
  artifacts: ArtifactRef[];
  logs: NormEvent[];
};

export type Snapshot = {
  server_time: string;
  poll_ms: number;
  demo: boolean;
  changed: string[];
  workspaces: Workspace[];
};

export type Meta = {
  demo: boolean;
  poll_ms: number;
  bind: string;
  auth_basic: boolean;
  auth_token: boolean;
  auth_required: boolean;
  schema_supported: number;
  read_only: boolean;
  log_rules: { agent: string; glob: string }[];
};

export type Creds = { token?: string; user?: string; password?: string };
