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

/// The normalized, format-agnostic standup model. Formatters consume this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupModel {
    pub time_range: TimeRange,
    pub sections: Vec<StandupSection>,
    /// Free-form markers the user or a future source might surface.
    pub blockers: Vec<String>,
}

/// A rendered draft ready for review/delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupDraft {
    pub formatter_key: String,
    pub time_range: TimeRange,
    pub text: String,
}
