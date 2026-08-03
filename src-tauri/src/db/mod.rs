//! Local data layer: SQLite connection, migrations, and repositories.
//!
//! One connection guarded by a Mutex is plenty for a single-user local app.
//! Repositories are thin functions over that connection — the "repository
//! layer" seam from the doc — so a future backend could reimplement them
//! against a remote store without touching callers.

pub mod migrations;
pub mod repo;
pub mod settings_repo;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// Wraps the SQLite connection. Held in Tauri state.
pub struct Db(pub Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::run(&conn)?;
        Ok(Db(Mutex::new(conn)))
    }
}
