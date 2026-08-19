//! Central error type for the git-hud engine.
//!
//! Engine/db functions return [`AppError`]. At the Tauri command boundary these are
//! converted to `String` (via [`AppError::to_string`]) so the generated TypeScript
//! bindings model command failures as a simple `string` — this keeps specta type
//! generation clean and avoids leaking Rust-specific error shapes to the frontend.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no repository is currently open")]
    NoRepo,
}

/// Serialize as a plain message string for transport to the frontend.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
