import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { PullRequest, ReviewQueue, SubmittedReview } from "../types";
import { openExternal, relativeTime } from "../util";
import { EmptyState } from "./EmptyState";

type SortKey = "waiting" | "newest" | "size" | "repo";

const SORTS: { key: SortKey; label: string; title: string }[] = [
  { key: "waiting", label: "Waiting", title: "Longest open first" },
  { key: "newest", label: "Newest", title: "Most recently opened first" },
  { key: "size", label: "Size", title: "Smallest first, quick wins" },
  { key: "repo", label: "Repo", title: "Grouped by repository" },
];

/**
 * Sort the queue. Direct requests and assignments stay pinned to the top under
 * every key: they're the trump rule, and burying one behind a sort choice is the
 * failure the tab exists to prevent.
 */
function sortQueue(prs: PullRequest[], key: SortKey): PullRequest[] {
  const churn = (p: PullRequest) => p.additions + p.deletions;
  return [...prs].sort((a, b) => {
    if (a.is_direct !== b.is_direct) return a.is_direct ? -1 : 1;
    switch (key) {
      case "waiting":
        return a.created_at.localeCompare(b.created_at);
      case "newest":
        return b.created_at.localeCompare(a.created_at);
      case "size":
        return churn(a) - churn(b);
      case "repo":
        return a.repo.localeCompare(b.repo) || a.number - b.number;
    }
  });
}

/** Short label for why a PR is in the queue. */
function reasonLabel(pr: PullRequest): string {
  if (pr.reasons.includes("assigned")) return "assigned to you";
  if (pr.reasons.includes("direct")) return "your review requested";
  if (pr.reasons.includes("team")) return "team requested";
  return "from your team";
}

/**
 * Review state as it matters for "is anyone actually looking at this".
 * `human_reviewers` already excludes bots, so a PR reviewed only by Copilot
 * correctly reads as unreviewed.
 */
function reviewState(pr: PullRequest): { label: string; cls: string } {
  if (pr.review_decision === "CHANGES_REQUESTED")
    return { label: "changes requested", cls: "pr-state-changes" };
  if (pr.review_decision === "APPROVED")
    return { label: "approved", cls: "pr-state-approved" };
  if (pr.human_reviewers.length > 0)
    return { label: `${pr.human_reviewers.length} reviewing`, cls: "pr-state-partial" };
  return { label: "no reviewers", cls: "pr-state-none" };
}

/** Rough size band, so a 9k-line PR is visibly not a quick look. */
function sizeLabel(pr: PullRequest): string {
  const n = pr.additions + pr.deletions;
  if (n >= 2000) return "XL";
  if (n >= 500) return "L";
  if (n >= 100) return "M";
  return "S";
}

function PrRow({
  pr,
  onToggle,
}: {
  pr: PullRequest;
  onToggle: (pr: PullRequest, reviewed: boolean) => void;
}) {
  const checked = !!pr.reviewed_at;
  const state = reviewState(pr);
  return (
    <div className={`pr-row ${checked ? "pr-row-done" : ""}`}>
      <input
        type="checkbox"
        className="pr-check"
        checked={checked}
        onChange={(e) => onToggle(pr, e.target.checked)}
        title={checked ? "Reviewed, counts in your standup" : "Mark as reviewed"}
        aria-label={`Mark ${pr.repo} #${pr.number} reviewed`}
      />
      <div
        className="pr-main"
        tabIndex={0}
        role="button"
        onClick={() => openExternal(pr.url)}
        onKeyDown={(e) => {
          if (e.key === "Enter") openExternal(pr.url);
        }}
        title={`${pr.title}\n${pr.additions} added, ${pr.deletions} removed across ${pr.changed_files} files`}
      >
        <div className="pr-top">
          <span className="pr-repo">
            {pr.repo}#{pr.number}
          </span>
          {pr.is_direct && <span className="pr-flag">you</span>}
          <span className={`pr-state ${state.cls}`}>{state.label}</span>
          <span className="pr-size">{sizeLabel(pr)}</span>
        </div>
        <div className="pr-title">{pr.title}</div>
        <div className="pr-meta">
          {pr.author} · opened {relativeTime(pr.created_at)} · {reasonLabel(pr)}
        </div>
      </div>
    </div>
  );
}

/** Verdict shorthand for a review the user left. */
function verdictLabel(state: string): { label: string; cls: string } {
  switch (state) {
    case "APPROVED":
      return { label: "approved", cls: "pr-state-approved" };
    case "CHANGES_REQUESTED":
      return { label: "changes", cls: "pr-state-changes" };
    case "DISMISSED":
      return { label: "dismissed", cls: "" };
    default:
      return { label: "commented", cls: "" };
  }
}

function DoneRow({ r }: { r: SubmittedReview }) {
  const verdict = verdictLabel(r.state);
  return (
    <div
      className="pr-row"
      tabIndex={0}
      role="button"
      onClick={() => openExternal(r.url)}
      onKeyDown={(e) => {
        if (e.key === "Enter") openExternal(r.url);
      }}
      title={r.title}
    >
      <div className="pr-main">
        <div className="pr-top">
          <span className="pr-repo">
            {r.repo}#{r.number}
          </span>
          <span className={`pr-state ${verdict.cls}`}>{verdict.label}</span>
          {r.pr_state !== "OPEN" && (
            <span className="pr-size">{r.pr_state.toLowerCase()}</span>
          )}
        </div>
        <div className="pr-title">{r.title}</div>
        <div className="pr-meta">
          {r.author} · reviewed {relativeTime(r.submitted_at)}
        </div>
      </div>
    </div>
  );
}

/** Day heading for the Done list, e.g. "Today · 27". */
function dayLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  if (same(d, today)) return "Today";
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (same(d, yesterday)) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" });
}

/**
 * Group reviews by day. Counts distinct PRs, not review rows: three passes over
 * one PR is one review, and matches what the standup line credits.
 */
function groupByDay(
  reviews: SubmittedReview[],
): { label: string; count: number; rows: SubmittedReview[] }[] {
  const out: { label: string; count: number; rows: SubmittedReview[] }[] = [];
  for (const r of reviews) {
    const label = dayLabel(r.submitted_at);
    let group = out.find((g) => g.label === label);
    if (!group) {
      group = { label, count: 0, rows: [] };
      out.push(group);
    }
    group.rows.push(r);
  }
  for (const g of out) {
    g.count = new Set(g.rows.map((r) => `${r.repo}#${r.number}`)).size;
  }
  return out;
}

export function ReviewsTab({
  dataVersion,
  onOpenSettings,
}: {
  dataVersion: number;
  onOpenSettings: () => void;
}) {
  const [queue, setQueue] = useState<ReviewQueue | null>(null);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<"waiting" | "done">("waiting");
  const [sort, setSort] = useState<SortKey>("waiting");

  const load = useCallback(async () => {
    try {
      setQueue(await api.getReviewQueue());
    } catch {
      setQueue({ prs: [], total: 0, done: [], gh_available: false, enabled: false });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load, dataVersion]);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setQueue(await api.refreshReviews());
    } catch {
      await load();
    } finally {
      setBusy(false);
    }
  }, [load]);

  const toggle = useCallback(
    async (pr: PullRequest, reviewed: boolean) => {
      // Optimistic: the checkbox is the whole interaction, so it must not lag.
      setQueue((q) =>
        q
          ? {
              ...q,
              prs: q.prs.map((p) =>
                p.repo === pr.repo && p.number === pr.number
                  ? { ...p, reviewed_at: reviewed ? new Date().toISOString() : null }
                  : p,
              ),
            }
          : q,
      );
      try {
        await api.setPrReviewed(pr.repo, pr.number, reviewed);
      } catch {
        await load(); // put the real state back
      }
    },
    [load],
  );

  // Before the early returns: hooks can't run conditionally.
  const sorted = useMemo(() => sortQueue(queue?.prs ?? [], sort), [queue?.prs, sort]);

  if (queue === null) return <div className="loading">Loading…</div>;

  if (!queue.enabled) {
    return (
      <EmptyState
        title="Reviews are off"
        hint="Turn on the GitHub review queue in Settings and set your org, login, and team."
      />
    );
  }

  if (!queue.gh_available) {
    return (
      <EmptyState
        title="`gh` not available"
        hint="The review queue uses the GitHub CLI. Install it and run `gh auth login`."
      />
    );
  }

  const hidden = queue.total - queue.prs.length;
  // Distinct PRs, matching what the standup line credits.
  const doneCount = new Set(queue.done.map((r) => `${r.repo}#${r.number}`)).size;

  return (
    <div className="reviews">
      <div className="segmented">
        <button
          className={`seg ${view === "waiting" ? "seg-active" : ""}`}
          onClick={() => setView("waiting")}
        >
          Waiting ({queue.prs.length})
        </button>
        <button
          className={`seg ${view === "done" ? "seg-active" : ""}`}
          onClick={() => setView("done")}
          title="Reviews you submitted in the last 7 days"
        >
          Done ({doneCount})
        </button>
      </div>

      <div className="reviews-bar">
        {view === "waiting" ? (
          <span className="reviews-sorts">
            {SORTS.map((s) => (
              <button
                key={s.key}
                className={`linkish ${sort === s.key ? "linkish-active" : ""}`}
                onClick={() => setSort(s.key)}
                title={s.title}
              >
                {s.label}
              </button>
            ))}
          </span>
        ) : (
          <span className="reviews-count">last 7 days</span>
        )}
        <button className="linkish" onClick={onOpenSettings}>
          Filters
        </button>
        <button className="linkish" onClick={refresh} disabled={busy}>
          {busy ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {view === "waiting" ? (
        <>
          {hidden > 0 && (
            <div className="reviews-note">
              Showing {queue.prs.length} of {queue.total}. Raise the cap in Settings.
            </div>
          )}
          {queue.prs.length === 0 ? (
            <EmptyState title="Nothing waiting" hint="No open PRs match your filters." />
          ) : (
            <div className="list">
              {sorted.map((pr) => (
                <PrRow key={`${pr.repo}#${pr.number}`} pr={pr} onToggle={toggle} />
              ))}
            </div>
          )}
        </>
      ) : queue.done.length === 0 ? (
        <EmptyState
          title="No reviews yet"
          hint="Reviews you submit on GitHub show up here, and count toward your standup."
        />
      ) : (
        <div className="list">
          {groupByDay(queue.done).map((g) => (
            <div key={g.label}>
              <div className="standup-section-title">
                {g.label} · {g.count}
              </div>
              {g.rows.map((r) => (
                <DoneRow key={`${r.repo}#${r.number}@${r.submitted_at}`} r={r} />
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
