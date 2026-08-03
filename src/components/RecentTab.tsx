import { useEffect, useState } from "react";
import { api } from "../api";
import type { Issue, RecentRange } from "../types";
import { IssueRow } from "./IssueRow";
import { EmptyState } from "./EmptyState";

const RANGES: { id: RecentRange; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "24h", label: "24h" },
  { id: "3d", label: "3 days" },
  { id: "7d", label: "7 days" },
];

export function RecentTab({
  dataVersion,
  defaultRange,
}: {
  dataVersion: number;
  defaultRange: string;
}) {
  const [range, setRange] = useState<RecentRange>((defaultRange as RecentRange) ?? "24h");
  const [issues, setIssues] = useState<Issue[] | null>(null);

  useEffect(() => {
    setIssues(null);
    api.getRecent(range).then(setIssues).catch(() => setIssues([]));
  }, [range, dataVersion]);

  return (
    <div className="recent">
      <div className="segmented">
        {RANGES.map((r) => (
          <button
            key={r.id}
            className={`seg ${range === r.id ? "seg-active" : ""}`}
            onClick={() => setRange(r.id)}
          >
            {r.label}
          </button>
        ))}
      </div>
      {issues === null ? (
        <div className="loading">Loading…</div>
      ) : issues.length === 0 ? (
        <EmptyState title="Nothing recent" hint="No work touched in this window." />
      ) : (
        <div className="list">
          {issues.map((i) => (
            <IssueRow key={i.issue_key} issue={i} />
          ))}
        </div>
      )}
    </div>
  );
}
