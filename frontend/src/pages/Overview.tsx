import { Link } from "react-router-dom";

import { useFeed } from "../feed";

export function OverviewExtras() {
  const { workspace } = useFeed();
  const artifacts = workspace?.artifacts ?? [];
  return (
    <section className="section">
      <h2>Recent artifacts</h2>
      {artifacts.length === 0 ? (
        <div className="empty">No reports in the ledger and no files in the artifacts directory.</div>
      ) : (
        <div className="artifacts">
          {artifacts.map((artifact) => (
            <Link key={artifact.id} className="artifact" to={`/artifacts?id=${encodeURIComponent(artifact.id)}`}>
              <div className="mark">F</div>
              <b>{artifact.name}</b>
              <span>{artifact.detail}</span>
            </Link>
          ))}
        </div>
      )}
    </section>
  );
}
