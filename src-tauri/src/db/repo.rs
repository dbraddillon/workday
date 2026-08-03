//! Repository functions over the SQLite connection.
//!
//! `IssueRepository` / `ActivityRepository` from the doc, expressed as plain
//! functions taking `&Connection`. Callers hold the `Db` mutex and pass the
//! guard in. Kept intentionally boring.

use crate::model::{ActivityEvent, Issue, SyncStatus};
use rusqlite::{params, Connection};

// ------------------------------- issues ------------------------------------

/// Upsert a batch of issues. `now` is an RFC3339 timestamp for first/last seen.
pub fn upsert_issues(conn: &Connection, issues: &[Issue], now: &str) -> rusqlite::Result<()> {
    let tx_sql = "
        INSERT INTO issues (
            source, source_issue_id, issue_key, summary, status_name, status_category,
            assignee_display, reporter_display, project_key, project_name,
            updated_at, created_at, browse_url, labels_json,
            first_seen_at, last_seen_at, raw_json
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15,NULL)
        ON CONFLICT(source, issue_key) DO UPDATE SET
            summary=excluded.summary,
            status_name=excluded.status_name,
            status_category=excluded.status_category,
            assignee_display=excluded.assignee_display,
            reporter_display=excluded.reporter_display,
            project_key=excluded.project_key,
            project_name=excluded.project_name,
            updated_at=excluded.updated_at,
            browse_url=excluded.browse_url,
            labels_json=excluded.labels_json,
            last_seen_at=excluded.last_seen_at
    ";
    let mut stmt = conn.prepare(tx_sql)?;
    for i in issues {
        let labels_json = serde_json::to_string(&i.labels).unwrap_or_else(|_| "[]".into());
        stmt.execute(params![
            i.source,
            i.source_issue_id,
            i.issue_key,
            i.summary,
            i.status_name,
            i.status_category,
            i.assignee_display,
            i.reporter_display,
            i.project_key,
            i.project_name,
            i.updated_at,
            i.created_at,
            i.browse_url,
            labels_json,
            now,
        ])?;
    }
    Ok(())
}

fn row_to_issue(row: &rusqlite::Row) -> rusqlite::Result<Issue> {
    let labels_json: String = row.get("labels_json")?;
    let labels: Vec<String> = serde_json::from_str(&labels_json).unwrap_or_default();
    Ok(Issue {
        source: row.get("source")?,
        source_issue_id: row.get("source_issue_id")?,
        issue_key: row.get("issue_key")?,
        summary: row.get("summary")?,
        status_name: row.get("status_name")?,
        status_category: row.get("status_category")?,
        assignee_display: row.get("assignee_display")?,
        reporter_display: row.get("reporter_display")?,
        project_key: row.get("project_key")?,
        project_name: row.get("project_name")?,
        updated_at: row.get("updated_at")?,
        created_at: row.get("created_at")?,
        browse_url: row.get("browse_url")?,
        labels,
    })
}

/// Issues that are not Done (the "In Progress" glance).
pub fn in_progress(conn: &Connection) -> rusqlite::Result<Vec<Issue>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM issues WHERE status_category != 'done' ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_issue)?;
    rows.collect()
}

/// Issues updated at/after `since` (RFC3339) — the "Recent" glance.
pub fn recent(conn: &Connection, since: &str) -> rusqlite::Result<Vec<Issue>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM issues WHERE updated_at >= ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([since], row_to_issue)?;
    rows.collect()
}

// ------------------------------ activity -----------------------------------

pub fn upsert_activity(conn: &Connection, events: &[ActivityEvent]) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO issue_activity
            (issue_key, activity_type, activity_at, actor_display, old_value, new_value, text_summary)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
    )?;
    for e in events {
        stmt.execute(params![
            e.issue_key,
            e.activity_type,
            e.activity_at,
            e.actor_display,
            e.old_value,
            e.new_value,
            e.text_summary,
        ])?;
    }
    Ok(())
}

/// Activity within [start, end] (RFC3339) — drives standup generation.
pub fn activity_in_range(
    conn: &Connection,
    start: &str,
    end: &str,
) -> rusqlite::Result<Vec<ActivityEvent>> {
    let mut stmt = conn.prepare(
        "SELECT issue_key, activity_type, activity_at, actor_display, old_value, new_value, text_summary
         FROM issue_activity
         WHERE activity_at >= ?1 AND activity_at <= ?2
         ORDER BY activity_at ASC",
    )?;
    let rows = stmt.query_map([start, end], |row| {
        Ok(ActivityEvent {
            issue_key: row.get(0)?,
            activity_type: row.get(1)?,
            activity_at: row.get(2)?,
            actor_display: row.get(3)?,
            old_value: row.get(4)?,
            new_value: row.get(5)?,
            text_summary: row.get(6)?,
        })
    })?;
    rows.collect()
}

/// Issues touched (updated) within [start, end] — the standup candidate set.
pub fn issues_in_range(
    conn: &Connection,
    start: &str,
    end: &str,
) -> rusqlite::Result<Vec<Issue>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM issues WHERE updated_at >= ?1 AND updated_at <= ?2 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([start, end], row_to_issue)?;
    rows.collect()
}

// ------------------------------ sync runs ----------------------------------

pub fn start_sync_run(conn: &Connection, started_at: &str) -> rusqlite::Result<i64> {
    conn.execute("INSERT INTO sync_runs (started_at) VALUES (?1)", params![started_at])?;
    Ok(conn.last_insert_rowid())
}

pub fn finish_sync_run(
    conn: &Connection,
    id: i64,
    finished_at: &str,
    ok: bool,
    issue_count: i64,
    message: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_runs SET finished_at=?1, ok=?2, issue_count=?3, message=?4 WHERE id=?5",
        params![finished_at, ok as i64, issue_count, message, id],
    )?;
    Ok(())
}

pub fn sync_status(conn: &Connection) -> rusqlite::Result<SyncStatus> {
    let last_run: Option<(String, i64, Option<String>, i64)> = conn
        .query_row(
            "SELECT COALESCE(finished_at, started_at), ok, message, issue_count
             FROM sync_runs ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();

    let last_success: Option<String> = conn
        .query_row(
            "SELECT finished_at FROM sync_runs WHERE ok = 1 ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    Ok(match last_run {
        Some((at, ok, msg, count)) => SyncStatus {
            last_run_at: Some(at),
            last_success_at: last_success,
            ok: ok != 0,
            message: msg,
            issue_count: count,
        },
        None => SyncStatus {
            last_run_at: None,
            last_success_at: None,
            ok: true,
            message: None,
            issue_count: 0,
        },
    })
}

// --------------------------- generated posts -------------------------------

pub fn save_generated_post(
    conn: &Connection,
    generated_at: &str,
    range_start: &str,
    range_end: &str,
    formatter_key: &str,
    draft_text: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO generated_posts
            (generated_at, time_range_start, time_range_end, formatter_key, draft_text)
         VALUES (?1,?2,?3,?4,?5)",
        params![generated_at, range_start, range_end, formatter_key, draft_text],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delivery bookkeeping — wired for when direct posting lands (v1 uses copy).
#[allow(dead_code)]
pub fn mark_post_delivered(
    conn: &Connection,
    id: i64,
    delivered_at: &str,
    status: &str,
    destination: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE generated_posts SET delivery_status=?1, delivered_at=?2, destination_summary=?3 WHERE id=?4",
        params![status, delivered_at, destination, id],
    )?;
    Ok(())
}
