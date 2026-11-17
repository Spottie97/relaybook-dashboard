import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import { Link, useSearchParams } from "react-router-dom";
import remarkGfm from "remark-gfm";

import { getArtifact } from "../api";
import { useFeed } from "../feed";
import type { ArtifactBody } from "../types";

export function ArtifactsPage() {
  const { workspace } = useFeed();
  const [params] = useSearchParams();
  const selected = params.get("id") ?? workspace?.artifacts[0]?.id ?? "";
  const [body, setBody] = useState<ArtifactBody | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspace || !selected) {
      setBody(null);
      return;
    }
    let stop = false;
    setError(null);
    getArtifact(workspace.id, selected)
      .then((next) => {
        if (!stop) setBody(next);
      })
      .catch(() => {
        if (!stop) setError("Could not read that artifact.");
      });
    return () => {
      stop = true;
    };
  }, [workspace, selected]);

  const items = workspace?.artifacts ?? [];
  return (
    <>
      <h1>Artifacts</h1>
      <p className="sub">Reports and decisions from the ledger, plus files from the artifacts directory.</p>
      {items.length === 0 ? <div className="empty">Nothing to show yet.</div> : null}
      <div className="split" style={{ marginTop: 14 }}>
        <div className="list">
          {items.map((item) => (
            <Link key={item.id} className="row" to={`/artifacts?id=${encodeURIComponent(item.id)}`}>
              <b>{item.name}</b>
              <span>{item.kind}</span>
              <small>{item.detail}</small>
            </Link>
          ))}
        </div>
        <div className="viewer">
          {error ? <div className="banner">{error}</div> : null}
          {!body ? <div className="empty">Select an artifact.</div> : null}
          {body && (body.language === "markdown" || body.kind === "decision" || body.name.endsWith(".md")) ? (
            <div className="markdown">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{body.content}</ReactMarkdown>
            </div>
          ) : null}
          {body && !(body.language === "markdown" || body.kind === "decision" || body.name.endsWith(".md")) ? (
            <pre><code>{body.content}</code></pre>
          ) : null}
          {body?.truncated ? <p className="note">Truncated.</p> : null}
        </div>
      </div>
    </>
  );
}
