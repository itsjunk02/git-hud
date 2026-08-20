//! The git-hud engine: all Git parsing, collaboration metrics, remote status, and the
//! filesystem watcher. Pure logic, decoupled from Tauri command wiring (see `lib.rs`).

pub mod conflicts;
pub mod git;
pub mod model;
pub mod remote;
pub mod reviews;
pub mod stats;
pub mod sync;
pub mod watcher;
