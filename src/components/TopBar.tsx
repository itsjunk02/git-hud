import { RefreshCw, SquareTerminal } from "lucide-react";

import { SyncStatus } from "./SyncStatus";

/** Header for the active view: title, repo path, live indicator, refresh, and terminal toggle. */
export function TopBar({
  title,
  subtitle,
  repoPath,
  live,
  onRefresh,
  terminalOpen,
  onToggleTerminal,
}: {
  title: string;
  subtitle?: string;
  repoPath: string | null;
  live: boolean;
  onRefresh?: () => void;
  terminalOpen?: boolean;
  onToggleTerminal?: () => void;
}) {
  return (
    <header className="flex items-center gap-3 border-b border-zinc-800 px-5 py-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <h1 className="text-sm font-semibold text-zinc-100">{title}</h1>
          <span
            className={
              "inline-block h-2 w-2 rounded-full " +
              (live ? "bg-emerald-400 shadow-[0_0_6px] shadow-emerald-400/70" : "bg-zinc-600")
            }
            title={live ? "Watching repository for changes" : "Not watching"}
          />
        </div>
        <p className="truncate text-xs text-zinc-500">{subtitle ?? repoPath ?? "No repository"}</p>
      </div>

      <div className="ml-auto flex items-center gap-3">
        <SyncStatus repoPath={repoPath} />
        {onRefresh && (
          <button
            onClick={onRefresh}
            className="flex items-center gap-1.5 rounded-md border border-zinc-700 px-2.5 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800"
          >
            <RefreshCw className="h-3.5 w-3.5" />
            Refresh
          </button>
        )}
        {onToggleTerminal && (
          <button
            onClick={onToggleTerminal}
            title="Toggle terminal (Ctrl+`)"
            aria-label="Toggle terminal"
            className={
              "flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs transition-colors " +
              (terminalOpen
                ? "border-emerald-500/40 bg-emerald-500/15 text-emerald-300"
                : "border-zinc-700 text-zinc-300 hover:bg-zinc-800")
            }
          >
            <SquareTerminal className="h-3.5 w-3.5" />
            Terminal
          </button>
        )}
      </div>
    </header>
  );
}
