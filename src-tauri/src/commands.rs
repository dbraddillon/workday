//! Tauri command surface — the IPC boundary the React UI calls via `invoke`.
//! Thin glue: validate, call the right layer, return serializable results.

use crate::config::{self, AppSettings};
use crate::db::{repo, settings_repo, Db};
use crate::delivery::{DeliveryMethod, DeliveryResult, SlackDeliveryService, V1DeliveryService};
use crate::model::{Issue, StandupDraft, StandupModel, SyncStatus, TimeRange};
use crate::standup::summarizer::{
    ClaudeCliSummarizer, PassthroughSummarizer, Summarizer,
};
use crate::standup::{compose, formatter};
use crate::{sync, AppState};
use chrono::{Duration, Utc};
use tauri::{Manager, State};

// ------------------------------- settings ----------------------------------

#[tauri::command]
pub fn get_settings(db: State<Db>) -> Result<AppSettings, String> {
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    Ok(settings_repo::load(&conn))
}

#[tauri::command]
pub fn save_settings(db: State<Db>, settings: AppSettings) -> Result<AppSettings, String> {
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    settings_repo::save(&conn, &settings).map_err(|e| e.to_string())?;
    Ok(settings_repo::load(&conn))
}

/// Store/replace the Jira API token in the Keychain. Empty string clears it.
#[tauri::command]
pub fn set_jira_token(token: String) -> Result<bool, String> {
    if token.trim().is_empty() {
        config::delete_jira_token()?;
        Ok(false)
    } else {
        config::set_jira_token(token.trim())?;
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
pub async fn refresh_now(db: State<'_, Db>) -> Result<SyncStatus, String> {
    let settings = {
        let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
        settings_repo::load(&conn)
    };
    let _ = sync::run_sync(&db, &settings).await; // errors are recorded, non-fatal
    let conn = db.0.lock().map_err(|_| "db lock poisoned")?;
    repo::sync_status(&conn).map_err(|e| e.to_string())
}

// ------------------------------- standup ------------------------------------

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
    let range = TimeRange {
        start: start.to_rfc3339(),
        end: end.to_rfc3339(),
        label,
    };
    Ok(compose(range, &issues, &activity))
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

    // 4. polish (optional; falls back to base text on any error)
    let text = if ai_polish {
        let hint = style_hint.clone().unwrap_or_default();
        match ClaudeCliSummarizer.polish(&base_text, &hint).await {
            Ok(t) => t,
            Err(_) => PassthroughSummarizer
                .polish(&base_text, &hint)
                .await
                .unwrap_or(base_text.clone()),
        }
    } else {
        base_text.clone()
    };

    let draft = StandupDraft {
        formatter_key: formatter_key.clone(),
        time_range: model.time_range.clone(),
        text: text.clone(),
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

// -------------------------------- window ------------------------------------

/// Hide the popover (used by the "close" affordance / Escape).
#[tauri::command]
pub fn hide_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
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
        "24h" => now - Duration::hours(24),
        "3d" => now - Duration::days(3),
        "7d" => now - Duration::days(7),
        _ => now - Duration::hours(24),
    }
}

fn resolve_range(range: &str) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let now = Utc::now();
    let (start, label) = match range {
        "today" => (range_to_start("today"), "Today"),
        "24h" => (now - Duration::hours(24), "Last 24 hours"),
        "3d" => (now - Duration::days(3), "Last 3 days"),
        "7d" => (now - Duration::days(7), "Last 7 days"),
        _ => (now - Duration::hours(24), "Last 24 hours"),
    };
    (start, now, label.to_string())
}
