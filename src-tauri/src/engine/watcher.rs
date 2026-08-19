//! Background filesystem watcher for a repository's `.git` directory.
//!
//! Runs on a dedicated OS thread and emits a `repo-changed` event to the frontend
//! whenever refs/index/objects change — the ambient "Trigger" in the Hook model.

use std::path::Path;
use std::sync::mpsc::channel;

use notify::{EventKind, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

/// Event name emitted to the webview when the watched repo changes on disk.
pub const REPO_CHANGED_EVENT: &str = "repo-changed";

/// Spawn a watcher thread for `<repo_path>/.git`. The thread lives until the channel
/// closes (i.e. for the remainder of the process); it holds the `Watcher` alive so
/// events keep flowing.
pub fn spawn(app: AppHandle, repo_path: String) {
    std::thread::spawn(move || {
        let git_dir = Path::new(&repo_path).join(".git");
        let (tx, rx) = channel();

        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(err) => {
                eprintln!("[git-hud] failed to create watcher: {err}");
                return;
            }
        };

        if let Err(err) = watcher.watch(&git_dir, RecursiveMode::Recursive) {
            eprintln!("[git-hud] failed to watch {}: {err}", git_dir.display());
            return;
        }

        for event in rx {
            let Ok(event) = event else { continue };
            if matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                let _ = app.emit(REPO_CHANGED_EVENT, &repo_path);
            }
        }
    });
}
