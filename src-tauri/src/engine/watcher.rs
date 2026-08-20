//! Background filesystem watcher for a repository's `.git` directory.
//!
//! Emits a `repo-changed` event (payload = the repo path) whenever refs/index/objects
//! change — the ambient "Trigger" in the Hook model. The returned [`RecommendedWatcher`]
//! handle must be kept alive by the caller to keep receiving events; dropping it stops the
//! watch. That lifecycle is how closing a project tears its watcher down.

use std::path::Path;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

/// Event name emitted to the webview when a watched repo changes on disk.
pub const REPO_CHANGED_EVENT: &str = "repo-changed";

/// Start watching `<repo_path>/.git`, emitting `repo-changed` (payload: the repo path) on
/// change. Returns the watcher handle — keep it alive to keep watching, drop it to stop.
/// Returns `None` if the watcher couldn't be created or the `.git` dir couldn't be watched.
pub fn watch(app: AppHandle, repo_path: String) -> Option<RecommendedWatcher> {
    let git_dir = Path::new(&repo_path).join(".git");
    let path_for_event = repo_path.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            let _ = app.emit(REPO_CHANGED_EVENT, &path_for_event);
        }
    })
    .map_err(|err| eprintln!("[git-hud] failed to create watcher: {err}"))
    .ok()?;

    watcher
        .watch(&git_dir, RecursiveMode::Recursive)
        .map_err(|err| eprintln!("[git-hud] failed to watch {}: {err}", git_dir.display()))
        .ok()?;

    Some(watcher)
}
