import { useCallback, useEffect, useState } from "react";
import { ArrowDown, ArrowUp, RefreshCw } from "lucide-react";

import { useTauriEvent } from "../hooks/useTauriEvent";
import {
  EVENTS,
  ipc,
  type FetchStatus,
  type RepoStatus,
} from "../services/tauri-ipc";

function ago(epochSeconds: number | null | undefined): string {
  if (!epochSeconds) return "not yet";
  const secs = Math.max(0, Math.floor(Date.now() / 1000 - epochSeconds));
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

/**
 * Remote sync indicator: how far the local branch is ahead/behind its remote, plus the
 * background-fetch state, with a manual "Fetch now" trigger. `behind` is how commits
 * pushed on the web surface locally once the background fetch pulls the refs down.
 */
export function SyncStatus({ repoPath }: { repoPath: string | null }) {
  const [status, setStatus] = useState<RepoStatus | null>(null);
  const [fetch, setFetch] = useState<FetchStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    if (!repoPath) return;
    ipc.getStatus().then(setStatus).catch(() => setStatus(null));
    ipc.fetchStatus(repoPath).then(setFetch).catch(() => setFetch(null));
  }, [repoPath]);

  useEffect(() => {
    load();
  }, [load]);

  // A background fetch emits repo-changed, so this refreshes when new refs land.
  useTauriEvent<string>(EVENTS.repoChanged, () => load());

  const doFetch = useCallback(async () => {
    if (!repoPath) return;
    setBusy(true);
    try {
      setFetch(await ipc.fetchNow(repoPath));
    } catch {
      /* surfaced via fetch.last_ok on next load */
    } finally {
      setBusy(false);
      load();
    }
  }, [repoPath, load]);

  if (!repoPath) return null;

  const ahead = status?.ahead ?? 0;
  const behind = status?.behind ?? 0;
  const running = busy || fetch?.running;
  const failed = fetch != null && !fetch.last_ok && (fetch.last_at ?? 0) > 0;

  const dot = running
    ? "bg-amber-400"
    : failed
      ? "bg-red-400"
      : fetch?.last_at
        ? "bg-emerald-400"
        : "bg-zinc-600";
  const fetchLabel = running
    ? "fetching…"
    : failed
      ? "fetch failed"
      : `synced ${ago(fetch?.last_at)}`;

  return (
    <div className="flex items-center gap-3 text-xs">
      {(behind > 0 || ahead > 0) && (
        <div className="flex items-center gap-2">
          {behind > 0 && (
            <span
              className="flex items-center gap-0.5 text-amber-400"
              title={`${behind} commit(s) on the remote you don't have yet — pull to merge them`}
            >
              <ArrowDown className="h-3.5 w-3.5" />
              {behind}
            </span>
          )}
          {ahead > 0 && (
            <span
              className="flex items-center gap-0.5 text-zinc-300"
              title={`${ahead} local commit(s) not yet pushed`}
            >
              <ArrowUp className="h-3.5 w-3.5" />
              {ahead}
            </span>
          )}
        </div>
      )}

      <span
        className="flex items-center gap-1.5 text-zinc-500"
        title={failed ? fetch?.message || "fetch failed" : "Last remote fetch"}
      >
        <span className={"inline-block h-2 w-2 rounded-full " + dot} />
        {fetchLabel}
      </span>

      <button
        onClick={doFetch}
        disabled={running}
        className="flex items-center gap-1.5 rounded-md border border-zinc-700 px-2.5 py-1.5 text-zinc-300 hover:bg-zinc-800 disabled:opacity-50"
        title="Fetch remote changes now"
      >
        <RefreshCw className={"h-3.5 w-3.5 " + (running ? "animate-spin" : "")} />
        Fetch
      </button>
    </div>
  );
}
