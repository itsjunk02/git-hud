//! Merge-conflict inspection.
//!
//! Conflict *detection* is real (it reads the index for conflict entries); the split
//! "ours/theirs/base" hunk extraction and the resolution write-back are intentionally
//! stubbed for this scaffold — the shapes and IPC wiring are in place so the Merge
//! Conflict Editor screen can be filled in without touching the frontend contract.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppResult;

/// A conflicted file with its three-way sides (sides are placeholders in the scaffold).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConflictHunk {
    pub file: String,
    pub ours: String,
    pub theirs: String,
    pub base: String,
}

/// Real: enumerate conflicted paths from the repository index.
pub fn list_conflicts(path: &str) -> AppResult<Vec<ConflictHunk>> {
    let repo = git2::Repository::discover(path)?;
    let index = repo.index()?;

    let mut out = Vec::new();
    if let Ok(conflicts) = index.conflicts() {
        for conflict in conflicts.flatten() {
            let file = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref())
                .and_then(|entry| String::from_utf8(entry.path.clone()).ok())
                .unwrap_or_default();
            out.push(ConflictHunk {
                file,
                // TODO: read blob contents for each stage to populate the split diff.
                ours: String::new(),
                theirs: String::new(),
                base: String::new(),
            });
        }
    }
    Ok(out)
}

/// Stub: a real implementation writes the chosen resolution and stages the file.
pub fn resolve_conflict(_path: &str, _file: &str, _resolution: &str) -> AppResult<()> {
    // Intentionally a no-op in the scaffold.
    Ok(())
}
