import { useCallback, useEffect, useState } from "react";

import { ipc, type CommitInfo } from "../services/tauri-ipc";

/** Loads (and can refresh) the commit graph for the active repo. */
export function useCommits(repoPath: string | null, limit = 300) {
  const [commits, setCommits] = useState<CommitInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!repoPath) {
      setCommits([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setCommits(await ipc.listCommits(limit));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [repoPath, limit]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { commits, loading, error, refresh };
}
