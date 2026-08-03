//! Jira Cloud connector.
//!
//! Auth: Basic (email + API token) over HTTPS. The token is read-only in
//! practice and long-lived — see docs/SETUP.md. Uses the JQL search API with
//! pagination, and derives activity events from the changelog + recent comments.
//!
//! v1 keeps this deliberately lean: only the fields the glance UI and standup
//! need. Raw responses are normalized into `crate::model` before leaving here.

use crate::connector::WorkSourceConnector;
use crate::model::{ActivityEvent, Issue};
use base64::Engine;
use serde_json::Value;

pub struct JiraConnector {
    base_url: String, // e.g. https://org.atlassian.net (no trailing slash)
    email: String,
    token: String,
    jql_in_progress: String,
    jql_recent: String,
    client: reqwest::Client,
}

impl JiraConnector {
    pub fn new(
        base_url: String,
        email: String,
        token: String,
        jql_in_progress: String,
        jql_recent: String,
    ) -> Self {
        JiraConnector {
            base_url: base_url.trim_end_matches('/').to_string(),
            email,
            token,
            jql_in_progress,
            jql_recent,
            client: reqwest::Client::new(),
        }
    }

    fn auth_header(&self) -> String {
        let raw = format!("{}:{}", self.email, self.token);
        format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(raw))
    }

    /// Fields we ask Jira for — keep this list tight.
    const FIELDS: &'static str =
        "summary,status,assignee,reporter,project,updated,created,labels";

    /// Run a JQL search with pagination, optionally expanding changelog.
    async fn search(&self, jql: &str, expand_changelog: bool) -> Result<Vec<Value>, String> {
        let mut start_at = 0u64;
        let page = 50u64;
        let mut all: Vec<Value> = Vec::new();

        loop {
            let mut url = format!(
                "{}/rest/api/3/search?jql={}&fields={}&startAt={}&maxResults={}",
                self.base_url,
                urlencoding(jql),
                Self::FIELDS,
                start_at,
                page
            );
            if expand_changelog {
                url.push_str("&expand=changelog");
            }

            let resp = self
                .client
                .get(&url)
                .header("Authorization", self.auth_header())
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| format!("jira request failed: {e}"))?;

            if !resp.status().is_success() {
                let code = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("jira {code}: {}", truncate(&body, 300)));
            }

            let body: Value = resp
                .json()
                .await
                .map_err(|e| format!("jira: bad json: {e}"))?;
            let issues = body
                .get("issues")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let got = issues.len() as u64;
            all.extend(issues);

            let total = body.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
            start_at += got;
            if got == 0 || start_at >= total {
                break;
            }
        }
        Ok(all)
    }

    fn normalize_issue(&self, raw: &Value) -> Option<Issue> {
        let key = raw.get("key")?.as_str()?.to_string();
        let id = raw.get("id")?.as_str().unwrap_or("").to_string();
        let f = raw.get("fields")?;
        let status = f.get("status");
        Some(Issue {
            source: "jira".into(),
            source_issue_id: id,
            issue_key: key.clone(),
            summary: f.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            status_name: status
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status_category: status
                .and_then(|s| s.get("statusCategory"))
                .and_then(|c| c.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("indeterminate")
                .to_string(),
            assignee_display: f
                .get("assignee")
                .and_then(|a| a.get("displayName"))
                .and_then(|v| v.as_str())
                .map(String::from),
            reporter_display: f
                .get("reporter")
                .and_then(|a| a.get("displayName"))
                .and_then(|v| v.as_str())
                .map(String::from),
            project_key: f
                .get("project")
                .and_then(|p| p.get("key"))
                .and_then(|v| v.as_str())
                .map(String::from),
            project_name: f
                .get("project")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from),
            updated_at: normalize_ts(f.get("updated").and_then(|v| v.as_str())),
            created_at: normalize_ts(f.get("created").and_then(|v| v.as_str())),
            browse_url: format!("{}/browse/{}", self.base_url, key),
            labels: f
                .get("labels")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        })
    }

    /// Derive activity events from an issue's changelog histories.
    fn activity_from(&self, raw: &Value) -> Vec<ActivityEvent> {
        let key = raw.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut events = Vec::new();
        let histories = raw
            .get("changelog")
            .and_then(|c| c.get("histories"))
            .and_then(|h| h.as_array());
        if let Some(histories) = histories {
            for h in histories {
                let at = normalize_ts(h.get("created").and_then(|v| v.as_str()));
                let actor = h
                    .get("author")
                    .and_then(|a| a.get("displayName"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                if let Some(items) = h.get("items").and_then(|v| v.as_array()) {
                    for it in items {
                        let field = it.get("field").and_then(|v| v.as_str()).unwrap_or("");
                        let old_v =
                            it.get("fromString").and_then(|v| v.as_str()).map(String::from);
                        let new_v =
                            it.get("toString").and_then(|v| v.as_str()).map(String::from);
                        let activity_type = match field {
                            "status" => "status_change",
                            "assignee" => "assigned",
                            "resolution" => "resolved",
                            _ => "field_change",
                        };
                        let text_summary = match field {
                            "status" => Some(format!(
                                "{} → {}",
                                old_v.clone().unwrap_or_default(),
                                new_v.clone().unwrap_or_default()
                            )),
                            _ => new_v.clone(),
                        };
                        events.push(ActivityEvent {
                            issue_key: key.clone(),
                            activity_type: activity_type.into(),
                            activity_at: at.clone(),
                            actor_display: actor.clone(),
                            old_value: old_v,
                            new_value: new_v,
                            text_summary,
                        });
                    }
                }
            }
        }
        events
    }
}

impl WorkSourceConnector for JiraConnector {
    async fn fetch_in_progress(&self) -> Result<Vec<Issue>, String> {
        let raw = self.search(&self.jql_in_progress, false).await?;
        Ok(raw.iter().filter_map(|r| self.normalize_issue(r)).collect())
    }

    async fn fetch_recent(&self) -> Result<(Vec<Issue>, Vec<ActivityEvent>), String> {
        let raw = self.search(&self.jql_recent, true).await?;
        let issues: Vec<Issue> = raw.iter().filter_map(|r| self.normalize_issue(r)).collect();
        let activity: Vec<ActivityEvent> = raw.iter().flat_map(|r| self.activity_from(r)).collect();
        Ok((issues, activity))
    }
}

// ------------------------------- helpers -----------------------------------

/// Minimal percent-encoding for JQL in a query string (spaces, common chars).
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Jira returns e.g. "2026-08-03T09:12:00.000-0400" (variable fractional
/// seconds, no-colon offset). Normalize to **UTC** RFC3339 so downstream string
/// comparisons against our UTC range bounds are correct regardless of the
/// issue's original timezone. Falls back to the raw string only if unparseable.
fn normalize_ts(s: Option<&str>) -> String {
    let s = match s {
        Some(s) if !s.is_empty() => s,
        _ => return String::new(),
    };
    // Try a few shapes: with/without fractional seconds, colon/no-colon offset.
    // chrono's %z accepts both +0000 and +00:00; %.f matches zero-or-more digits.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%dT%H:%M:%S%z",
    ] {
        if let Ok(dt) = chrono::DateTime::parse_from_str(s, fmt) {
            return dt.with_timezone(&chrono::Utc).to_rfc3339();
        }
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&chrono::Utc).to_rfc3339();
    }
    s.to_string()
}

/// Truncate to at most `n` bytes without splitting a UTF-8 char.
fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let mut end = n;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}
