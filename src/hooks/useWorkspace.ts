import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { EVENTS, ipc, type ProjectStatus } from "../services/tauri-ipc";
import { useTauriEvent } from "./useTauriEvent";

const MAX_PROJECTS = 7;

/**
 * Owns the multi-project workspace: the open projects (up to 7, restored by the backend on
 * launch), which one is active, and open/close/switch. Every open project stays live, so
 * the list refreshes on any `repo-changed` event to keep each card's sync state current.
 */
export function useWorkspace() {
  const [projects, setProjects] = useState<ProjectStatus[]>([]);
  const [error, setError] = useState<string | null>(null);

  const activePath = projects.find((p) => p.active)?.path ?? null;

  const refresh = useCallback(async () => {
    try {
      setProjects(await ipc.listOpenRepos());
    } catch {
      setProjects([]);
    }
  }, []);

  // The backend re-opens last session's projects during setup; read them on mount.
  useEffect(() => {
    void refresh();
  }, [refresh]);

  // All projects are live: refresh the whole list when any project changes on disk (a
  // commit, a background fetch, a pull) so per-card behind/fetch stays current.
  useTauriEvent<string>(EVENTS.repoChanged, () => void refresh());

  const openProject = useCallback(async () => {
    if (projects.length >= MAX_PROJECTS) {
      setError(`You can open up to ${MAX_PROJECTS} projects at once. Close one first.`);
      return;
    }
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Open a Git repository",
    });
    if (typeof selected !== "string") return;
    setError(null);
    try {
      await ipc.openRepository(selected); // adds + activates it backend-side
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [projects.length, refresh]);

  const closeProject = useCallback(
    async (path: string) => {
      await ipc.closeRepository(path).catch(() => undefined);
      await refresh();
    },
    [refresh],
  );

  const setActive = useCallback(
    async (path: string) => {
      await ipc.setActiveRepo(path).catch(() => undefined);
      await refresh();
    },
    [refresh],
  );

  return { projects, activePath, error, openProject, closeProject, setActive };
}
