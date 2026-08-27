//! Schema migrations. A simple `schema_version` pragma-style table drives an
//! ordered list of migration steps — enough for v1, and easy to extend.

use rusqlite::Connection;

/// Ordered migrations. Append new ones; never edit an already-shipped step.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
    CREATE TABLE IF NOT EXISTS app_settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS issues (
        source           TEXT NOT NULL DEFAULT 'jira',
        source_issue_id  TEXT NOT NULL,
        issue_key        TEXT NOT NULL,
        summary          TEXT NOT NULL,
        status_name      TEXT NOT NULL,
        status_category  TEXT NOT NULL,
        assignee_display TEXT,
        reporter_display TEXT,
        project_key      TEXT,
        project_name     TEXT,
        updated_at       TEXT NOT NULL,
        created_at       TEXT NOT NULL,
        browse_url       TEXT NOT NULL,
        labels_json      TEXT NOT NULL DEFAULT '[]',
        first_seen_at    TEXT NOT NULL,
        last_seen_at     TEXT NOT NULL,
        raw_json         TEXT,
        PRIMARY KEY (source, issue_key)
    );
    CREATE INDEX IF NOT EXISTS idx_issues_updated ON issues(updated_at);
    CREATE INDEX IF NOT EXISTS idx_issues_status_category ON issues(status_category);

    CREATE TABLE IF NOT EXISTS issue_activity (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        issue_key     TEXT NOT NULL,
        activity_type TEXT NOT NULL,
        activity_at   TEXT NOT NULL,
        actor_display TEXT,
        old_value     TEXT,
        new_value     TEXT,
        text_summary  TEXT,
        raw_json      TEXT
    );
    -- Dedup key that treats NULL old/new values as equal (COALESCE), so
    -- INSERT OR IGNORE won't drop distinct changes that share a timestamp but
    -- differ only in old_value, and won't let NULLs slip past the constraint.
    CREATE UNIQUE INDEX IF NOT EXISTS idx_activity_dedup ON issue_activity(
        issue_key, activity_type, activity_at,
        COALESCE(old_value, ''), COALESCE(new_value, '')
    );
    CREATE INDEX IF NOT EXISTS idx_activity_at ON issue_activity(activity_at);
    CREATE INDEX IF NOT EXISTS idx_activity_issue ON issue_activity(issue_key);

    CREATE TABLE IF NOT EXISTS sync_runs (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        started_at   TEXT NOT NULL,
        finished_at  TEXT,
        ok           INTEGER NOT NULL DEFAULT 0,
        issue_count  INTEGER NOT NULL DEFAULT 0,
        message      TEXT
    );

    CREATE TABLE IF NOT EXISTS generated_posts (
        id                  INTEGER PRIMARY KEY AUTOINCREMENT,
        generated_at        TEXT NOT NULL,
        time_range_start    TEXT NOT NULL,
        time_range_end      TEXT NOT NULL,
        formatter_key       TEXT NOT NULL,
        draft_text          TEXT NOT NULL,
        delivery_status     TEXT NOT NULL DEFAULT 'draft',
        delivered_at        TEXT,
        destination_summary TEXT
    );
    "#,
    // 2: GitHub review queue — cached PRs plus durable checkoffs.
    r#"
    CREATE TABLE IF NOT EXISTS pull_requests (
        repo            TEXT NOT NULL,
        number          INTEGER NOT NULL,
        title           TEXT NOT NULL,
        url             TEXT NOT NULL,
        author          TEXT NOT NULL,
        created_at      TEXT NOT NULL,
        updated_at      TEXT NOT NULL,
        review_decision TEXT NOT NULL,
        additions       INTEGER NOT NULL DEFAULT 0,
        deletions       INTEGER NOT NULL DEFAULT 0,
        changed_files   INTEGER NOT NULL DEFAULT 0,
        reviewers_json  TEXT NOT NULL DEFAULT '[]',
        reasons_json    TEXT NOT NULL DEFAULT '[]',
        is_direct       INTEGER NOT NULL DEFAULT 0,
        last_seen_at    TEXT NOT NULL,
        PRIMARY KEY (repo, number)
    );
    CREATE INDEX IF NOT EXISTS idx_pr_created ON pull_requests(created_at);

    -- Deliberately NOT a column on pull_requests: a checkoff has to outlive the
    -- PR falling out of the query window (merged, approved, or aged past the
    -- window), otherwise the standup count loses reviews the moment they land.
    CREATE TABLE IF NOT EXISTS pr_review_checkoffs (
        repo        TEXT NOT NULL,
        number      INTEGER NOT NULL,
        checked_at  TEXT NOT NULL,
        title       TEXT,
        url         TEXT,
        PRIMARY KEY (repo, number)
    );
    CREATE INDEX IF NOT EXISTS idx_checkoff_at ON pr_review_checkoffs(checked_at);
    "#,
    // 3: reviews the user actually submitted on GitHub. Separate from
    // pr_review_checkoffs: this is observed fact from GitHub, that table is the
    // user asserting they handled something. Most reviewed PRs merge within a
    // day, so they never appear in the open review queue and can only be counted
    // from here.
    r#"
    CREATE TABLE IF NOT EXISTS submitted_reviews (
        repo         TEXT NOT NULL,
        number       INTEGER NOT NULL,
        title        TEXT NOT NULL DEFAULT '',
        url          TEXT NOT NULL DEFAULT '',
        author       TEXT NOT NULL DEFAULT '',
        submitted_at TEXT NOT NULL,
        state        TEXT NOT NULL DEFAULT '',
        pr_state     TEXT NOT NULL DEFAULT '',
        PRIMARY KEY (repo, number, submitted_at)
    );
    CREATE INDEX IF NOT EXISTS idx_submitted_at ON submitted_reviews(submitted_at);
    "#,
];

pub fn run(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
    )?;
    let current: i64 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))
        .unwrap_or(0);

    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current {
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO schema_version (version) VALUES (?1)", [version])?;
        }
    }
    Ok(())
}
