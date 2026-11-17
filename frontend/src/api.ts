import type { ArtifactBody, Creds, Meta, NormEvent, Snapshot } from "./types";

const KEY = "relaybook-dashboard-auth";

let browserAuth = false;

/** Basic auth is handled by the browser. Do not attach an Authorization header. */
export function setBrowserAuth(on: boolean) {
  browserAuth = on;
  if (on) sessionStorage.removeItem(KEY);
}

export function loadCreds(): Creds | null {
  const raw = sessionStorage.getItem(KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as Creds;
  } catch {
    return null;
  }
}

export function saveCreds(creds: Creds | null) {
  if (!creds) sessionStorage.removeItem(KEY);
  else sessionStorage.setItem(KEY, JSON.stringify(creds));
}

export function authHeaders(): Headers {
  const headers = new Headers();
  if (browserAuth) return headers;
  const creds = loadCreds();
  if (!creds) return headers;
  if (creds.token) headers.set("Authorization", `Bearer ${creds.token}`);
  else if (creds.user) {
    const bytes = new TextEncoder().encode(`${creds.user}:${creds.password ?? ""}`);
    let binary = "";
    bytes.forEach((byte) => {
      binary += String.fromCharCode(byte);
    });
    headers.set("Authorization", `Basic ${btoa(binary)}`);
  }
  return headers;
}

export async function getMeta(): Promise<Meta> {
  const response = await fetch("/api/meta");
  if (!response.ok) throw new Error("meta unavailable");
  return response.json() as Promise<Meta>;
}

export async function getState(): Promise<Snapshot> {
  const response = await fetch("/api/state", { headers: authHeaders() });
  if (response.status === 401) throw new Error("unauthorized");
  if (!response.ok) throw new Error("state unavailable");
  return response.json() as Promise<Snapshot>;
}

export async function getArtifact(workspace: string, id: string): Promise<ArtifactBody> {
  const query = new URLSearchParams({ workspace, id });
  const response = await fetch(`/api/artifact?${query}`, { headers: authHeaders() });
  if (!response.ok) throw new Error("artifact unavailable");
  return response.json() as Promise<ArtifactBody>;
}

export async function openFeed(
  onState: (snapshot: Snapshot) => void,
  onLog: (event: NormEvent) => void,
  signal: AbortSignal,
): Promise<void> {
  if (browserAuth) {
    return openEventSource(onState, onLog, signal);
  }
  const response = await fetch("/api/stream", { headers: authHeaders(), signal });
  if (response.status === 401) throw new Error("unauthorized");
  if (!response.ok || !response.body) throw new Error("feed unavailable");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (!signal.aborted) {
    const chunk = await reader.read();
    if (chunk.done) break;
    buffer += decoder.decode(chunk.value, { stream: true });
    let split = buffer.indexOf("\n\n");
    while (split >= 0) {
      const raw = buffer.slice(0, split);
      buffer = buffer.slice(split + 2);
      let event = "message";
      let data = "";
      for (const line of raw.split("\n")) {
        if (line.startsWith("event:")) event = line.slice(6).trim();
        else if (line.startsWith("data:")) data += line.slice(5).trim();
      }
      if (data && event === "state") onState(JSON.parse(data) as Snapshot);
      if (data && event === "log") {
        const parsed = JSON.parse(data) as { event: NormEvent };
        onLog(parsed.event);
      }
      split = buffer.indexOf("\n\n");
    }
  }
}

function openEventSource(
  onState: (snapshot: Snapshot) => void,
  onLog: (event: NormEvent) => void,
  signal: AbortSignal,
): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      resolve();
      return;
    }
    const source = new EventSource("/api/stream");
    let settled = false;
    const finish = (error?: Error) => {
      if (settled) return;
      settled = true;
      source.close();
      if (error && !signal.aborted) reject(error);
      else resolve();
    };
    signal.addEventListener("abort", () => finish());
    source.addEventListener("state", (message) => {
      const data = (message as MessageEvent).data as string;
      onState(JSON.parse(data) as Snapshot);
    });
    source.addEventListener("log", (message) => {
      const data = (message as MessageEvent).data as string;
      const parsed = JSON.parse(data) as { event: NormEvent };
      onLog(parsed.event);
    });
    source.onerror = () => finish(new Error("feed unavailable"));
  });
}
