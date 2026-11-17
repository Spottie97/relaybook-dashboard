import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import { getMeta, getState, loadCreds, openFeed, saveCreds } from "./api";
import type { Creds, Meta, NormEvent, Snapshot, Workspace } from "./types";

type Feed = {
  meta: Meta | null;
  snapshot: Snapshot | null;
  logs: NormEvent[];
  workspaceId: string;
  setWorkspaceId: (id: string) => void;
  workspace: Workspace | null;
  connected: boolean;
  banner: string | null;
  creds: Creds | null;
  setCreds: (creds: Creds | null) => void;
};

const Ctx = createContext<Feed | null>(null);

export function FeedProvider({ children }: { children: ReactNode }) {
  const [meta, setMeta] = useState<Meta | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [logs, setLogs] = useState<NormEvent[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [connected, setConnected] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const [creds, setCredsState] = useState<Creds | null>(loadCreds());

  function setCreds(next: Creds | null) {
    saveCreds(next);
    setCredsState(next);
  }

  useEffect(() => {
    let stop = false;
    getMeta()
      .then((value) => {
        if (!stop) setMeta(value);
      })
      .catch(() => {
        if (!stop) setBanner("The dashboard is not answering.");
      });
    return () => {
      stop = true;
    };
  }, []);

  const locked = Boolean(meta?.auth_required && !creds);

  useEffect(() => {
    if (!meta || locked) return;
    const controller = new AbortController();
    let timer = 0;
    const run = () => {
      setBanner(null);
      openFeed(
        (next) => {
          setSnapshot(next);
          setConnected(true);
          setLogs((local) => mergeLogs(next.workspaces.find((item) => item.mission?.status === "running")?.logs ?? next.workspaces[0]?.logs ?? [], local));
          setWorkspaceId((current) => pickWorkspace(next, current));
        },
        (event) => {
          setLogs((local) => mergeLogs(local, [event]));
          setConnected(true);
        },
        controller.signal,
      )
        .catch((err: Error) => {
          if (controller.signal.aborted) return;
          if (err.message === "unauthorized") {
            setCreds(null);
            setBanner("Sign in to read the ledger.");
            return;
          }
          setConnected(false);
          setBanner("Feed interrupted. Reconnecting…");
          timer = window.setTimeout(run, 1500);
        });
    };
    getState()
      .then((next) => {
        setSnapshot(next);
        setLogs(next.workspaces.find((item) => item.mission?.status === "running")?.logs ?? next.workspaces[0]?.logs ?? []);
        setWorkspaceId((current) => pickWorkspace(next, current));
      })
      .catch((err: Error) => {
        if (err.message === "unauthorized") setCreds(null);
      });
    run();
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [meta, locked, creds]);

  const workspace = useMemo(
    () => snapshot?.workspaces.find((item) => item.id === workspaceId) ?? snapshot?.workspaces[0] ?? null,
    [snapshot, workspaceId],
  );

  const value: Feed = {
    meta,
    snapshot,
    logs: logs.length ? logs : workspace?.logs ?? [],
    workspaceId: workspace?.id ?? workspaceId,
    setWorkspaceId,
    workspace,
    connected,
    banner,
    creds,
    setCreds,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

function pickWorkspace(snapshot: Snapshot, current: string): string {
  if (current && snapshot.workspaces.some((item) => item.id === current)) return current;
  return snapshot.workspaces.find((item) => item.mission?.status === "running")?.id
    ?? snapshot.workspaces[0]?.id
    ?? "";
}

function mergeLogs(incoming: NormEvent[], local: NormEvent[]): NormEvent[] {
  const map = new Map<number, NormEvent>();
  for (const event of incoming) map.set(event.seq, event);
  for (const event of local) if (!map.has(event.seq)) map.set(event.seq, event);
  return [...map.values()].sort((left, right) => left.seq - right.seq).slice(-400);
}

export function useFeed(): Feed {
  const value = useContext(Ctx);
  if (!value) throw new Error("feed missing");
  return value;
}
