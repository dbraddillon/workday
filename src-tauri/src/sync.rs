//! Sync coordinator. Chooses the connector (fake vs Jira) from settings, fetches
//! in-progress + recent, normalizes, and writes to the cache — recording a
//! sync_run either way so errors are visible but non-fatal (cached data still
//! displays). Polling in v1; the same `run_sync` is called by the background
//! timer and the manual refresh button.

use crate::config::AppSettings;
use crate::connector::{
    fake::FakeConnector, github::GithubConnector, jira::JiraConnector, WorkSourceConnector,
};
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

    // The GitHub review queue is a separate source with its own failure mode, so
    // it runs outside the Jira result: a broken `gh` must not mark the Jira sync
    // failed, and a Jira outage must not blank the Reviews tab. Errors here are
    // dropped on purpose — the tab reports its own staleness from the cache.
    //
    // The two queries are independent (open queue vs submitted reviews) and each
    // spawns a `gh` process, so they run concurrently: sequentially they roughly
    // doubled the pass duration, and a long pass is what surfaced the in-flight
    // status bug. Neither holds the db lock across its await.
    let _ = tokio::join!(
        sync_review_queue(db, settings),
        sync_submitted_reviews(db, settings)
    );

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

/// How far back to look for reviews the user submitted. Independent of the
/// queue's age window: that one bounds what's worth reviewing, this one has to
/// cover any standup range the user might pick, including "last week".
const REVIEW_HISTORY_DAYS: i64 = 14;

/// Build the connector from settings. Nothing here reaches the Keychain - the
/// `gh` CLI owns the GitHub credential.
fn github_connector(settings: &AppSettings) -> GithubConnector {
    GithubConnector {
        org: settings.github_org.clone(),
        login: settings.github_login.clone(),
        teams: settings.github_team_list(),
        window_days: settings.github_window_days as i64,
        max_results: settings.github_max_results as usize,
        include_team_authored: settings.github_include_team_authored,
    }
}

/// Fetch the GitHub review queue and replace the cache. Returns the pre-cap
/// total. A no-op unless enabled and configured.
pub async fn sync_review_queue(db: &Db, settings: &AppSettings) -> Result<usize, String> {
    if !settings.github_enabled {
        return Ok(0);
    }
    let (prs, total) = github_connector(settings).fetch_review_queue().await?;
    let now = Utc::now().to_rfc3339();
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::replace_pull_requests(&conn, &prs, &now).map_err(|e| e.to_string())?;
    Ok(total)
}

/// Fetch reviews the user submitted and merge them into the cache. Separate call
/// from the queue: the queue is open PRs, and most reviewed PRs merge within a
/// day, so a completed review is almost never findable there.
pub async fn sync_submitted_reviews(db: &Db, settings: &AppSettings) -> Result<usize, String> {
    if !settings.github_enabled {
        return Ok(0);
    }
    let reviews = github_connector(settings)
        .fetch_submitted_reviews(REVIEW_HISTORY_DAYS)
        .await?;
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::upsert_submitted_reviews(&conn, &reviews).map_err(|e| e.to_string())?;
    Ok(reviews.len())
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
