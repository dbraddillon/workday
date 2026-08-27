//! Tauri command surface — the IPC boundary the React UI calls via `invoke`.
//! Thin glue: validate, call the right layer, return serializable results.

use crate::config::{self, AppSettings};
use crate::db::{repo, settings_repo, Db};
use crate::delivery::{DeliveryMethod, DeliveryResult, SlackDeliveryService, V1DeliveryService};
use crate::model::{Issue, StandupDraft, StandupModel, StandupNarrative, SyncStatus, TimeRange};
use crate::popover;
use crate::standup::summarizer::{ClaudeCliSummarizer, Summarizer};
use crate::standup::{compose, formatter};
use crate::{sync, AppState};
use chrono::{Duration, Utc};
use tauri::{Manager, State};

// ------------------------------- settings ----------------------------------

#[tauri::command]
pub fn get_settings(db: State<Db>, state: State<AppState>) -> Result<AppSettings, String> {
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    let mut s = settings_repo::load(&conn);
    s.has_jira_token = state.has_jira_token(); // from cache, not the Keychain
    Ok(s)
}

#[tauri::command]
pub fn save_settings(
    db: State<Db>,
    state: State<AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    // If the data source changed (esp. fake-data mode toggled off), wipe the
    // cache so stale sample issues don't mix with real Jira data — both share
    // source='jira' and would otherwise never age out.
    let prev = settings_repo::load(&conn);
    if prev.fake_data_mode != settings.fake_data_mode {
        repo::clear_issue_cache(&conn).map_err(|e| e.to_string())?;
    }
    settings_repo::save(&conn, &settings).map_err(|e| e.to_string())?;
    let mut s = settings_repo::load(&conn);
    s.has_jira_token = state.has_jira_token();
    Ok(s)
}

/// Store/replace the Jira API token in the Keychain. Empty string clears it.
/// Updates the in-memory cache so nothing has to re-read the Keychain.
#[tauri::command]
pub fn set_jira_token(state: State<AppState>, token: String) -> Result<bool, String> {
    if token.trim().is_empty() {
        config::delete_jira_token()?;
        state.set_jira_token(None);
        Ok(false)
    } else {
        let t = token.trim().to_string();
        config::set_jira_token(&t)?;
        state.set_jira_token(Some(t));
        Ok(true)
    }
}

// -------------------------------- glance ------------------------------------

#[tauri::command]
pub fn get_in_progress(db: State<Db>) -> Result<Vec<Issue>, String> {
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::in_progress(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recent(db: State<Db>, range: String) -> Result<Vec<Issue>, String> {
    let since = range_to_start(&range);
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::recent(&conn, &since.to_rfc3339()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sync_status(db: State<Db>) -> Result<SyncStatus, String> {
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::sync_status(&conn).map_err(|e| e.to_string())
}

/// Manual refresh — runs a sync pass now.
#[tauri::command]
pub async fn refresh_now(
    db: State<'_, Db>,
    state: State<'_, AppState>,
) -> Result<SyncStatus, String> {
    let token = state.jira_token();
    let settings = {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        settings_repo::load(&conn)
    };
    let _ = sync::run_sync(&db, &settings, token).await; // errors are recorded, non-fatal
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::sync_status(&conn).map_err(|e| e.to_string())
}

// ------------------------------ reviews (GitHub) ----------------------------

/// The review queue, plus whether the source is usable and how many rows the cap
/// hid. `total` is the pre-cap count so the UI can say "showing N of M" rather
/// than truncating silently.
#[derive(serde::Serialize)]
pub struct ReviewQueue {
    pub prs: Vec<crate::model::PullRequest>,
    pub total: usize,
    /// Reviews the user submitted recently, newest first. Not a subset of `prs`:
    /// a reviewed PR usually merges within a day and so leaves the open queue.
    pub done: Vec<crate::model::SubmittedReview>,
    /// False when `gh` is missing or unauthenticated — the UI explains rather
    /// than showing an unexplained empty list.
    pub gh_available: bool,
    pub enabled: bool,
}

/// How far back the Reviews tab shows submitted reviews. Shorter than the synced
/// history (`sync::REVIEW_HISTORY_DAYS`), which has to cover a standup range
/// picked after the fact.
const DONE_DISPLAY_DAYS: i64 = 7;

fn done_since() -> String {
    (Utc::now() - chrono::Duration::days(DONE_DISPLAY_DAYS)).to_rfc3339()
}

/// Whether a `gh` CLI is present AND authenticated, so the Reviews tab can work.
/// Mirrors `ai_polish_available`: gate the feature rather than fail silently.
#[tauri::command]
pub async fn gh_available() -> bool {
    crate::connector::github::gh_available().await
}

/// Read the cached review queue. Does not hit the network — the poll loop
/// refreshes it (see `sync::sync_review_queue`).
#[tauri::command]
pub async fn get_review_queue(
    db: State<'_, Db>,
) -> Result<ReviewQueue, String> {
    let (settings, prs, done) = {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        let s = settings_repo::load(&conn);
        let prs = repo::pull_requests(&conn).map_err(|e| e.to_string())?;
        let done = repo::submitted_reviews_since(&conn, &done_since()).map_err(|e| e.to_string())?;
        (s, prs, done)
    };
    // Only preflight `gh` when the feature is on; the check spawns a process and
    // there is no reason to pay for it on every read otherwise.
    let gh_ok = if settings.github_enabled {
        crate::connector::github::gh_available().await
    } else {
        false
    };
    let total = prs.len();
    Ok(ReviewQueue {
        prs,
        total,
        done,
        gh_available: gh_ok,
        enabled: settings.github_enabled,
    })
}

/// Force a review-queue refresh now (the Reviews tab's own refresh).
#[tauri::command]
pub async fn refresh_reviews(db: State<'_, Db>) -> Result<ReviewQueue, String> {
    let settings = {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        settings_repo::load(&conn)
    };
    let total = sync::sync_review_queue(&db, &settings).await?;
    // Submitted reviews are a second query; a failure there must not fail the
    // refresh, since the queue itself already came back.
    let _ = sync::sync_submitted_reviews(&db, &settings).await;
    let (prs, done) = {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        (
            repo::pull_requests(&conn).map_err(|e| e.to_string())?,
            repo::submitted_reviews_since(&conn, &done_since()).map_err(|e| e.to_string())?,
        )
    };
    Ok(ReviewQueue {
        prs,
        total,
        done,
        gh_available: true, // a successful fetch proves it
        enabled: settings.github_enabled,
    })
}

/// Tick a PR off as reviewed (or untick it). Persisted separately from the PR
/// cache so it survives the PR leaving the window.
#[tauri::command]
pub fn set_pr_reviewed(
    db: State<Db>,
    repo_name: String,
    number: i64,
    reviewed: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::set_pr_checkoff(&conn, &repo_name, number, reviewed, &now).map_err(|e| e.to_string())
}

// ------------------------------- standup ------------------------------------

/// Whether AI polish is available on this machine (a `claude` CLI on PATH).
/// The UI uses this to show/enable the AI-polish toggle; the app works fully
/// without it.
#[tauri::command]
pub async fn ai_polish_available() -> bool {
    crate::standup::summarizer::claude_cli_available().await
}

/// Build the normalized standup model for a time window (no rendering yet, so
/// the UI can show include/exclude toggles first).
#[tauri::command]
pub fn build_standup_model(db: State<Db>, range: String) -> Result<StandupModel, String> {
    let (start, end, label) = resolve_range(&range);
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    let issues = repo::issues_in_range(&conn, &start.to_rfc3339(), &end.to_rfc3339())
        .map_err(|e| e.to_string())?;
    let activity = repo::activity_in_range(&conn, &start.to_rfc3339(), &end.to_rfc3339())
        .map_err(|e| e.to_string())?;
    // Seed the freeform narrative from the user's saved standup defaults; the UI
    // can edit these before rendering.
    let settings = settings_repo::load(&conn);
    let narrative = StandupNarrative {
        doing: settings.thread_doing,
        pairing: settings.thread_pairing,
        post_scrum: settings.thread_post_scrum,
        blocker: settings.thread_blocker,
        prompt_doing: settings.thread_prompt_doing,
        prompt_working: settings.thread_prompt_working,
        prompt_pairing: settings.thread_prompt_pairing,
        prompt_blocker: settings.thread_prompt_blocker,
        prompt_post_scrum: settings.thread_prompt_post_scrum,
    };
    // PRs ticked off in the same window feed the optional reviews line.
    // Reviews GitHub recorded, unioned with manual checkoffs: a review on a PR
    // that never entered the queue still counts, and one that's both counts once.
    let reviewed =
        repo::review_credit_count_in_range(&conn, &start.to_rfc3339(), &end.to_rfc3339())
            .unwrap_or(0);
    let range = TimeRange {
        start: start.to_rfc3339(),
        end: end.to_rfc3339(),
        label,
    };
    Ok(compose(range, &issues, &activity, narrative, reviewed))
}

/// Render a (possibly user-edited) model to a draft with the given formatter,
/// optionally AI-polishing it, and always writing the on-demand context file.
/// Persists the generated post.
#[tauri::command]
pub async fn generate_standup(
    db: State<'_, Db>,
    app: tauri::AppHandle,
    model: StandupModel,
    formatter_key: String,
    ai_polish: bool,
    style_hint: Option<String>,
) -> Result<StandupDraft, String> {
    // 3. render
    let base_text = formatter::render_with(&formatter_key, &model);

    // 4. polish (optional; falls back to base text on any error, but records
    //    *that* it fell back so the UI can surface it instead of it being silent)
    let mut fell_back: Option<String> = None;
    let text = if ai_polish {
        let hint = style_hint.clone().unwrap_or_default();
        match ClaudeCliSummarizer.polish(&base_text, &hint).await {
            Ok(t) => t,
            Err(e) => {
                fell_back = Some(e);
                base_text.clone()
            }
        }
    } else {
        base_text.clone()
    };

    let draft = StandupDraft {
        formatter_key: formatter_key.clone(),
        time_range: model.time_range.clone(),
        text: text.clone(),
        ai_polish_fell_back: fell_back,
    };

    // Write the on-demand context file next to app data.
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = crate::standup::summarizer::write_context_file(&dir, &model, &draft);
    }

    // Persist the generated post.
    {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        let _ = repo::save_generated_post(
            &conn,
            &Utc::now().to_rfc3339(),
            &draft.time_range.start,
            &draft.time_range.end,
            &formatter_key,
            &text,
        );
    }

    Ok(draft)
}

// ------------------------------- delivery -----------------------------------

/// Record a delivery outcome. The actual clipboard write happens in the webview
/// (clipboard plugin); this just logs it against the seam.
#[tauri::command]
pub fn record_delivery(method: String) -> Result<DeliveryResult, String> {
    let method = match method.as_str() {
        "webhook" => DeliveryMethod::Webhook,
        _ => DeliveryMethod::Clipboard,
    };
    Ok(V1DeliveryService.deliver("", method))
}

// ------------------------------ autostart ----------------------------------

/// Whether the app is set to launch at login.
#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Enable/disable launch at login.
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable().map_err(|e| e.to_string())?;
    } else {
        mgr.disable().map_err(|e| e.to_string())?;
    }
    mgr.is_enabled().map_err(|e| e.to_string())
}

// -------------------------------- window ------------------------------------

/// Hide the popover (used by the "close" affordance / Escape).
///
/// Goes through `popover` rather than `window.hide()` so the panel's own
/// ordering is used, and so an explicit dismissal clears the pin — same rule as
/// the tray toggle.
#[tauri::command]
pub fn hide_window(app: tauri::AppHandle) {
    popover::set_pinned(false);
    popover::hide(&app);
}

/// Whether the popover is pinned open (survives losing focus).
#[tauri::command]
pub fn get_pinned() -> bool {
    popover::is_pinned()
}

/// Pin/unpin the popover. Session-only state — intentionally not persisted to
/// `AppSettings`; see `popover.rs`.
#[tauri::command]
pub fn set_pinned(pinned: bool) -> bool {
    popover::set_pinned(pinned);
    pinned
}

/// Persist the last-known dark/light or other trivial UI prefs is out of scope
/// for v1; provided as a placeholder no-op the UI can call safely.
#[tauri::command]
pub fn ui_ready(state: State<AppState>) {
    state.mark_ready();
}

// ------------------------------- helpers ------------------------------------

fn range_to_start(range: &str) -> chrono::DateTime<Utc> {
    let now = Utc::now();
    match range {
        "today" => now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|nd| chrono::DateTime::<Utc>::from_naive_utc_and_offset(nd, Utc))
            .unwrap_or(now - Duration::hours(24)),
        "standup" => standup_start(),
        "24h" => now - Duration::hours(24),
        "3d" => now - Duration::days(3),
        "7d" => now - Duration::days(7),
        _ => now - Duration::hours(24),
    }
}

/// Day-aware "since last working day" start, resolved in **local** time then
/// converted to UTC (the weekend boundary is a local-time concept).
///
/// Standup happens every weekday; the window should cover work since the start
/// of the previous working day:
///   - Mon → back to Fri 00:00 local (Fri + the weekend)
///   - Sun → back to Fri 00:00 local (weekend catch-up)
///   - Tue–Sat → back to the previous calendar day 00:00 local ("since yesterday")
/// Pure day-math for the standup window: how many days back the window starts,
/// given today's weekday. Mon/Sun → Friday, else yesterday. Split out so it's
/// unit-testable without mocking the clock.
fn standup_days_back(today: chrono::Weekday) -> i64 {
    use chrono::Weekday;
    match today {
        Weekday::Mon => 3, // Fri
        Weekday::Sun => 2, // Fri
        _ => 1,            // yesterday
    }
}

fn standup_start() -> chrono::DateTime<Utc> {
    use chrono::{Datelike, Local};
    let now_local = Local::now();
    let start_date = now_local.date_naive() - Duration::days(standup_days_back(now_local.weekday()));
    start_date
        .and_hms_opt(0, 0, 0)
        .and_then(|nd| nd.and_local_timezone(Local).single())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| now_local.with_timezone(&Utc) - Duration::hours(24))
}

fn resolve_range(range: &str) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let now = Utc::now();
    let (start, label): (chrono::DateTime<Utc>, String) = match range {
        "today" => (range_to_start("today"), "Today".into()),
        "standup" => (standup_start(), standup_label()),
        "24h" => (now - Duration::hours(24), "Last 24 hours".into()),
        "3d" => (now - Duration::days(3), "Last 3 days".into()),
        "7d" => (now - Duration::days(7), "Last 7 days".into()),
        _ => (now - Duration::hours(24), "Last 24 hours".into()),
    };
    (start, now, label)
}

/// Human label for the day-aware standup window (mirrors `standup_start`).
fn standup_label() -> String {
    use chrono::{Datelike, Local, Weekday};
    match Local::now().weekday() {
        Weekday::Mon | Weekday::Sun => "Since Friday".into(),
        _ => "Since yesterday".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::standup_days_back;
    use chrono::Weekday;

    #[test]
    fn monday_reaches_back_to_friday() {
        assert_eq!(standup_days_back(Weekday::Mon), 3);
    }

    #[test]
    fn sunday_reaches_back_to_friday() {
        assert_eq!(standup_days_back(Weekday::Sun), 2);
    }

    #[test]
    fn weekdays_reach_back_one_day() {
        for wd in [Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat] {
            assert_eq!(standup_days_back(wd), 1, "{wd:?} should look back 1 day");
        }
    }
}
