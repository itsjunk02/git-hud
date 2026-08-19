//! Versioned schema migrations, applied on startup via `rusqlite_migration`.

use rusqlite_migration::{Migrations, M};

/// The ordered set of migrations. Append new `M::up(...)` entries — never edit past ones.
pub fn runner() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        r#"
        CREATE TABLE repos (
            id          INTEGER PRIMARY KEY,
            path        TEXT NOT NULL UNIQUE,
            alias       TEXT,
            last_opened INTEGER
        );

        CREATE TABLE commits_cache (
            repo_id      INTEGER NOT NULL,
            id           TEXT NOT NULL,
            summary      TEXT,
            author_name  TEXT,
            author_email TEXT,
            timestamp    INTEGER,
            lane         INTEGER,
            PRIMARY KEY (repo_id, id),
            FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
        );

        CREATE TABLE contributor_stats (
            repo_id      INTEGER NOT NULL,
            author_email TEXT NOT NULL,
            author_name  TEXT,
            commits      INTEGER,
            PRIMARY KEY (repo_id, author_email),
            FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
        );

        CREATE TABLE ci_status (
            repo_id    INTEGER NOT NULL,
            pipeline   TEXT NOT NULL,
            status     TEXT,
            badge      TEXT,
            updated_at INTEGER,
            PRIMARY KEY (repo_id, pipeline),
            FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
        );

        CREATE TABLE config (
            key   TEXT PRIMARY KEY,
            value TEXT
        );
        "#,
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        // Guards against typos in the SQL above.
        assert!(runner().validate().is_ok());
    }
}
