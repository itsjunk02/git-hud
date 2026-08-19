//! Query/upsert helpers over the SQLite cache.

use rusqlite::{params, Connection};

use crate::engine::model::CommitInfo;
use crate::error::AppResult;

/// Insert the repo (or bump its `last_opened`) and return its row id.
pub fn upsert_repo(conn: &Connection, path: &str) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO repos (path, last_opened) VALUES (?1, strftime('%s','now'))
         ON CONFLICT(path) DO UPDATE SET last_opened = strftime('%s','now')",
        params![path],
    )?;
    let id = conn.query_row(
        "SELECT id FROM repos WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Cache a batch of commits for instant subsequent loads.
pub fn cache_commits(conn: &Connection, repo_id: i64, commits: &[CommitInfo]) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO commits_cache
                (repo_id, id, summary, author_name, author_email, timestamp, lane)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for c in commits {
            stmt.execute(params![
                repo_id,
                c.id,
                c.summary,
                c.author_name,
                c.author_email,
                c.timestamp,
                c.lane,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Read a user config value by key.
pub fn get_config(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let value = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(value)
}

/// Upsert a user config value.
pub fn set_config(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}
