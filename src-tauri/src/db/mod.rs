//! Local SQLite cache (bundled SQLite). Stores repo aliases, cached commits, contributor
//! stats, CI status, and user config so dashboards load instantly and the app compounds
//! in utility over time (the Hook model's "Investment" layer).

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;

pub mod cache;
pub mod migrations;

/// Open (creating if needed) the cache database at `db_path`, applying all migrations.
pub fn open(db_path: &Path) -> AppResult<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(db_path)?;
    // `execute_batch` tolerates PRAGMAs that return rows (e.g. journal_mode).
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrations::runner().to_latest(&mut conn)?;
    Ok(conn)
}
