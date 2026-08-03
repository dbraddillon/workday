import type { Issue } from "../types";
import { openExternal, relativeTime, statusClass } from "../util";

/** A dense, keyboard-openable issue row. Enter/click opens Jira in the browser. */
export function IssueRow({ issue }: { issue: Issue }) {
  return (
    <div
      className="issue-row"
      tabIndex={0}
      role="button"
      onClick={() => openExternal(issue.browse_url)}
      onKeyDown={(e) => {
        if (e.key === "Enter") openExternal(issue.browse_url);
      }}
      title={`${issue.status_name} · updated ${relativeTime(issue.updated_at)}`}
    >
      <span className={`dot ${statusClass(issue.status_category)}`} />
      <div className="issue-main">
        <div className="issue-top">
          <span className="issue-key">{issue.issue_key}</span>
          <span className="issue-status">{issue.status_name}</span>
        </div>
        <div className="issue-summary">{issue.summary}</div>
      </div>
      <span className="issue-updated">{relativeTime(issue.updated_at)}</span>
    </div>
  );
}
