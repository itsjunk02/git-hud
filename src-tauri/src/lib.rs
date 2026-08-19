//! git-hud backend entrypoint: Tauri command surface + app wiring.
//!
//! Architecture:
//! - `engine/` — pure Git/collab/remote logic (no Tauri types).
//! - `db/`     — local SQLite cache (Investment layer).
//! - here      — thin `#[tauri::command]` wrappers, typed-IPC generation (tauri-specta),
//!               the tray HUD + background workers (Trigger layer).
//!
//! Commands return `Result<T, String>`: engine [`AppError`]s are stringified at the
//! boundary so the generated TypeScript models failures as a plain `string`.

mod db;
mod engine;
mod error;

use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{Emitter, Manager};

use engine::conflicts::ConflictHunk;
use engine::model::{BranchInfo, CommitInfo, RepoStatus, RepoSummary};
use engine::remote::CiStatus;
use engine::stats::ContributorStat;
use error::AppError;

/// Shared application state, managed by Tauri and injected into commands.
pub struct AppState {
    /// Single pooled connection to the local cache DB, guarded for cross-thread use.
    pub db: Mutex<Connection>,
    /// Absolute path of the repository the UI is currently focused on.
    pub current_repo: Mutex<Option<String>>,
}

impl AppState {
    fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            current_repo: Mutex::new(None),
        }
    }
}

/// Resolve the active repository path or return a user-facing error.
fn require_repo(state: &AppState) -> Result<String, String> {
    state
        .current_repo
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::NoRepo.to_string())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Open a repository: summarize it, persist + cache it, mark it active, and start
/// watching its `.git` dir for ambient change triggers.
#[tauri::command]
#[specta::specta]
fn open_repository(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<RepoSummary, String> {
    let summary = engine::git::summarize(&path).map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().unwrap();
        let repo_id = db::cache::upsert_repo(&conn, &path).map_err(|e| e.to_string())?;
        if let Ok(commits) = engine::git::list_commits(&path, 200) {
            let _ = db::cache::cache_commits(&conn, repo_id, &commits);
        }
    }

    *state.current_repo.lock().unwrap() = Some(path.clone());
    engine::watcher::spawn(app, path);

    Ok(summary)
}

/// Path of the currently active repository, if any.
#[tauri::command]
#[specta::specta]
fn current_repo(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.current_repo.lock().unwrap().clone())
}

/// Commits (with graph lanes) for the active repo — powers the Canvas timeline.
#[tauri::command]
#[specta::specta]
fn list_commits(
    state: tauri::State<'_, AppState>,
    limit: u32,
) -> Result<Vec<CommitInfo>, String> {
    let path = require_repo(&state)?;
    engine::git::list_commits(&path, limit as usize).map_err(|e| e.to_string())
}

/// Local + remote branches for the active repo.
#[tauri::command]
#[specta::specta]
fn list_branches(state: tauri::State<'_, AppState>) -> Result<Vec<BranchInfo>, String> {
    let path = require_repo(&state)?;
    engine::git::list_branches(&path).map_err(|e| e.to_string())
}

/// Working-tree status for the active repo.
#[tauri::command]
#[specta::specta]
fn get_status(state: tauri::State<'_, AppState>) -> Result<RepoStatus, String> {
    let path = require_repo(&state)?;
    engine::git::status(&path).map_err(|e| e.to_string())
}

/// Conflicted files in the active repo (Merge Conflict Editor data source).
#[tauri::command]
#[specta::specta]
fn list_conflicts(state: tauri::State<'_, AppState>) -> Result<Vec<ConflictHunk>, String> {
    let path = require_repo(&state)?;
    engine::conflicts::list_conflicts(&path).map_err(|e| e.to_string())
}

/// Apply a conflict resolution (stub in the scaffold).
#[tauri::command]
#[specta::specta]
fn resolve_conflict(
    state: tauri::State<'_, AppState>,
    file: String,
    resolution: String,
) -> Result<(), String> {
    let path = require_repo(&state)?;
    engine::conflicts::resolve_conflict(&path, &file, &resolution).map_err(|e| e.to_string())
}

/// "Who Did What" contributor metrics for the active repo.
#[tauri::command]
#[specta::specta]
fn contributor_stats(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContributorStat>, String> {
    let path = require_repo(&state)?;
    engine::stats::contributor_stats(&path).map_err(|e| e.to_string())
}

/// CI/CD + compliance badges for the active repo: real DCO from local commit trailers +
/// build/test from the GitHub Checks API (via `gh`). The poll is persisted to the local
/// cache with a real `updated_at` before returning.
#[tauri::command]
#[specta::specta]
fn get_ci_status(state: tauri::State<'_, AppState>) -> Result<Vec<CiStatus>, String> {
    let path = require_repo(&state)?;
    let statuses = engine::remote::poll_ci(&path);

    // Best-effort cache write — a failed persist must not fail the read.
    {
        let conn = state.db.lock().unwrap();
        if let Ok(repo_id) = db::cache::upsert_repo(&conn, &path) {
            let _ = db::cache::upsert_ci_status(&conn, repo_id, &statuses);
        }
    }

    Ok(statuses)
}

/// Read a persisted user-config value (custom filters, aliases, groupings, …).
#[tauri::command]
#[specta::specta]
fn get_config(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    let conn = state.db.lock().unwrap();
    db::cache::get_config(&conn, &key).map_err(|e| e.to_string())
}

/// Persist a user-config value.
#[tauri::command]
#[specta::specta]
fn set_config(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::cache::set_config(&conn, &key, &value).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// IPC binding builder + app wiring
// ---------------------------------------------------------------------------

/// Construct the tauri-specta builder with every command registered. Shared by the
/// runtime handler and the binding-export test so the two never drift.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        open_repository,
        current_repo,
        list_commits,
        list_branches,
        get_status,
        list_conflicts,
        resolve_conflict,
        contributor_stats,
        get_ci_status,
        get_config,
        set_config,
    ])
}

/// Build the OS tray HUD (Trigger layer): quick access + quit.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open_item = MenuItem::with_id(app, "open", "Open git-hud", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("git-hud — monitoring your repositories")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "open" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Background CI/CD poller (Trigger layer). Emits a `ci-poll` heartbeat the frontend can
/// react to; a real implementation would diff forge status and notify on change.
fn spawn_ci_poller(handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            ticker.tick().await;
            let _ = handle.emit("ci-poll", ());
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // Regenerate the TypeScript IPC bindings on every debug run.
    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/services/bindings.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // Local cache DB (Investment layer).
            let db_path = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("git-hud.db");
            let conn = db::open(&db_path).expect("failed to open cache database");
            app.manage(AppState::new(conn));

            // Ambient triggers.
            build_tray(app)?;
            spawn_ci_poller(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// Generates `src/services/bindings.ts` without launching the GUI, and doubles as a
    /// compile check of the whole command surface. Run: `cargo test export_bindings`.
    #[test]
    fn export_bindings() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default(),
                "../src/services/bindings.ts",
            )
            .expect("failed to export bindings");
    }
}
