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

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use engine::conflicts::ConflictHunk;
use engine::model::{BranchInfo, CommitInfo, FetchStatus, ProjectStatus, RepoStatus, RepoSummary};
use engine::remote::CiStatus;
use engine::reviews::ReviewerStat;
use engine::stats::ContributorStat;
use error::AppError;

/// Maximum number of projects that can be open (live) at once.
const MAX_PROJECTS: usize = 7;

/// Last-seen alert state per project, so the alerts worker notifies only on *new* items.
#[derive(Default)]
pub struct AlertState {
    had_conflicts: bool,
    failing: HashSet<String>,
    seen_reviews: HashSet<u32>,
}

/// Shared application state, managed by Tauri and injected into commands.
///
/// Multi-project workspace: up to [`MAX_PROJECTS`] repos are open at once, each with its
/// own live watcher + fetch status. `active_repo` is the one the main views focus on.
pub struct AppState {
    /// Single pooled connection to the local cache DB, guarded for cross-thread use.
    pub db: Mutex<Connection>,
    /// Ordered list of open project paths (≤ [`MAX_PROJECTS`]).
    pub open_repos: Mutex<Vec<String>>,
    /// Path of the project the UI is currently focused on.
    pub active_repo: Mutex<Option<String>>,
    /// Live `.git` watcher per open project; dropping an entry stops that watcher.
    pub watchers: Mutex<HashMap<String, notify::RecommendedWatcher>>,
    /// Per-project background-fetch status, surfaced in each project's sync indicator.
    pub fetch: Mutex<HashMap<String, FetchStatus>>,
    /// Per-project last-seen alert state, so notifications fire only on new items.
    pub alerts: Mutex<HashMap<String, AlertState>>,
}

impl AppState {
    fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            open_repos: Mutex::new(Vec::new()),
            active_repo: Mutex::new(None),
            watchers: Mutex::new(HashMap::new()),
            fetch: Mutex::new(HashMap::new()),
            alerts: Mutex::new(HashMap::new()),
        }
    }
}

/// Seconds since the Unix epoch, as an `f64` for the IPC layer.
fn now_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as f64)
        .unwrap_or(0.0)
}

/// Run a single remote fetch for `path`, record the outcome in shared state, and nudge the
/// UI to reload (a fetch may have advanced `origin/*`). Shared by the manual command and
/// the background worker.
fn run_fetch(app: &tauri::AppHandle, state: &AppState, path: &str) {
    state
        .fetch
        .lock()
        .unwrap()
        .entry(path.to_string())
        .or_default()
        .running = true;
    let outcome = engine::sync::git_fetch(path);
    {
        let mut map = state.fetch.lock().unwrap();
        let f = map.entry(path.to_string()).or_default();
        f.running = false;
        f.last_ok = outcome.ok;
        f.last_at = now_epoch();
        f.message = outcome.message;
    }
    // Fetch is non-destructive but may have moved remote-tracking refs; reuse the
    // repo-changed trigger so the timeline + sync indicator refresh.
    let _ = app.emit(engine::watcher::REPO_CHANGED_EVENT, path.to_string());
}

/// Resolve the active repository path or return a user-facing error.
fn require_repo(state: &AppState) -> Result<String, String> {
    state
        .active_repo
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::NoRepo.to_string())
}

/// Persist the open-project set + active project to the config table, so the workspace
/// restores on next launch. Locks are taken sequentially (never nested) to avoid deadlock.
fn persist_workspace(state: &AppState) {
    let open = state.open_repos.lock().unwrap().clone();
    let active = state.active_repo.lock().unwrap().clone();
    let conn = state.db.lock().unwrap();
    let _ = db::cache::set_config(&conn, "open_repos", &open.join("\n"));
    let _ = db::cache::set_config(&conn, "active_repo", active.as_deref().unwrap_or(""));
}

/// Add `path` to the ordered open set, enforcing the cap. `Ok(true)` = newly added,
/// `Ok(false)` = already open (no-op), `Err` = at capacity. Pure, so it's unit-tested.
fn workspace_add(open: &mut Vec<String>, path: &str) -> Result<bool, String> {
    if open.iter().any(|p| p == path) {
        return Ok(false);
    }
    if open.len() >= MAX_PROJECTS {
        return Err(format!(
            "You can open up to {MAX_PROJECTS} projects at once. Close one first."
        ));
    }
    open.push(path.to_string());
    Ok(true)
}

/// Remove `path` from the open set; if it was the active project, reassign active to the
/// first remaining project (or `None`). Pure, so it's unit-tested.
fn workspace_remove(open: &mut Vec<String>, active: &mut Option<String>, path: &str) {
    open.retain(|p| p != path);
    if active.as_deref() == Some(path) {
        *active = open.first().cloned();
    }
}

/// Add `path` to the open set (enforcing the cap), cache it, start watching it, mark it
/// active, and persist the workspace. A no-op-then-activate if it's already open. Shared by
/// the `open_repository` command and workspace restore.
fn open_internal(app: &tauri::AppHandle, state: &AppState, path: &str) -> Result<(), String> {
    {
        let mut open = state.open_repos.lock().unwrap();
        workspace_add(&mut open, path)?;
    }

    // Cache commits for instant loads (best-effort).
    {
        let conn = state.db.lock().unwrap();
        if let Ok(repo_id) = db::cache::upsert_repo(&conn, path) {
            if let Ok(commits) = engine::git::list_commits(path, 200) {
                let _ = db::cache::cache_commits(&conn, repo_id, &commits);
            }
        }
    }

    // Start a live watcher (only if not already watching this path).
    {
        let mut watchers = state.watchers.lock().unwrap();
        if !watchers.contains_key(path) {
            if let Some(w) = engine::watcher::watch(app.clone(), path.to_string()) {
                watchers.insert(path.to_string(), w);
            }
        }
    }

    *state.active_repo.lock().unwrap() = Some(path.to_string());
    persist_workspace(state);
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Open a project: validate it's a repo, add it to the workspace (up to [`MAX_PROJECTS`]),
/// cache it, start a live watcher, and mark it active. Returns `Err` if the cap is reached.
#[tauri::command]
#[specta::specta]
fn open_repository(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<RepoSummary, String> {
    let summary = engine::git::summarize(&path).map_err(|e| e.to_string())?;
    open_internal(&app, &state, &path)?;
    Ok(summary)
}

/// The open projects with their live per-project sync state — powers the sidebar cards.
/// Paths that no longer resolve as repos are skipped.
#[tauri::command]
#[specta::specta]
fn list_open_repos(state: tauri::State<'_, AppState>) -> Result<Vec<ProjectStatus>, String> {
    let open = state.open_repos.lock().unwrap().clone();
    let active = state.active_repo.lock().unwrap().clone();

    let mut out = Vec::with_capacity(open.len());
    for path in open {
        let Ok(summary) = engine::git::summarize(&path) else {
            continue;
        };
        let (ahead, behind, has_conflicts) = engine::git::status(&path)
            .map(|s| (s.ahead, s.behind, s.has_conflicts))
            .unwrap_or((0, 0, false));
        let fetch = state
            .fetch
            .lock()
            .unwrap()
            .get(&path)
            .cloned()
            .unwrap_or_default();
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&path)
            .to_string();

        out.push(ProjectStatus {
            active: active.as_deref() == Some(path.as_str()),
            path,
            name,
            head_branch: summary.head_branch,
            commit_count: summary.commit_count,
            branch_count: summary.branch_count,
            ahead,
            behind,
            has_conflicts,
            fetch,
        });
    }
    Ok(out)
}

/// Switch which open project the main views focus on.
#[tauri::command]
#[specta::specta]
fn set_active_repo(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let is_open = state.open_repos.lock().unwrap().iter().any(|p| *p == path);
    if !is_open {
        return Err("That project is not open.".to_string());
    }
    *state.active_repo.lock().unwrap() = Some(path);
    persist_workspace(&state);
    Ok(())
}

/// Close a project: remove it from the workspace, stop its watcher, and — if it was the
/// active one — fall back to another open project (or none).
#[tauri::command]
#[specta::specta]
fn close_repository(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    {
        let mut open = state.open_repos.lock().unwrap();
        let mut active = state.active_repo.lock().unwrap();
        workspace_remove(&mut open, &mut active, &path);
    }
    // Dropping the watcher handle stops the OS watch.
    state.watchers.lock().unwrap().remove(&path);
    state.fetch.lock().unwrap().remove(&path);
    persist_workspace(&state);
    Ok(())
}

/// Path of the currently active project, if any.
#[tauri::command]
#[specta::specta]
fn current_repo(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.active_repo.lock().unwrap().clone())
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

/// The active repo's GitHub web base URL (`https://github.com/owner/repo`), if its `origin`
/// remote is a GitHub URL — used to deep-link commits/contributors to github.com.
#[tauri::command]
#[specta::specta]
fn github_repo_url(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let path = require_repo(&state)?;
    let url = engine::git::remote_url(&path, "origin").ok().flatten();
    Ok(url
        .and_then(|u| engine::remote::parse_github_slug(&u))
        .map(|(owner, repo)| format!("https://github.com/{owner}/{repo}")))
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

/// Resolve a conflict by taking one whole side (`resolution` = `"ours"`/`"theirs"`).
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

/// Save a hand-edited merge resolution: write `content` to the file and stage it.
#[tauri::command]
#[specta::specta]
fn save_conflict_resolution(
    state: tauri::State<'_, AppState>,
    file: String,
    content: String,
) -> Result<(), String> {
    let path = require_repo(&state)?;
    engine::conflicts::save_resolution(&path, &file, &content).map_err(|e| e.to_string())
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

/// Per-reviewer review-throughput metrics for the active repo (via the GitHub `gh` CLI).
#[tauri::command]
#[specta::specta]
fn review_stats(state: tauri::State<'_, AppState>) -> Result<Vec<ReviewerStat>, String> {
    let path = require_repo(&state)?;
    Ok(engine::reviews::review_stats(&path))
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

/// Background-fetch status for a specific project's sync indicator.
#[tauri::command]
#[specta::specta]
fn fetch_status(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<FetchStatus, String> {
    Ok(state.fetch.lock().unwrap().get(&path).cloned().unwrap_or_default())
}

/// Trigger a remote fetch for a specific project now (the "Fetch" button). Non-destructive:
/// updates remote-tracking refs only. Returns that project's resulting status.
#[tauri::command]
#[specta::specta]
fn fetch_now(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<FetchStatus, String> {
    run_fetch(&app, &state, &path);
    Ok(state.fetch.lock().unwrap().get(&path).cloned().unwrap_or_default())
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
        list_open_repos,
        set_active_repo,
        close_repository,
        current_repo,
        list_commits,
        github_repo_url,
        list_branches,
        get_status,
        list_conflicts,
        resolve_conflict,
        save_conflict_resolution,
        contributor_stats,
        review_stats,
        get_ci_status,
        fetch_status,
        fetch_now,
        get_config,
        set_config,
    ])
}

/// Stable id so the tray can be looked up and its tooltip updated after creation.
const TRAY_ID: &str = "hud-tray";

/// Build the OS tray HUD (Trigger layer): quick access + quit.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open_item = MenuItem::with_id(app, "open", "Open git-hud", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
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

/// Background remote-fetch worker (Trigger layer). Every interval, if a repo is open and
/// the user hasn't disabled auto-fetch (config `auto_fetch = "off"`), runs a non-destructive
/// `git fetch` so commits pushed remotely (e.g. on the web) land in the local clone and
/// surface as `behind: N` + timeline commits — without the user pulling manually.
fn spawn_auto_fetch(handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(120));
        ticker.tick().await; // consume the immediate first tick
        loop {
            ticker.tick().await;

            let state = handle.state::<AppState>();
            let disabled = {
                let conn = state.db.lock().unwrap();
                db::cache::get_config(&conn, "auto_fetch").ok().flatten().as_deref() == Some("off")
            };
            if disabled {
                continue;
            }

            // Fetch every open project (sequential is fine for <= MAX_PROJECTS).
            let repos = state.open_repos.lock().unwrap().clone();
            for path in repos {
                let app = handle.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let state = app.state::<AppState>();
                    run_fetch(&app, &state, &path);
                })
                .await;
            }
        }
    });
}

/// Fire an OS notification (best-effort; the `notification:default` capability is granted).
fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Background alerts worker (Trigger layer): each cycle, per open project, diff conflict /
/// failing-pipeline / review-request state against the previous cycle and fire an OS
/// notification for each *new* item, then roll the totals into the tray tooltip.
fn spawn_alerts(handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(120));
        ticker.tick().await; // skip the immediate first tick
        loop {
            ticker.tick().await;
            let app = handle.clone();
            // git2 + gh are blocking — run the pass off the async runtime.
            let _ = tokio::task::spawn_blocking(move || run_alerts(&app)).await;
        }
    });
}

/// One alerts pass. Side effects only: notifications + the tray tooltip rollup.
fn run_alerts(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let repos = state.open_repos.lock().unwrap().clone();

    let (mut n_conflicts, mut n_failing, mut n_reviews) = (0usize, 0usize, 0usize);

    for path in repos {
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&path)
            .to_string();

        let has_conflicts = engine::git::status(&path)
            .map(|s| s.has_conflicts)
            .unwrap_or(false);
        let failing_now: HashSet<String> = engine::remote::poll_ci(&path)
            .into_iter()
            .filter(|c| c.status == "failed")
            .map(|c| c.pipeline)
            .collect();
        let requests = engine::reviews::review_requests(&path);

        if has_conflicts {
            n_conflicts += 1;
        }
        n_failing += failing_now.len();
        n_reviews += requests.len();

        // Diff against last cycle, notify on new items, then record the new state.
        let mut alerts = state.alerts.lock().unwrap();
        let prev = alerts.entry(path.clone()).or_default();

        if has_conflicts && !prev.had_conflicts {
            notify(app, "Merge conflict", &format!("{name} has merge conflicts"));
        }
        for pipeline in failing_now.difference(&prev.failing) {
            notify(app, "Pipeline failed", &format!("{name}: {pipeline} is failing"));
        }
        for req in &requests {
            if !prev.seen_reviews.contains(&req.number) {
                notify(
                    app,
                    "Review requested",
                    &format!("{name} #{}: {}", req.number, req.title),
                );
            }
        }

        prev.had_conflicts = has_conflicts;
        prev.failing = failing_now;
        prev.seen_reviews = requests.iter().map(|r| r.number).collect();
    }

    // Roll the totals into the tray tooltip.
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let mut parts: Vec<String> = Vec::new();
        if n_conflicts > 0 {
            parts.push(format!("{n_conflicts} conflicts"));
        }
        if n_failing > 0 {
            parts.push(format!("{n_failing} failing"));
        }
        if n_reviews > 0 {
            parts.push(format!("{n_reviews} reviews"));
        }
        let tip = if parts.is_empty() {
            "git-hud — all clear".to_string()
        } else {
            format!("git-hud — {}", parts.join(" · "))
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

/// Re-open the projects that were open last session (up to [`MAX_PROJECTS`]), restoring the
/// saved active project. Paths that no longer resolve as repos are silently pruned.
fn restore_workspace(handle: &tauri::AppHandle) {
    let state = handle.state::<AppState>();
    let (open_csv, saved_active) = {
        let conn = state.db.lock().unwrap();
        let mut open_csv = db::cache::get_config(&conn, "open_repos")
            .ok()
            .flatten()
            .unwrap_or_default();
        // One-time migration: seed the workspace from the pre-multi-project `last_repo`.
        if open_csv.is_empty() {
            if let Some(last) = db::cache::get_config(&conn, "last_repo").ok().flatten() {
                open_csv = last;
            }
        }
        let active = db::cache::get_config(&conn, "active_repo")
            .ok()
            .flatten()
            .unwrap_or_default();
        (open_csv, active)
    };

    for path in open_csv
        .split('\n')
        .filter(|s| !s.is_empty())
        .take(MAX_PROJECTS)
    {
        if engine::git::summarize(path).is_ok() {
            let _ = open_internal(handle, &state, path);
        }
    }

    // Prefer the saved active project if it survived the prune.
    if !saved_active.is_empty()
        && state.open_repos.lock().unwrap().iter().any(|p| *p == saved_active)
    {
        *state.active_repo.lock().unwrap() = Some(saved_active);
    }
    persist_workspace(&state);
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

            // Re-open last session's projects before the workers start.
            restore_workspace(app.handle());

            // Ambient triggers.
            build_tray(app)?;
            spawn_ci_poller(app.handle().clone());
            spawn_auto_fetch(app.handle().clone());
            spawn_alerts(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{workspace_add, workspace_remove, MAX_PROJECTS};

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

    #[test]
    fn open_set_caps_at_max_projects() {
        let mut open = Vec::new();
        for i in 0..MAX_PROJECTS {
            assert_eq!(workspace_add(&mut open, &format!("/p{i}")), Ok(true));
        }
        assert_eq!(open.len(), MAX_PROJECTS);
        // The 8th distinct project is rejected.
        assert!(workspace_add(&mut open, "/overflow").is_err());
        assert_eq!(open.len(), MAX_PROJECTS);
    }

    #[test]
    fn re_opening_dedupes_without_error() {
        let mut open = vec!["/a".to_string(), "/b".to_string()];
        // Already open → Ok(false), no duplicate, and not counted against the cap.
        assert_eq!(workspace_add(&mut open, "/a"), Ok(false));
        assert_eq!(open, vec!["/a".to_string(), "/b".to_string()]);
    }

    #[test]
    fn closing_active_reassigns_to_first_remaining() {
        let mut open = vec!["/a".to_string(), "/b".to_string(), "/c".to_string()];
        let mut active = Some("/b".to_string());
        workspace_remove(&mut open, &mut active, "/b");
        assert_eq!(open, vec!["/a".to_string(), "/c".to_string()]);
        assert_eq!(active, Some("/a".to_string())); // fell back to first remaining
    }

    #[test]
    fn closing_last_project_clears_active() {
        let mut open = vec!["/only".to_string()];
        let mut active = Some("/only".to_string());
        workspace_remove(&mut open, &mut active, "/only");
        assert!(open.is_empty());
        assert_eq!(active, None);
    }

    #[test]
    fn closing_inactive_leaves_active_untouched() {
        let mut open = vec!["/a".to_string(), "/b".to_string()];
        let mut active = Some("/a".to_string());
        workspace_remove(&mut open, &mut active, "/b");
        assert_eq!(active, Some("/a".to_string()));
    }
}
