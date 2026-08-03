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

export interface StandupModel {
  time_range: TimeRange;
  sections: StandupSection[];
  blockers: string[];
}

export interface StandupDraft {
  formatter_key: string;
  time_range: TimeRange;
  text: string;
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
}

export type RecentRange = "today" | "24h" | "3d" | "7d";
export type Tab = "in_progress" | "recent" | "standup";
