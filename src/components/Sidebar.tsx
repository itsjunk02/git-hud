import {
  Activity,
  FolderOpen,
  GitBranch,
  GitMerge,
  Settings as SettingsIcon,
  Users,
} from "lucide-react";

import type { View } from "../lib/views";
import type { RepoSummary } from "../services/tauri-ipc";

const NAV: { id: View; label: string; icon: typeof GitBranch; hint: string }[] = [
  { id: "timeline", label: "Timeline", icon: GitBranch, hint: "Commit & branch graph" },
  { id: "merge", label: "Conflicts", icon: GitMerge, hint: "Merge conflict editor" },
  { id: "dashboard", label: "Collaboration", icon: Users, hint: "Who did what" },
  { id: "settings", label: "Settings", icon: SettingsIcon, hint: "Aliases & filters" },
];

function repoName(path: string): string {
  const parts = path.replace(/[/\\]+$/, "").split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

export function Sidebar({
  view,
  onNavigate,
  repo,
  onOpenRepo,
}: {
  view: View;
  onNavigate: (view: View) => void;
  repo: RepoSummary | null;
  onOpenRepo: () => void;
}) {
  return (
    <aside className="flex h-full w-60 flex-col border-r border-zinc-800 bg-zinc-900/60">
      <div className="flex items-center gap-2 px-4 py-4">
        <Activity className="h-5 w-5 text-emerald-400" />
        <span className="text-sm font-semibold tracking-wide">
          git<span className="text-emerald-400">-hud</span>
        </span>
        <span className="ml-auto text-[10px] text-zinc-500">v0.1.0</span>
      </div>

      <div className="mx-3 mb-3 rounded-lg border border-zinc-800 bg-zinc-900 p-3">
        {repo ? (
          <>
            <div className="truncate text-sm font-medium text-zinc-100">
              {repoName(repo.path)}
            </div>
            <div className="mt-1 flex items-center gap-1 text-xs text-zinc-400">
              <GitBranch className="h-3 w-3" />
              <span className="truncate">{repo.head_branch ?? "detached"}</span>
            </div>
            <div className="mt-1 text-[11px] text-zinc-500">
              {repo.commit_count.toLocaleString()} commits · {repo.branch_count} branches
            </div>
          </>
        ) : (
          <div className="text-xs text-zinc-400">No repository open</div>
        )}
      </div>

      <nav className="flex flex-1 flex-col gap-1 px-3">
        {NAV.map(({ id, label, icon: Icon, hint }) => {
          const active = view === id;
          return (
            <button
              key={id}
              onClick={() => onNavigate(id)}
              title={hint}
              className={
                "flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors " +
                (active
                  ? "bg-emerald-500/15 text-emerald-300 ring-1 ring-emerald-500/30"
                  : "text-zinc-300 hover:bg-zinc-800/70")
              }
            >
              <Icon className="h-4 w-4" />
              {label}
            </button>
          );
        })}
      </nav>

      <div className="p-3">
        <button
          onClick={onOpenRepo}
          className="flex w-full items-center justify-center gap-2 rounded-md bg-emerald-500 px-3 py-2 text-sm font-medium text-emerald-950 transition-colors hover:bg-emerald-400"
        >
          <FolderOpen className="h-4 w-4" />
          Open repository
        </button>
      </div>
    </aside>
  );
}
