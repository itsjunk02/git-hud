<div align="center">

# git-hud

**A lightweight, high-performance, cross-platform Git HUD for individuals and teams.**

Local-first · Zero-latency · Open source (MIT)

Built with **Tauri v2** · **Rust** (`git2` + SQLite) · **React 19 + TypeScript**

</div>

---

git-hud is a desktop Git GUI that treats your repositories as a live heads-up display.
A background Rust engine parses Git directly through `libgit2` (no shelling out to the
`git` CLI), caches derived metrics in local SQLite, and streams change events to a
React/Canvas frontend. 

## ✨ Features

| Hook stage | Feature | Status in this build |
| --- | --- | --- |
| **Trigger** | OS tray HUD + `.git` file-watcher + background CI/CD poll emitting ambient change events | ✅ Wired end-to-end |
| **Action** | High-performance Canvas **commit & branch timeline** (real `git2` data, DPR-aware) | ✅ Real |
| **Action** | 1-click **Merge Conflict Editor** with side-by-side split diff | ✅  Detection real; content/resolution  |
| **Reward** | **"Who Did What"** collaboration dashboard (ownership by commit attribution) |  Commit attribution real; ownership/reviews stubbed |
| **Reward** | CI/CD pipeline health + DCO/CLA compliance badges | 🔶 Stubbed poll |
| **Investment** | Repo aliases, filters & config persisted in local SQLite so the app compounds over time | ✅ Real |

> This repository is an **initialized scaffold**: the full architecture, type-safe IPC,
> and background workers are in place and compiling. The commit timeline is a complete
> vertical slice; the remaining feature screens are functional stubs wired to the same
> plumbing, ready to be filled in.

## 🏗️ Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Frontend  (src/, 100% TypeScript)                           │
│  React 19 · Vite · Tailwind v4 · Radix · lucide · Canvas     │
│                                                              │
│   components/ ── hooks/ ── services/tauri-ipc.ts             │
│                                    │                         │
│                       services/bindings.ts  ◄── generated    │
└────────────────────────────────────┼─────────────────────────┘
                                     │ type-safe IPC (tauri-specta)
┌────────────────────────────────────┼─────────────────────────┐
│  Backend  (src-tauri/, Rust)        ▼                         │
│                                                              │
│   lib.rs ── #[tauri::command] surface + tray + workers      │
│      │                                                       │
│      ├── engine/   git.rs · watcher.rs · stats.rs ·         │
│      │             conflicts.rs · remote.rs · model.rs      │
│      │                └── git2 (vendored libgit2)           │
│      └── db/       SQLite cache (rusqlite, bundled)          │
└──────────────────────────────────────────────────────────────┘
```

- **Core engine (`src-tauri/src/engine`)** — Git parsing via `git2`/libgit2 (vendored, no
  subprocess). All disk reads, the `.git` file watcher (`notify`), and the CI/CD poll run on
  background threads / the async runtime.
- **Local cache (`src-tauri/src/db`)** — `rusqlite` (bundled SQLite) with versioned
  `rusqlite_migration` schema for repos, cached commits, contributor stats, CI status, and
  user config.
- **Type-safe IPC** — `tauri-specta` generates `src/services/bindings.ts` from the Rust
  command signatures on every debug build. The frontend never hand-writes IPC types.
- **Frontend (`src`)** — React + Vite + Tailwind + Radix. The commit graph is rendered on a
  DPR-aware HTML5 Canvas for smooth performance on large histories.

## 📋 Prerequisites

- **Rust** toolchain (stable) — <https://rustup.rs>
- **Node.js** ≥ 18 and **pnpm** ≥ 8 (`npm i -g pnpm`)
- **Platform build dependencies for Tauri v2** — see
  <https://tauri.app/start/prerequisites/>:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Linux**: `webkit2gtk`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `build-essential`
  - **Windows**: Microsoft C++ Build Tools + WebView2 runtime

`git2` builds a vendored copy of libgit2 and `rusqlite` builds a bundled SQLite, so a C
compiler must be available (provided by the platform toolchains above). No system
`libgit2`/`pkg-config` is required.

## 🚀 Development

```bash
pnpm install        # install frontend dependencies
pnpm tauri dev      # launch the app with hot-reload (Rust + React)
```

The first `pnpm tauri dev` compiles the vendored libgit2, bundled SQLite, and the webview
bindings — expect a few minutes. Subsequent builds are incremental. Running in debug also
regenerates `src/services/bindings.ts` from the Rust commands.

Common scripts:

```bash
pnpm dev            # frontend only (Vite dev server on :1420)
pnpm build          # type-check (tsc) + production frontend build
pnpm tauri build    # produce a distributable desktop bundle
```

Regenerate IPC bindings / compile-check the whole backend without launching the GUI:

```bash
cd src-tauri && cargo test export_bindings
```

## 🗂️ Project structure

```
git-hud/
├── src/                          # TypeScript frontend
│   ├── components/               # Sidebar, TopBar, CommitTimeline (Canvas), MergeConflictEditor,
│   │   └── ui/                   #   CollaborationDashboard, SettingsPanel, CommitDetail, ui/
│   ├── hooks/                    # useRepository, useCommits, useTauriEvent
│   ├── lib/                      # shared view types
│   ├── services/
│   │   ├── tauri-ipc.ts          # ergonomic wrapper over the generated bindings + events
│   │   └── bindings.ts           # AUTO-GENERATED by tauri-specta (do not edit)
│   ├── App.tsx · main.tsx · index.css
│
└── src-tauri/                    # Rust backend
    ├── src/
    │   ├── lib.rs                # commands, tauri-specta builder, tray, background workers
    │   ├── error.rs              # AppError (thiserror)
    │   ├── engine/               # git.rs, watcher.rs, stats.rs, conflicts.rs, remote.rs, model.rs
    │   └── db/                   # mod.rs, migrations.rs, cache.rs
    ├── capabilities/default.json # window permissions (core, event, dialog, notification)
    ├── tauri.conf.json · build.rs · Cargo.toml
```

## 🔌 IPC command surface

Defined in `src-tauri/src/lib.rs`, consumed type-safely via `src/services/tauri-ipc.ts`:

`open_repository` · `current_repo` · `list_commits` · `list_branches` · `get_status` ·
`list_conflicts` · `resolve_conflict` · `contributor_stats` · `get_ci_status` ·
`get_config` · `set_config`

Backend → frontend events: `repo-changed` (file watcher), `ci-poll` (CI heartbeat).

## 🧭 Roadmap (filling in the stubs)

- Three-way blob extraction + write-back for the Merge Conflict Editor
- Blame-based ownership heatmaps and review-throughput metrics
- Real forge integration (GitHub/GitLab) for CI/CD status and DCO/CLA verification
- Push/pull/commit actions from the branch timeline

## 📄 License

Released under the [MIT License](./LICENSE). © 2026 git-hud contributors.
