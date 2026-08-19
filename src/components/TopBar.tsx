import { RefreshCw } from "lucide-react";

/** Header for the active view: title, repo path, live indicator, and refresh. */
export function TopBar({
  title,
  subtitle,
  repoPath,
  live,
  onRefresh,
}: {
  title: string;
  subtitle?: string;
  repoPath: string | null;
  live: boolean;
  onRefresh?: () => void;
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

      {onRefresh && (
        <button
          onClick={onRefresh}
          className="ml-auto flex items-center gap-1.5 rounded-md border border-zinc-700 px-2.5 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Refresh
        </button>
      )}
    </header>
  );
}
