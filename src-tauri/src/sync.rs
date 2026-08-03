//! Sync coordinator. Chooses the connector (fake vs Jira) from settings, fetches
//! in-progress + recent, normalizes, and writes to the cache — recording a
//! sync_run either way so errors are visible but non-fatal (cached data still
//! displays). Polling in v1; the same `run_sync` is called by the background
//! timer and the manual refresh button.

use crate::config::AppSettings;
use crate::connector::{fake::FakeConnector, jira::JiraConnector, WorkSourceConnector};
use crate::db::{repo, Db};
use chrono::Utc;

/// Run one sync pass. Returns the number of issues cached, or an error string.
/// Always records a sync_run row. The Jira token is passed in (from the cached
/// AppState) so this never touches the Keychain — see AppState::jira_token.
pub async fn run_sync(
    db: &Db,
    settings: &AppSettings,
    jira_token: Option<String>,
) -> Result<i64, String> {
    let started = Utc::now().to_rfc3339();
    let run_id = {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        repo::start_sync_run(&conn, &started).map_err(|e| e.to_string())?
    };

    let result = fetch_and_store(db, settings, jira_token).await;

    let finished = Utc::now().to_rfc3339();
    {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        match &result {
            Ok(count) => {
                repo::finish_sync_run(&conn, run_id, &finished, true, *count, None)
                    .map_err(|e| e.to_string())?;
            }
            Err(msg) => {
                repo::finish_sync_run(&conn, run_id, &finished, false, 0, Some(msg))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    result
}

async fn fetch_and_store(
    db: &Db,
    settings: &AppSettings,
    jira_token: Option<String>,
) -> Result<i64, String> {
    // Gather (connector chosen from settings).
    let (in_prog, recent, activity) = if settings.fake_data_mode {
        let c = FakeConnector;
        let ip = c.fetch_in_progress().await?;
        let (rec, act) = c.fetch_recent().await?;
        (ip, rec, act)
    } else {
        let token = jira_token
            .ok_or("No Jira API token in Keychain. Add one in Settings.")?;
        if settings.jira_base_url.is_empty() || settings.jira_email.is_empty() {
            return Err("Jira base URL and email are required. See Settings.".into());
        }
        let c = JiraConnector::new(
            settings.jira_base_url.clone(),
            settings.jira_email.clone(),
            token,
            settings.jira_jql_in_progress.clone(),
            settings.jira_jql_recent.clone(),
        );
        let ip = c.fetch_in_progress().await?;
        let (rec, act) = c.fetch_recent().await?;
        (ip, rec, act)
    };

    // Store. Merge in-progress + recent into the issues cache (dedup by key
    // happens via upsert).
    let now = Utc::now().to_rfc3339();
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::upsert_issues(&conn, &in_prog, &now).map_err(|e| e.to_string())?;
    repo::upsert_issues(&conn, &recent, &now).map_err(|e| e.to_string())?;
    repo::upsert_activity(&conn, &activity).map_err(|e| e.to_string())?;

    // Report a representative count.
    let count = repo::in_progress(&conn).map(|v| v.len() as i64).unwrap_or(0);
    Ok(count)
}
