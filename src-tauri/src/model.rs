//! Normalized domain model.
//!
//! These types are the *seam* between raw source data (Jira today, maybe others
//! later) and everything downstream (UI, standup composer, delivery). Nothing
//! outside `connector::jira` should ever touch a raw Jira JSON shape — it all
//! flows through these normalized structs. That decoupling is the design rule
//! from the handoff doc: the standup feature consumes normalized data, never raw
//! Jira responses.

use serde::{Deserialize, Serialize};

/// A single work item (a Jira issue in v1), normalized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub source: String, // "jira" — leaves room for other sources later
    pub source_issue_id: String,
    pub issue_key: String,
    pub summary: String,
    pub status_name: String,
    pub status_category: String, // "new" | "indeterminate" | "done" (Jira's categories)
    pub assignee_display: Option<String>,
    pub reporter_display: Option<String>,
    pub project_key: Option<String>,
    pub project_name: Option<String>,
    pub updated_at: String, // RFC3339
    pub created_at: String, // RFC3339
    pub browse_url: String,
    pub labels: Vec<String>,
}

/// A normalized activity event derived from Jira changelog/comments.
/// Deliberately broad — enough signal for "recent work" + standup, not full
/// event sourcing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub issue_key: String,
    pub activity_type: String, // "status_change" | "comment" | "created" | "assigned" | ...
    pub activity_at: String,   // RFC3339
    pub actor_display: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub text_summary: Option<String>,
}

/// A pull request awaiting review, normalized. Parallel to `Issue`: the raw
/// GitHub GraphQL shape stays inside `connector::github`, same rule as Jira.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub repo: String,   // "provider-domain-service" (name only; org is implied)
    pub number: i64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub created_at: String, // RFC3339
    pub updated_at: String, // RFC3339
    /// GitHub's rollup: "REVIEW_REQUIRED" | "CHANGES_REQUESTED" | "APPROVED" | "NONE".
    pub review_decision: String,
    pub additions: i64,
    pub deletions: i64,
    pub changed_files: i64,
    /// Human reviewers who have left a review. Bot reviewers (Copilot) are
    /// filtered out in the connector: they inflate the count and make an
    /// unreviewed PR look attended-to.
    pub human_reviewers: Vec<String>,
    /// Why this PR is in the list: "team" (a team of mine was requested),
    /// "authored" (a teammate opened it), "direct" (review requested of me
    /// personally), "assigned" (assigned to me). A PR can match several.
    pub reasons: Vec<String>,
    /// True when matched by a reason that ignores the age window (direct request
    /// or assignment). Sorted to the top and never aged out.
    pub is_direct: bool,
    /// Set once the user ticks the PR off in the Reviews tab. Carries the
    /// RFC3339 checkoff time so the standup composer can count it in a window.
    pub reviewed_at: Option<String>,
}

/// A review the user actually submitted on GitHub. Distinct from a checkoff: this
/// is observed fact (GitHub recorded the review), whereas a checkoff is the user
/// asserting they handled a PR. Both feed the standup's review line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmittedReview {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub url: String,
    /// PR author, so the row reads as "reviewed X's change".
    pub author: String,
    /// When the review was submitted. This is the review's own timestamp, NOT the
    /// PR's `updatedAt` — filtering on the latter counts old reviews on
    /// recently-touched PRs and badly overstates a day's work.
    pub submitted_at: String,
    /// "APPROVED" | "CHANGES_REQUESTED" | "COMMENTED" | "DISMISSED".
    pub state: String,
    /// PR state at fetch time: "OPEN" | "MERGED" | "CLOSED". Most reviewed PRs
    /// merge quickly, which is why they can't be found in the open queue.
    pub pr_state: String,
}

/// Result of a sync run, surfaced to the UI for freshness/error display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_run_at: Option<String>,
    pub last_success_at: Option<String>,
    pub ok: bool,
    pub message: Option<String>,
    pub issue_count: i64,
}

// ---------------------------------------------------------------------------
// Standup intermediate model — format-agnostic (the doc's key requirement).
//
// A composer builds one of these from normalized data over a time range.
// A *formatter* then renders it to text. Swapping formatters (bullets, grouped
// by project, example-mirrored style, LLM-polished) never touches retrieval.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: String, // RFC3339
    pub end: String,   // RFC3339
    pub label: String, // human label e.g. "Last 24 hours"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupItem {
    pub issue_key: String,
    pub summary: String,
    pub status_name: String,
    pub status_category: String,
    pub project_key: Option<String>,
    pub browse_url: String,
    /// Human-readable notes about what happened in the window (status moves,
    /// comments, etc.), derived from ActivityEvents.
    pub activity_notes: Vec<String>,
    /// Whether this item looks like it's still in flight (carryover) vs done.
    pub is_carryover: bool,
    pub included: bool, // user can toggle before final render
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupSection {
    pub key: String,   // e.g. "in_progress", "done", "blocked", or a project key
    pub title: String, // display heading
    pub items: Vec<StandupItem>,
}

/// Free-form, user-authored standup answers that aren't derived from work items
/// (how you're doing, pairing availability, post-scrum notes). Kept in the model
/// so it stays format-agnostic — a formatter decides how/whether to render them.
/// Optional so formatters that don't need it (and older serialized drafts) are
/// unaffected.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StandupNarrative {
    // Answers (right side) for the non-Jira lines.
    pub doing: String,
    pub pairing: String,
    pub post_scrum: String,
    /// Fallback for the blocker line when Jira reports no blockers.
    #[serde(default)]
    pub blocker: String,
    // Prompt emoji (left side / subject) for each of the five lines. Carried on
    // the model so the thread formatter renders the exact team template without
    // reaching into settings. Empty strings fall back to the formatter's builtin.
    #[serde(default)]
    pub prompt_doing: String,
    #[serde(default)]
    pub prompt_working: String,
    #[serde(default)]
    pub prompt_pairing: String,
    #[serde(default)]
    pub prompt_blocker: String,
    #[serde(default)]
    pub prompt_post_scrum: String,
}

/// The normalized, format-agnostic standup model. Formatters consume this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupModel {
    pub time_range: TimeRange,
    pub sections: Vec<StandupSection>,
    /// Free-form markers the user or a future source might surface.
    pub blockers: Vec<String>,
    /// User-authored freeform answers (defaults sourced from settings). Present
    /// for thread-style formatters; ignored by others.
    #[serde(default)]
    pub narrative: StandupNarrative,
    /// PRs the user ticked off in the Reviews tab within this window. A count,
    /// not a list: the formatter decides whether and how to render it, and the
    /// model stays format-agnostic. Zero means "say nothing".
    #[serde(default)]
    pub reviewed_pr_count: i64,
}

/// A rendered draft ready for review/delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupDraft {
    pub formatter_key: String,
    pub time_range: TimeRange,
    pub text: String,
    /// True when AI polish was requested but failed/unavailable, so `text` is
    /// the deterministic draft. Lets the UI surface the fallback instead of it
    /// being silent. `None` when polish wasn't requested.
    #[serde(default)]
    pub ai_polish_fell_back: Option<String>,
}
