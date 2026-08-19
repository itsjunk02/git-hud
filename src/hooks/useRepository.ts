import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { ipc, type RepoSummary } from "../services/tauri-ipc";

const LAST_REPO_KEY = "last_repo";

/**
 * Owns the active repository. Persists the last-opened path in the SQLite-backed config
 * (the Investment layer) so the app reopens it on next launch.
 */
export function useRepository() {
  const [repo, setRepo] = useState<RepoSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const openPath = useCallback(async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const summary = await ipc.openRepository(path);
      setRepo(summary);
      await ipc.setConfig(LAST_REPO_KEY, path).catch(() => undefined);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const pickAndOpen = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Open a Git repository",
    });
    if (typeof selected === "string") await openPath(selected);
  }, [openPath]);

  // Restore the last-opened repo once, on mount.
  useEffect(() => {
    (async () => {
      try {
        const last = await ipc.getConfig(LAST_REPO_KEY);
        if (last) await openPath(last);
      } catch {
        /* first run / no config yet — ignore */
      }
    })();
  }, [openPath]);

  return { repo, loading, error, openPath, pickAndOpen };
}
