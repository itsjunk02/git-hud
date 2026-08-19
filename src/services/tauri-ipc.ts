/**
 * Ergonomic wrapper around the auto-generated `bindings.ts` (tauri-specta).
 *
 * `bindings.ts` is regenerated on every debug run of the Rust backend — never edit it.
 * Here we (a) unwrap the tauri-specta `Result` into a value-or-throw async API, and
 * (b) expose typed helpers for the backend-emitted events.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { commands } from "./bindings";
import type {
  BranchInfo,
  CiStatus,
  CommitInfo,
  ConflictHunk,
  ContributorStat,
  FileStatus,
  RepoStatus,
  RepoSummary,
} from "./bindings";

export type {
  BranchInfo,
  CiStatus,
  CommitInfo,
  ConflictHunk,
  ContributorStat,
  FileStatus,
  RepoStatus,
  RepoSummary,
};

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: string };

/** Unwrap a tauri-specta `Result`, throwing on the error variant. */
async function unwrap<T>(promise: Promise<Result<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new Error(result.error);
  return result.data;
}

/** Value-or-throw view over every backend command. */
export const ipc = {
  openRepository: (path: string) => unwrap(commands.openRepository(path)),
  currentRepo: () => unwrap(commands.currentRepo()),
  listCommits: (limit: number) => unwrap(commands.listCommits(limit)),
  listBranches: () => unwrap(commands.listBranches()),
  getStatus: () => unwrap(commands.getStatus()),
  listConflicts: () => unwrap(commands.listConflicts()),
  resolveConflict: (file: string, resolution: string) =>
    unwrap(commands.resolveConflict(file, resolution)),
  contributorStats: () => unwrap(commands.contributorStats()),
  getCiStatus: () => unwrap(commands.getCiStatus()),
  getConfig: (key: string) => unwrap(commands.getConfig(key)),
  setConfig: (key: string, value: string) => unwrap(commands.setConfig(key, value)),
};

/** Event names emitted by the Rust backend (see `src-tauri/src/engine/watcher.rs`). */
export const EVENTS = {
  repoChanged: "repo-changed",
  ciPoll: "ci-poll",
} as const;

/** Subscribe to the `.git` file-watcher trigger; resolves with an unlisten fn. */
export function onRepoChanged(
  handler: (path: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(EVENTS.repoChanged, (event) => handler(event.payload));
}
