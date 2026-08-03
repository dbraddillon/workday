//! Deterministic fake data source. Powers `fake_data_mode` so the whole app —
//! all three tabs and standup generation — is usable before any Jira creds are
//! entered, and so development never depends on the network.

use crate::connector::WorkSourceConnector;
use crate::model::{ActivityEvent, Issue};
use chrono::{Duration, Utc};

fn iso(offset_hours: i64) -> String {
    (Utc::now() - Duration::hours(offset_hours)).to_rfc3339()
}

fn issue(
    key: &str,
    summary: &str,
    status: &str,
    cat: &str,
    project: &str,
    updated_h: i64,
) -> Issue {
    Issue {
        source: "jira".into(),
        source_issue_id: format!("100{}", key.chars().last().unwrap_or('0')),
        issue_key: key.into(),
        summary: summary.into(),
        status_name: status.into(),
        status_category: cat.into(),
        assignee_display: Some("Donald".into()),
        reporter_display: Some("Donald".into()),
        project_key: Some(project.into()),
        project_name: Some(format!("{project} Project")),
        updated_at: iso(updated_h),
        created_at: iso(updated_h + 72),
        browse_url: format!("https://example.atlassian.net/browse/{key}"),
        labels: vec![],
    }
}

pub struct FakeConnector;

impl WorkSourceConnector for FakeConnector {
    async fn fetch_in_progress(&self) -> Result<Vec<Issue>, String> {
        Ok(vec![
            issue("PLAT-142", "Wire up menu bar popover positioning", "In Progress", "indeterminate", "PLAT", 2),
            issue("PLAT-139", "Jira connector: pagination + normalization", "In Review", "indeterminate", "PLAT", 5),
            issue("APP-88", "Standup formatter: grouped-by-status default", "In Progress", "indeterminate", "APP", 20),
            issue("APP-91", "Keychain storage for API token", "To Do", "new", "APP", 30),
        ])
    }

    async fn fetch_recent(&self) -> Result<(Vec<Issue>, Vec<ActivityEvent>), String> {
        let issues = vec![
            issue("PLAT-142", "Wire up menu bar popover positioning", "In Progress", "indeterminate", "PLAT", 2),
            issue("PLAT-139", "Jira connector: pagination + normalization", "In Review", "indeterminate", "PLAT", 5),
            issue("PLAT-130", "SQLite schema + migrations", "Done", "done", "PLAT", 18),
            issue("APP-88", "Standup formatter: grouped-by-status default", "In Progress", "indeterminate", "APP", 20),
        ];
        let activity = vec![
            ActivityEvent {
                issue_key: "PLAT-130".into(),
                activity_type: "status_change".into(),
                activity_at: iso(18),
                actor_display: Some("Donald".into()),
                old_value: Some("In Review".into()),
                new_value: Some("Done".into()),
                text_summary: Some("In Review → Done".into()),
            },
            ActivityEvent {
                issue_key: "PLAT-142".into(),
                activity_type: "status_change".into(),
                activity_at: iso(2),
                actor_display: Some("Donald".into()),
                old_value: Some("To Do".into()),
                new_value: Some("In Progress".into()),
                text_summary: Some("To Do → In Progress".into()),
            },
            ActivityEvent {
                issue_key: "PLAT-139".into(),
                activity_type: "status_change".into(),
                activity_at: iso(5),
                actor_display: Some("Donald".into()),
                old_value: Some("In Progress".into()),
                new_value: Some("In Review".into()),
                text_summary: Some("In Progress → In Review".into()),
            },
        ];
        Ok((issues, activity))
    }
}
