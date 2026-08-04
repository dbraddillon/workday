//! Settings persistence. Non-secret `AppSettings` stored as key/value rows so
//! adding a field never needs a schema migration.

use crate::config::AppSettings;
use rusqlite::{params, Connection};

pub fn load(conn: &Connection) -> AppSettings {
    let mut s = AppSettings::default();
    let mut stmt = match conn.prepare("SELECT key, value FROM app_settings") {
        Ok(s) => s,
        Err(_) => return s,
    };
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (k, v) = row;
            match k.as_str() {
                "jira_base_url" => s.jira_base_url = v,
                "jira_email" => s.jira_email = v,
                "jira_jql_in_progress" => s.jira_jql_in_progress = v,
                "jira_jql_recent" => s.jira_jql_recent = v,
                "refresh_interval_secs" => s.refresh_interval_secs = v.parse().unwrap_or(300),
                "default_recent_range" => s.default_recent_range = v,
                "default_formatter" => s.default_formatter = v,
                "ai_polish_enabled" => s.ai_polish_enabled = v == "true",
                "fake_data_mode" => s.fake_data_mode = v == "true",
                "thread_doing" => s.thread_doing = v,
                "thread_pairing" => s.thread_pairing = v,
                "thread_blocker" => s.thread_blocker = v,
                "thread_post_scrum" => s.thread_post_scrum = v,
                "thread_prompt_doing" => s.thread_prompt_doing = v,
                "thread_prompt_working" => s.thread_prompt_working = v,
                "thread_prompt_pairing" => s.thread_prompt_pairing = v,
                "thread_prompt_blocker" => s.thread_prompt_blocker = v,
                "thread_prompt_post_scrum" => s.thread_prompt_post_scrum = v,
                _ => {}
            }
        }
    }
    // `has_jira_token` is filled by the caller from the cached AppState token —
    // load() must not touch the Keychain (it runs on every settings load and
    // every sync pass, which would re-prompt).
    s
}

pub fn save(conn: &Connection, s: &AppSettings) -> rusqlite::Result<()> {
    let pairs: [(&str, String); 18] = [
        ("jira_base_url", s.jira_base_url.clone()),
        ("jira_email", s.jira_email.clone()),
        ("jira_jql_in_progress", s.jira_jql_in_progress.clone()),
        ("jira_jql_recent", s.jira_jql_recent.clone()),
        ("refresh_interval_secs", s.refresh_interval_secs.to_string()),
        ("default_recent_range", s.default_recent_range.clone()),
        ("default_formatter", s.default_formatter.clone()),
        ("ai_polish_enabled", s.ai_polish_enabled.to_string()),
        ("fake_data_mode", s.fake_data_mode.to_string()),
        ("thread_doing", s.thread_doing.clone()),
        ("thread_pairing", s.thread_pairing.clone()),
        ("thread_blocker", s.thread_blocker.clone()),
        ("thread_post_scrum", s.thread_post_scrum.clone()),
        ("thread_prompt_doing", s.thread_prompt_doing.clone()),
        ("thread_prompt_working", s.thread_prompt_working.clone()),
        ("thread_prompt_pairing", s.thread_prompt_pairing.clone()),
        ("thread_prompt_blocker", s.thread_prompt_blocker.clone()),
        ("thread_prompt_post_scrum", s.thread_prompt_post_scrum.clone()),
    ];
    for (k, v) in pairs {
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![k, v],
        )?;
    }
    Ok(())
}
