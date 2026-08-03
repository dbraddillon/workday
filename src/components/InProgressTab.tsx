import { useEffect, useState } from "react";
import { api } from "../api";
import type { Issue } from "../types";
import { IssueRow } from "./IssueRow";
import { EmptyState } from "./EmptyState";

export function InProgressTab({ dataVersion }: { dataVersion: number }) {
  const [issues, setIssues] = useState<Issue[] | null>(null);

  useEffect(() => {
    api.getInProgress().then(setIssues).catch(() => setIssues([]));
  }, [dataVersion]);

  if (issues === null) return <div className="loading">Loading…</div>;
  if (issues.length === 0)
    return (
      <EmptyState
        title="No work in progress"
        hint="Anything assigned to you that isn't Done will show here after a sync."
      />
    );

  return (
    <div className="list">
      {issues.map((i) => (
        <IssueRow key={i.issue_key} issue={i} />
      ))}
    </div>
  );
}
