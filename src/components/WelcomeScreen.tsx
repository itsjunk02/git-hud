import { Bell, FolderOpen, GitMerge, Users } from "lucide-react";

const FEATURES = [
  { icon: Bell, title: "Ambient triggers", body: "Tray HUD alerts for conflicts, pipelines, and reviews." },
  { icon: GitMerge, title: "1-click actions", body: "Visual merge resolution and branch operations." },
  { icon: Users, title: "Real ownership", body: "Who-did-what attribution over raw line counts." },
];

/** Shown when no repository is open — the primary call to action. */
export function WelcomeScreen({
  onOpenRepo,
  error,
}: {
  onOpenRepo: () => void;
  error: string | null;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <div className="mb-2 text-3xl font-semibold tracking-tight">
        git<span className="text-emerald-400">-hud</span>
      </div>
      <p className="mb-8 max-w-md text-sm text-zinc-400">
        A high-performance, local-first Git HUD. Open a repository to see its commit graph,
        conflicts, and collaboration metrics.
      </p>

      <button
        onClick={onOpenRepo}
        className="flex items-center gap-2 rounded-lg bg-emerald-500 px-5 py-2.5 text-sm font-medium text-emerald-950 transition-colors hover:bg-emerald-400"
      >
        <FolderOpen className="h-4 w-4" />
        Open a repository
      </button>

      {error && <p className="mt-4 text-xs text-red-400">{error}</p>}

      <div className="mt-12 grid max-w-2xl grid-cols-1 gap-4 sm:grid-cols-3">
        {FEATURES.map(({ icon: Icon, title, body }) => (
          <div key={title} className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4 text-left">
            <Icon className="mb-2 h-5 w-5 text-emerald-400" />
            <div className="text-sm font-medium text-zinc-100">{title}</div>
            <div className="mt-1 text-xs text-zinc-500">{body}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
