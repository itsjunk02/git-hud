//! IPC-facing data models shared between the Rust engine and the TypeScript frontend.
//!
//! Every type derives [`specta::Type`] so `tauri-specta` can generate a matching
//! TypeScript interface. Numeric fields use `u32`/`f64` (never `i64`/`usize`) so they
//! map to a plain TS `number` — specta treats 64-bit integers as `bigint`, which would
//! mismatch the JSON numbers Tauri actually sends. Timestamps are `f64` Unix seconds.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A single commit, enriched with a `lane` index used by the Canvas timeline renderer.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CommitInfo {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    /// Author time, Unix epoch seconds.
    pub timestamp: f64,
    pub parent_ids: Vec<String>,
    /// Horizontal lane assigned by the graph-layout pass (0-based).
    pub lane: u32,
}

/// A local or remote branch reference.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
    /// Hex OID the branch points at, if resolvable.
    pub target: Option<String>,
}

/// Working-tree status for a single path.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FileStatus {
    pub path: String,
    /// One of: `new`, `modified`, `deleted`, `renamed`, `typechange`, `conflicted`, `ignored`.
    pub status: String,
    pub staged: bool,
}

/// Aggregate working-tree + HEAD status for the open repository.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RepoStatus {
    pub head_branch: Option<String>,
    pub files: Vec<FileStatus>,
    pub ahead: u32,
    pub behind: u32,
    pub has_conflicts: bool,
}

/// State of the background remote-fetch worker, surfaced in the UI's sync indicator.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct FetchStatus {
    /// Whether the most recent fetch attempt succeeded.
    pub last_ok: bool,
    /// Unix epoch seconds of the last attempt (0 if none yet this session).
    pub last_at: f64,
    /// Empty on success; a short reason on failure.
    pub message: String,
    /// True while a fetch is currently in flight.
    pub running: bool,
}

/// Per-project card data for the multi-project sidebar: identity + live sync state.
/// Bundles what one card needs so a single command feeds the whole open-projects list.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProjectStatus {
    pub path: String,
    /// Directory basename, shown as the card title.
    pub name: String,
    pub head_branch: Option<String>,
    pub commit_count: u32,
    pub branch_count: u32,
    /// Commits the local branch is ahead / behind its remote.
    pub ahead: u32,
    pub behind: u32,
    pub has_conflicts: bool,
    /// Whether this is the currently focused project.
    pub active: bool,
    /// This project's own background-fetch status.
    pub fetch: FetchStatus,
}

/// Lightweight summary returned when a repository is opened.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RepoSummary {
    pub path: String,
    pub head_branch: Option<String>,
    pub commit_count: u32,
    pub branch_count: u32,
}
