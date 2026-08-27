//! App settings (non-secret) and secret storage (Keychain).
//!
//! Split on purpose: non-secret settings live in SQLite (`app_settings`),
//! secrets (Jira API token, later Slack tokens) live in the macOS Keychain and
//! are *never* written to SQLite or the repo.

use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "com.dbraddillon.workday";

/// Non-secret settings, persisted in SQLite. The Jira API token is intentionally
/// absent here — it lives in the Keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub jira_base_url: String,      // e.g. https://yourorg.atlassian.net
    pub jira_email: String,         // Atlassian account email (Basic auth username)
    pub jira_jql_in_progress: String,
    pub jira_jql_recent: String,
    pub refresh_interval_secs: u64,
    pub default_recent_range: String, // "today" | "24h" | "3d" | "7d"
    pub default_formatter: String,    // formatter key
    pub ai_polish_enabled: bool,      // opt-in: run the Claude CLI over the deterministic draft
    /// If true, the app serves fake data and never calls Jira. Great for dev
    /// and for a first-run demo before credentials are entered.
    pub fake_data_mode: bool,
    // --- Standup thread template (used by the "thread" formatter) ---
    // The whole 5-prompt template is editable so it can mirror a team's exact
    // standup thread. Each prompt has a left-side emoji (the subject) and, for
    // the non-Jira lines, a default answer. The working-on line is always derived
    // from Jira; blockers derive from Jira but fall back to `thread_blocker`.
    //
    // Prompt emoji (left side):
    #[serde(default = "default_prompt_doing")]
    pub thread_prompt_doing: String,
    #[serde(default = "default_prompt_working")]
    pub thread_prompt_working: String,
    #[serde(default = "default_prompt_pairing")]
    pub thread_prompt_pairing: String,
    #[serde(default = "default_prompt_blocker")]
    pub thread_prompt_blocker: String,
    #[serde(default = "default_prompt_post_scrum")]
    pub thread_prompt_post_scrum: String,
    // Default answers (right side) for the non-Jira lines:
    /// Default answer for the "how are you doing" prompt.
    #[serde(default = "default_thread_doing")]
    pub thread_doing: String,
    /// Default answer for the "pairing opportunities" prompt.
    #[serde(default = "default_thread_pairing")]
    pub thread_pairing: String,
    /// Fallback for the blocker line when Jira reports no blockers.
    #[serde(default = "default_thread_blocker")]
    pub thread_blocker: String,
    /// Default answer for the "post scrum" prompt.
    #[serde(default = "default_thread_post_scrum")]
    pub thread_post_scrum: String,
    /// Whether a Jira token is present in the Keychain (derived, never stored).
    #[serde(default)]
    pub has_jira_token: bool,
    // --- GitHub review queue (Reviews tab) ---
    // No token here or in the Keychain: the connector shells out to `gh`, which
    // already holds the credential.
    /// Whether to fetch the review queue at all.
    #[serde(default)]
    pub github_enabled: bool,
    /// Org login, e.g. "healthsparq".
    #[serde(default)]
    pub github_org: String,
    /// The viewer's GitHub login. Used to exclude their own PRs and to resolve
    /// the direct-request/assignee trump rules.
    #[serde(default)]
    pub github_login: String,
    /// Team slugs whose review requests count, comma-separated. Must be narrow:
    /// an org-wide team returns hundreds of open PRs and swamps the tab.
    #[serde(default)]
    pub github_teams: String,
    /// Age window in days, applied to PR creation date.
    #[serde(default = "default_github_window_days")]
    pub github_window_days: u32,
    /// Also include PRs authored by team members, not just PRs where a team was
    /// tagged for review. These overlap far less than expected — a PR opened
    /// without tagging anyone is invisible to the team-tagged query.
    #[serde(default = "default_true")]
    pub github_include_team_authored: bool,
    /// Cap on rows shown. The count before capping is reported separately so the
    /// UI can say "showing N of M".
    #[serde(default = "default_github_max_results")]
    pub github_max_results: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            jira_base_url: String::new(),
            jira_email: String::new(),
            // "My open work" and "My recently updated work" — reasonable defaults.
            jira_jql_in_progress:
                "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
                    .to_string(),
            jira_jql_recent:
                "assignee = currentUser() AND updated >= -7d ORDER BY updated DESC".to_string(),
            refresh_interval_secs: 300,
            default_recent_range: "24h".to_string(),
            default_formatter: "thread".to_string(),
            ai_polish_enabled: false,
            fake_data_mode: true, // start in fake mode until the user configures Jira
            has_jira_token: false,
            thread_prompt_doing: default_prompt_doing(),
            thread_prompt_working: default_prompt_working(),
            thread_prompt_pairing: default_prompt_pairing(),
            thread_prompt_blocker: default_prompt_blocker(),
            thread_prompt_post_scrum: default_prompt_post_scrum(),
            thread_doing: default_thread_doing(),
            thread_pairing: default_thread_pairing(),
            thread_blocker: default_thread_blocker(),
            thread_post_scrum: default_thread_post_scrum(),
            github_enabled: false,
            github_org: String::new(),
            github_login: String::new(),
            github_teams: String::new(),
            github_window_days: default_github_window_days(),
            github_include_team_authored: default_true(),
            github_max_results: default_github_max_results(),
        }
    }
}

impl AppSettings {
    /// `github_teams` split into slugs, trimmed, empties dropped.
    pub fn github_team_list(&self) -> Vec<String> {
        self.github_teams
            .split(',')
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect()
    }
}

fn default_true() -> bool {
    true
}
fn default_github_window_days() -> u32 {
    7
}
fn default_github_max_results() -> u32 {
    40
}

// Prompt emoji defaults (the team's standup thread subjects).
fn default_prompt_doing() -> String {
    ":city_sunrise:".to_string()
}
fn default_prompt_working() -> String {
    ":computer:".to_string()
}
fn default_prompt_pairing() -> String {
    ":two-peas-in-a-pod:".to_string()
}
fn default_prompt_blocker() -> String {
    ":blocker:".to_string()
}
fn default_prompt_post_scrum() -> String {
    ":high-five:".to_string()
}

// Answer defaults for the non-Jira lines.
fn default_thread_doing() -> String {
    ":batman: :thumbsup_all:".to_string()
}
fn default_thread_pairing() -> String {
    ":available:".to_string()
}
fn default_thread_blocker() -> String {
    "Nope".to_string()
}
fn default_thread_post_scrum() -> String {
    "Nope".to_string()
}

// ---------------------------------------------------------------------------
// Keychain — secret storage. Isolated here so the auth mechanism is one seam.
// ---------------------------------------------------------------------------

const JIRA_TOKEN_ACCOUNT: &str = "jira_api_token";

pub fn set_jira_token(token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, JIRA_TOKEN_ACCOUNT)
        .map_err(|e| format!("keychain: {e}"))?;
    entry.set_password(token).map_err(|e| format!("keychain: {e}"))
}

pub fn get_jira_token() -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, JIRA_TOKEN_ACCOUNT).ok()?;
    entry.get_password().ok()
}

pub fn delete_jira_token() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, JIRA_TOKEN_ACCOUNT)
        .map_err(|e| format!("keychain: {e}"))?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain: {e}")),
    }
}
