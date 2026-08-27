//! Repository functions over the SQLite connection.
//!
//! `IssueRepository` / `ActivityRepository` from the doc, expressed as plain
//! functions taking `&Connection`. Callers hold the `Db` mutex and pass the
//! guard in. Kept intentionally boring.

use crate::model::{ActivityEvent, Issue, PullRequest, SubmittedReview, SyncStatus};
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

/// Wipe all cached issues and derived activity. Used when switching data
/// sources (e.g. fake-data mode OFF) so stale rows from the old source — which
/// live under the same `source='jira'` key and would otherwise never age out —
/// don't mix with the new source's data.
pub fn clear_issue_cache(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM issue_activity", [])?;
    conn.execute("DELETE FROM issues", [])?;
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

// --------------------------- pull requests ---------------------------------

/// Replace the cached review queue. A full replace rather than an upsert: a PR
/// that dropped out (merged, approved, aged past the window) must disappear from
/// the tab, and `last_seen_at` bookkeeping can't express that on its own.
/// Checkoffs live in their own table and are untouched by this.
pub fn replace_pull_requests(
    conn: &Connection,
    prs: &[PullRequest],
    now: &str,
) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM pull_requests", [])?;
    let mut stmt = conn.prepare(
        "INSERT INTO pull_requests (
            repo, number, title, url, author, created_at, updated_at,
            review_decision, additions, deletions, changed_files,
            reviewers_json, reasons_json, is_direct, last_seen_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
    )?;
    for p in prs {
        stmt.execute(params![
            p.repo,
            p.number,
            p.title,
            p.url,
            p.author,
            p.created_at,
            p.updated_at,
            p.review_decision,
            p.additions,
            p.deletions,
            p.changed_files,
            serde_json::to_string(&p.human_reviewers).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&p.reasons).unwrap_or_else(|_| "[]".into()),
            p.is_direct as i64,
            now,
        ])?;
    }
    Ok(())
}

/// The cached review queue, with each PR's checkoff time joined on. Direct
/// requests/assignments sort first (the trump rule), then newest.
pub fn pull_requests(conn: &Connection) -> rusqlite::Result<Vec<PullRequest>> {
    let mut stmt = conn.prepare(
        "SELECT p.repo, p.number, p.title, p.url, p.author, p.created_at, p.updated_at,
                p.review_decision, p.additions, p.deletions, p.changed_files,
                p.reviewers_json, p.reasons_json, p.is_direct, c.checked_at
         FROM pull_requests p
         LEFT JOIN pr_review_checkoffs c
                ON c.repo = p.repo AND c.number = p.number
         ORDER BY p.is_direct DESC, p.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let reviewers: String = r.get(11)?;
        let reasons: String = r.get(12)?;
        Ok(PullRequest {
            repo: r.get(0)?,
            number: r.get(1)?,
            title: r.get(2)?,
            url: r.get(3)?,
            author: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
            review_decision: r.get(7)?,
            additions: r.get(8)?,
            deletions: r.get(9)?,
            changed_files: r.get(10)?,
            human_reviewers: serde_json::from_str(&reviewers).unwrap_or_default(),
            reasons: serde_json::from_str(&reasons).unwrap_or_default(),
            is_direct: r.get::<_, i64>(13)? != 0,
            reviewed_at: r.get(14)?,
        })
    })?;
    rows.collect()
}

/// Tick a PR off (or untick it). Stores title/url alongside so a checked-off PR
/// can still be named in the standup after it leaves the cache.
pub fn set_pr_checkoff(
    conn: &Connection,
    repo: &str,
    number: i64,
    checked: bool,
    now: &str,
) -> rusqlite::Result<()> {
    if checked {
        conn.execute(
            "INSERT INTO pr_review_checkoffs (repo, number, checked_at, title, url)
             SELECT ?1, ?2, ?3, p.title, p.url
               FROM pull_requests p WHERE p.repo = ?1 AND p.number = ?2
             ON CONFLICT(repo, number) DO UPDATE SET checked_at=excluded.checked_at",
            params![repo, number, now],
        )?;
        // The PR may not be in the cache (checked off from a stale view); still
        // record the checkoff so the standup count doesn't lose it.
        conn.execute(
            "INSERT OR IGNORE INTO pr_review_checkoffs (repo, number, checked_at)
             VALUES (?1, ?2, ?3)",
            params![repo, number, now],
        )?;
    } else {
        conn.execute(
            "DELETE FROM pr_review_checkoffs WHERE repo = ?1 AND number = ?2",
            params![repo, number],
        )?;
    }
    Ok(())
}

// -------------------------- submitted reviews ------------------------------

/// Upsert reviews the user submitted. Upsert rather than replace: each fetch
/// covers a bounded window, and a replace would drop everything older than it.
pub fn upsert_submitted_reviews(
    conn: &Connection,
    reviews: &[SubmittedReview],
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO submitted_reviews
            (repo, number, title, url, author, submitted_at, state, pr_state)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(repo, number, submitted_at) DO UPDATE SET
            title=excluded.title,
            url=excluded.url,
            author=excluded.author,
            state=excluded.state,
            pr_state=excluded.pr_state",
    )?;
    for r in reviews {
        stmt.execute(params![
            r.repo,
            r.number,
            r.title,
            r.url,
            r.author,
            r.submitted_at,
            r.state,
            r.pr_state,
        ])?;
    }
    Ok(())
}

/// Reviews submitted at/after `since` (RFC3339), newest first.
pub fn submitted_reviews_since(
    conn: &Connection,
    since: &str,
) -> rusqlite::Result<Vec<SubmittedReview>> {
    let mut stmt = conn.prepare(
        "SELECT repo, number, title, url, author, submitted_at, state, pr_state
         FROM submitted_reviews
         WHERE submitted_at >= ?1
         ORDER BY submitted_at DESC",
    )?;
    let rows = stmt.query_map([since], |r| {
        Ok(SubmittedReview {
            repo: r.get(0)?,
            number: r.get(1)?,
            title: r.get(2)?,
            url: r.get(3)?,
            author: r.get(4)?,
            submitted_at: r.get(5)?,
            state: r.get(6)?,
            pr_state: r.get(7)?,
        })
    })?;
    rows.collect()
}

/// Distinct PRs credited in [start, end] from either signal: a review GitHub
/// recorded, or a manual checkoff. `UNION` (not `UNION ALL`) is what makes this
/// distinct per PR: three passes over one PR, or a PR both reviewed and ticked
/// off, counts once.
pub fn review_credit_count_in_range(
    conn: &Connection,
    start: &str,
    end: &str,
) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM (
            SELECT repo, number FROM submitted_reviews
             WHERE submitted_at >= ?1 AND submitted_at <= ?2
            UNION
            SELECT repo, number FROM pr_review_checkoffs
             WHERE checked_at >= ?1 AND checked_at <= ?2
         )",
        params![start, end],
        |r| r.get(0),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn migrated() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn fake_issue(key: &str) -> Issue {
        Issue {
            source: "jira".into(),
            source_issue_id: key.into(),
            issue_key: key.into(),
            summary: "Sample".into(),
            status_name: "In Progress".into(),
            status_category: "indeterminate".into(),
            assignee_display: None,
            reporter_display: None,
            project_key: None,
            project_name: None,
            updated_at: "2026-01-01T00:00:00+00:00".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            browse_url: String::new(),
            labels: vec![],
        }
    }

    // Mirrors the fake→real transition: sample rows in the cache get wiped so
    // they can't bleed into real Jira data (both live under source='jira').
    #[test]
    fn clear_issue_cache_empties_issues_and_activity() {
        let conn = migrated();
        let now = "2026-01-01T00:00:00+00:00";
        upsert_issues(&conn, &[fake_issue("APP-1"), fake_issue("PLAT-2")], now).unwrap();
        upsert_activity(
            &conn,
            &[ActivityEvent {
                issue_key: "APP-1".into(),
                activity_type: "status_change".into(),
                activity_at: now.into(),
                actor_display: None,
                old_value: None,
                new_value: Some("Done".into()),
                text_summary: Some("→ Done".into()),
            }],
        )
        .unwrap();
        assert_eq!(in_progress(&conn).unwrap().len(), 2);

        clear_issue_cache(&conn).unwrap();

        assert_eq!(in_progress(&conn).unwrap().len(), 0);
        let activity_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM issue_activity", [], |r| r.get(0))
            .unwrap();
        assert_eq!(activity_count, 0);
    }

    fn fake_pr(repo: &str, number: i64) -> PullRequest {
        PullRequest {
            repo: repo.into(),
            number,
            title: "Some change".into(),
            url: format!("https://github.com/org/{repo}/pull/{number}"),
            author: "someone".into(),
            created_at: "2026-08-25T00:00:00+00:00".into(),
            updated_at: "2026-08-26T00:00:00+00:00".into(),
            review_decision: "REVIEW_REQUIRED".into(),
            additions: 10,
            deletions: 2,
            changed_files: 3,
            human_reviewers: vec![],
            reasons: vec!["team".into()],
            is_direct: false,
            reviewed_at: None,
        }
    }

    // The point of a separate checkoff table: a PR that leaves the queue (merged,
    // approved, or aged out) must not take the standup's review count with it.
    #[test]
    fn checkoff_survives_the_pr_leaving_the_cache() {
        let conn = migrated();
        let now = "2026-08-27T12:00:00+00:00";
        replace_pull_requests(&conn, &[fake_pr("svc", 7), fake_pr("ui", 9)], now).unwrap();

        set_pr_checkoff(&conn, "svc", 7, true, now).unwrap();
        let rows = pull_requests(&conn).unwrap();
        assert_eq!(rows.iter().filter(|p| p.reviewed_at.is_some()).count(), 1);

        // Next sync: svc#7 got merged and is gone from the queue.
        replace_pull_requests(&conn, &[fake_pr("ui", 9)], now).unwrap();
        assert_eq!(pull_requests(&conn).unwrap().len(), 1);

        // The checkoff is still counted for the standup.
        let count = review_credit_count_in_range(
            &conn,
            "2026-08-27T00:00:00+00:00",
            "2026-08-27T23:59:59+00:00",
        )
        .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn checkoff_count_respects_the_window_and_unticking() {
        let conn = migrated();
        replace_pull_requests(&conn, &[fake_pr("svc", 7)], "2026-08-27T12:00:00+00:00").unwrap();

        set_pr_checkoff(&conn, "svc", 7, true, "2026-08-20T09:00:00+00:00").unwrap();
        let day = |s: &str, e: &str| review_credit_count_in_range(&conn, s, e).unwrap();

        // Outside the window → not counted.
        assert_eq!(day("2026-08-27T00:00:00+00:00", "2026-08-27T23:59:59+00:00"), 0);
        // Inside → counted.
        assert_eq!(day("2026-08-20T00:00:00+00:00", "2026-08-20T23:59:59+00:00"), 1);

        // Unticking removes it.
        set_pr_checkoff(&conn, "svc", 7, false, "2026-08-20T10:00:00+00:00").unwrap();
        assert_eq!(day("2026-08-20T00:00:00+00:00", "2026-08-20T23:59:59+00:00"), 0);
    }

    fn fake_review(repo: &str, number: i64, submitted_at: &str) -> SubmittedReview {
        SubmittedReview {
            repo: repo.into(),
            number,
            title: "Some change".into(),
            url: format!("https://github.com/org/{repo}/pull/{number}"),
            author: "someone".into(),
            submitted_at: submitted_at.into(),
            state: "APPROVED".into(),
            pr_state: "MERGED".into(),
        }
    }

    // Each fetch covers a bounded window, so a second sync must not wipe reviews
    // from before it.
    #[test]
    fn upserting_reviews_keeps_older_rows() {
        let conn = migrated();
        upsert_submitted_reviews(
            &conn,
            &[
                fake_review("svc", 1, "2026-08-20T09:00:00+00:00"),
                fake_review("svc", 2, "2026-08-26T09:00:00+00:00"),
            ],
        )
        .unwrap();
        // A later sync with a narrower window only reports the recent one.
        upsert_submitted_reviews(&conn, &[fake_review("svc", 2, "2026-08-26T09:00:00+00:00")])
            .unwrap();
        assert_eq!(submitted_reviews_since(&conn, "2026-08-01T00:00:00+00:00").unwrap().len(), 2);
    }

    // Two passes over the same PR is one review as far as standup is concerned.
    #[test]
    fn review_count_is_distinct_per_pr() {
        let conn = migrated();
        upsert_submitted_reviews(
            &conn,
            &[
                fake_review("svc", 7, "2026-08-26T09:00:00+00:00"),
                fake_review("svc", 7, "2026-08-26T14:00:00+00:00"),
                fake_review("ui", 3, "2026-08-26T15:00:00+00:00"),
            ],
        )
        .unwrap();
        let day = ("2026-08-26T00:00:00+00:00", "2026-08-26T23:59:59+00:00");
        assert_eq!(submitted_reviews_since(&conn, day.0).unwrap().len(), 3);
        assert_eq!(review_credit_count_in_range(&conn, day.0, day.1).unwrap(), 2);
        // Outside the window.
        assert_eq!(
            review_credit_count_in_range(&conn, "2026-08-27T00:00:00+00:00", "2026-08-27T23:59:59+00:00")
                .unwrap(),
            0
        );
    }

    // A PR both formally reviewed and manually ticked off counts once.
    #[test]
    fn review_credit_unions_reviews_and_checkoffs() {
        let conn = migrated();
        let (start, end) = ("2026-08-26T00:00:00+00:00", "2026-08-26T23:59:59+00:00");
        upsert_submitted_reviews(
            &conn,
            &[
                fake_review("svc", 7, "2026-08-26T09:00:00+00:00"),
                fake_review("ui", 3, "2026-08-26T10:00:00+00:00"),
            ],
        )
        .unwrap();
        // svc#7 overlaps a checkoff; api#1 is checkoff-only.
        set_pr_checkoff(&conn, "svc", 7, true, "2026-08-26T11:00:00+00:00").unwrap();
        set_pr_checkoff(&conn, "api", 1, true, "2026-08-26T12:00:00+00:00").unwrap();

        assert_eq!(review_credit_count_in_range(&conn, start, end).unwrap(), 3);
    }

    // Ticking off a PR that isn't in the cache (stale view, or it just left the
    // queue) still has to record, or the standup count silently drops it.
    #[test]
    fn checkoff_records_even_when_pr_is_not_cached() {
        let conn = migrated();
        let now = "2026-08-27T12:00:00+00:00";
        set_pr_checkoff(&conn, "ghost", 1, true, now).unwrap();
        assert_eq!(
            review_credit_count_in_range(&conn, "2026-08-27T00:00:00+00:00", "2026-08-27T23:59:59+00:00")
                .unwrap(),
            1
        );
    }
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
