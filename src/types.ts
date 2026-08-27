// Mirrors the normalized Rust model (src-tauri/src/model.rs) and settings.
// Kept hand-synced for v1; small enough that a codegen step isn't worth it yet.

export interface Issue {
  source: string;
  source_issue_id: string;
  issue_key: string;
  summary: string;
  status_name: string;
  status_category: string; // "new" | "indeterminate" | "done"
  assignee_display?: string | null;
  reporter_display?: string | null;
  project_key?: string | null;
  project_name?: string | null;
  updated_at: string;
  created_at: string;
  browse_url: string;
  labels: string[];
}

export interface PullRequest {
  repo: string;
  number: number;
  title: string;
  url: string;
  author: string;
  created_at: string;
  updated_at: string;
  review_decision: string; // "REVIEW_REQUIRED" | "CHANGES_REQUESTED" | "APPROVED" | "NONE"
  additions: number;
  deletions: number;
  changed_files: number;
  // Bot reviewers (Copilot) are filtered out in the connector.
  human_reviewers: string[];
  // "team" | "authored" | "direct" | "assigned"; a PR can match several.
  reasons: string[];
  // Matched by a reason that ignores the age window (direct request/assignment).
  is_direct: boolean;
  reviewed_at?: string | null;
}

// A review the user submitted, as GitHub recorded it. Not a subset of the queue:
// a reviewed PR usually merges within a day and so leaves the open queue.
export interface SubmittedReview {
  repo: string;
  number: number;
  title: string;
  url: string;
  // PR author, so the row reads as "reviewed X's change".
  author: string;
  submitted_at: string;
  // "APPROVED" | "CHANGES_REQUESTED" | "COMMENTED" | "DISMISSED"
  state: string;
  // "OPEN" | "MERGED" | "CLOSED"
  pr_state: string;
}

export interface ReviewQueue {
  prs: PullRequest[];
  // Pre-cap count, so the UI can say "showing N of M".
  total: number;
  done: SubmittedReview[];
  gh_available: boolean;
  enabled: boolean;
}

export interface SyncStatus {
  last_run_at?: string | null;
  last_success_at?: string | null;
  ok: boolean;
  message?: string | null;
  issue_count: number;
}

export interface TimeRange {
  start: string;
  end: string;
  label: string;
}

export interface StandupItem {
  issue_key: string;
  summary: string;
  status_name: string;
  status_category: string;
  project_key?: string | null;
  browse_url: string;
  activity_notes: string[];
  is_carryover: boolean;
  included: boolean;
}

export interface StandupSection {
  key: string;
  title: string;
  items: StandupItem[];
}

export interface StandupNarrative {
  doing: string;
  pairing: string;
  post_scrum: string;
  // Populated by the backend from settings; the UI edits only the three above.
  blocker?: string;
  prompt_doing?: string;
  prompt_working?: string;
  prompt_pairing?: string;
  prompt_blocker?: string;
  prompt_post_scrum?: string;
}

export interface StandupModel {
  time_range: TimeRange;
  sections: StandupSection[];
  blockers: string[];
  narrative: StandupNarrative;
  // PRs ticked off in the window; 0 renders no line at all.
  reviewed_pr_count: number;
}

export interface StandupDraft {
  formatter_key: string;
  time_range: TimeRange;
  text: string;
  // Set to an error string when AI polish was requested but fell back to the
  // deterministic draft; null/absent otherwise.
  ai_polish_fell_back?: string | null;
}

export interface AppSettings {
  jira_base_url: string;
  jira_email: string;
  jira_jql_in_progress: string;
  jira_jql_recent: string;
  refresh_interval_secs: number;
  default_recent_range: string;
  default_formatter: string;
  ai_polish_enabled: boolean;
  fake_data_mode: boolean;
  has_jira_token: boolean;
  // Thread template — prompt emoji (left) + answer defaults (right).
  thread_prompt_doing: string;
  thread_prompt_working: string;
  thread_prompt_pairing: string;
  thread_prompt_blocker: string;
  thread_prompt_post_scrum: string;
  thread_doing: string;
  thread_pairing: string;
  thread_blocker: string;
  thread_post_scrum: string;
  // GitHub review queue. No token: the backend shells out to the authed `gh` CLI.
  github_enabled: boolean;
  github_org: string;
  github_login: string;
  github_teams: string; // comma-separated slugs
  github_window_days: number;
  github_include_team_authored: boolean;
  github_max_results: number;
}

// "standup" is the day-aware window: Mon/Sun reach back to Friday, else yesterday.
export type RecentRange = "standup" | "today" | "24h" | "3d" | "7d";
export type Tab = "in_progress" | "recent" | "reviews" | "standup";
