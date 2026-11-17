import { useEffect, useMemo, useRef, useState } from "react";

import { useFeed } from "../feed";
import { agentColor, hm, roleLabel } from "../format";
import type { NormEvent } from "../types";

const FILTERS = ["all", "claude", "cursor", "codex", "graphify"];

export function LiveLog() {
  const { logs } = useFeed();
  const [filter, setFilter] = useState("all");
  const [stick, setStick] = useState(true);
  const [open, setOpen] = useState<number | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const shown = useMemo(
    () => logs.filter((event) => filter === "all" || event.agent === filter),
    [logs, filter],
  );

  useEffect(() => {
    if (stick && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [shown, stick]);

  return (
    <>
      <div className="log-head">
        <div className="filters">
          {FILTERS.map((item) => (
            <button key={item} type="button" className={item === filter ? "on" : ""} onClick={() => setFilter(item)}>
              {item === "all" ? "All" : roleLabel(item)}
            </button>
          ))}
        </div>
        <button type="button" className={stick ? "toggle on" : "toggle"} onClick={() => setStick((value) => !value)}>
          Auto-scroll {stick ? "on" : "off"}
        </button>
      </div>
      <div className="log" ref={box}>
        {shown.length === 0 ? <div className="empty">No run files under the logs directory yet.</div> : null}
        {shown.map((event) => (
          <LogLine key={event.seq} event={event} open={open === event.seq} onToggle={() => setOpen(open === event.seq ? null : event.seq)} />
        ))}
      </div>
    </>
  );
}

function LogLine({ event, open, onToggle }: { event: NormEvent; open: boolean; onToggle: () => void }) {
  const transcript = event.kind === "transcript";
  const review = event.kind === "review";
  const text = typeof event.detail.text === "string" ? event.detail.text : "";
  return (
    <div className="log-line" onClick={transcript ? undefined : onToggle}>
      <time>{hm(event.ts)}</time>
      <span style={{ color: agentColor(event.agent) }}>[{roleLabel(event.agent)}]</span>
      <span>{event.summary}</span>
      {review && text ? <pre className="detail review">{text}</pre> : null}
      {transcript ? (
        <button
          type="button"
          className="toggle"
          onClick={(click) => {
            click.stopPropagation();
            onToggle();
          }}
        >
          {open ? "Hide transcript" : "Show transcript"}
        </button>
      ) : null}
      {open && transcript ? <pre className="detail">{text}</pre> : null}
      {open && !transcript && !review ? <pre className="detail">{detailText(event)}</pre> : null}
    </div>
  );
}

function detailText(event: NormEvent): string {
  const detail = event.detail;
  const parts: string[] = [];
  if (typeof detail.text === "string") parts.push(detail.text);
  if (detail.input !== undefined && detail.input !== null) parts.push(`input\n${JSON.stringify(detail.input, null, 2)}`);
  if (typeof detail.output === "string" && detail.output) parts.push(`output\n${detail.output}`);
  if (parts.length === 0) return JSON.stringify(detail, null, 2);
  return parts.join("\n\n");
}
