import {
  Activity,
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  FolderOpen,
  GitBranch,
  GitMerge,
  Settings as SettingsIcon,
  Users,
  X,
} from "lucide-react";

import type { View } from "../lib/views";
import type { ProjectStatus } from "../services/tauri-ipc";

const MAX_PROJECTS = 7;

const NAV: { id: View; label: string; icon: typeof GitBranch; hint: string }[] = [
  { id: "timeline", label: "Timeline", icon: GitBranch, hint: "Commit & branch graph" },
  { id: "merge", label: "Conflicts", icon: GitMerge, hint: "Merge conflict editor" },
  { id: "dashboard", label: "Collaboration", icon: Users, hint: "Who did what" },
  { id: "settings", label: "Settings", icon: SettingsIcon, hint: "Aliases & filters" },
];

export function Sidebar({
  view,
  onNavigate,
  projects,
  activePath,
  onOpenProject,
  onCloseProject,
  onSelectProject,
}: {
  view: View;
  onNavigate: (view: View) => void;
  projects: ProjectStatus[];
  activePath: string | null;
  onOpenProject: () => void;
  onCloseProject: (path: string) => void;
  onSelectProject: (path: string) => void;
}) {
  const atCap = projects.length >= MAX_PROJECTS;

  return (
    <aside className="flex h-full w-60 flex-col border-r border-zinc-800 bg-zinc-900/60">
      <div className="flex items-center gap-2 px-4 py-4">
        <Activity className="h-5 w-5 text-emerald-400" />
        <span className="text-sm font-semibold tracking-wide">
          git<span className="text-emerald-400">-hud</span>
        </span>
        <span className="ml-auto text-[10px] text-zinc-500">v0.1.0</span>
      </div>

      <div className="flex items-center justify-between px-4 pb-1">
        <span className="text-[10px] font-medium uppercase tracking-wide text-zinc-500">
          Projects
        </span>
        <span className="text-[10px] text-zinc-600">
          {projects.length}/{MAX_PROJECTS}
        </span>
      </div>

      <div className="mx-3 mb-3 max-h-64 space-y-1.5 overflow-auto">
        {projects.length === 0 ? (
          <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-3 text-xs text-zinc-400">
            No projects open
          </div>
        ) : (
          projects.map((p) => (
            <ProjectCard
              key={p.path}
              project={p}
              active={p.path === activePath}
              onSelect={() => onSelectProject(p.path)}
              onClose={() => onCloseProject(p.path)}
            />
          ))
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
          onClick={onOpenProject}
          disabled={atCap}
          title={atCap ? `Maximum ${MAX_PROJECTS} projects — close one first` : "Open a repository"}
          className="flex w-full items-center justify-center gap-2 rounded-md bg-emerald-500 px-3 py-2 text-sm font-medium text-emerald-950 transition-colors hover:bg-emerald-400 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <FolderOpen className="h-4 w-4" />
          {atCap ? `Max ${MAX_PROJECTS} projects` : "Open repository"}
        </button>
      </div>
    </aside>
  );
}

function fetchDot(fetch: ProjectStatus["fetch"]): string {
  if (fetch.running) return "bg-amber-400";
  if (!fetch.last_ok && (fetch.last_at ?? 0) > 0) return "bg-red-400";
  if (fetch.last_at) return "bg-emerald-400";
  return "bg-zinc-600";
}

function ProjectCard({
  project,
  active,
  onSelect,
  onClose,
}: {
  project: ProjectStatus;
  active: boolean;
  onSelect: () => void;
  onClose: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      className={
        "cursor-pointer rounded-lg border bg-zinc-900 p-3 transition-colors " +
        (active
          ? "border-emerald-500/40 ring-1 ring-emerald-500/30"
          : "border-zinc-800 hover:border-zinc-700")
      }
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1 truncate text-sm font-medium text-zinc-100">
          {project.name}
        </div>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          title="Close project"
          aria-label={`Close ${project.name}`}
          className="-mr-1 -mt-0.5 shrink-0 rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="mt-1 flex items-center gap-1 text-xs text-zinc-400">
        <GitBranch className="h-3 w-3 shrink-0" />
        <span className="truncate">{project.head_branch ?? "detached"}</span>
      </div>

      <div className="mt-1.5 flex items-center gap-2 text-[11px]">
        <span className={"inline-block h-2 w-2 shrink-0 rounded-full " + fetchDot(project.fetch)} />
        {project.behind > 0 && (
          <span className="flex items-center gap-0.5 text-amber-400" title={`${project.behind} behind remote`}>
            <ArrowDown className="h-3 w-3" />
            {project.behind}
          </span>
        )}
        {project.ahead > 0 && (
          <span className="flex items-center gap-0.5 text-zinc-400" title={`${project.ahead} ahead of remote`}>
            <ArrowUp className="h-3 w-3" />
            {project.ahead}
          </span>
        )}
        {project.has_conflicts && (
          <span className="flex items-center gap-0.5 text-red-400" title="Merge conflicts">
            <AlertTriangle className="h-3 w-3" />
          </span>
        )}
        <span className="ml-auto text-zinc-600">
          {project.commit_count.toLocaleString()} · {project.branch_count}b
        </span>
      </div>
    </div>
  );
}
