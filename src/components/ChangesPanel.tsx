import { AlertTriangle, GitCommit, Minus, Plus } from "lucide-react";
import { useCallback, useEffect, useState, type ComponentType, type ReactNode } from "react";

import { useTauriEvent } from "../hooks/useTauriEvent";
import { EVENTS, ipc, type FileStatus } from "../services/tauri-ipc";

/** Accent colour per status label, matching the design system. */
const STATUS_COLOR: Record<string, string> = {
  new: "text-emerald-400",
  modified: "text-amber-400",
  deleted: "text-red-400",
  renamed: "text-blue-400",
  typechange: "text-purple-400",
};

/** Single-letter badge (git short status). */
const STATUS_LETTER: Record<string, string> = {
  new: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  typechange: "T",
};

/**
 * Stage / unstage / commit surface for the active repo. Reads the working-tree status via the
 * existing `get_status`, splitting each file into its staged (index) and unstaged (worktree)
 * sides so a file edited after staging appears in both lists. Writes go through the thin
 * stage/unstage/commit commands; the backend's post-write reconcile fires `repo-changed`, which
 * refreshes this panel (and the Timeline) automatically.
 */
export function ChangesPanel({ repoPath }: { repoPath: string | null }) {
  const [files, setFiles] = useState<FileStatus[]>([]);
  const [message, setMessage] = useState("");
  const [amend, setAmend] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!repoPath) {
      setFiles([]);
      return;
    }
    try {
      setFiles((await ipc.getStatus()).files);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [repoPath]);

  useEffect(() => {
    void load();
  }, [load]);

  // Refresh when anything (including our own writes) changes the repo.
  useTauriEvent<string>(EVENTS.repoChanged, () => void load());

  const run = useCallback(
    async (fn: () => Promise<unknown>) => {
      setBusy(true);
      setError(null);
      try {
        await fn();
        await load();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [load],
  );

  const conflicted = files.filter((f) => f.conflicted);
  const staged = files.filter((f) => !f.conflicted && f.index_status !== "unmodified");
  const unstaged = files.filter((f) => !f.conflicted && f.worktree_status !== "unmodified");

  const stage = (paths: string[]) => run(() => ipc.stagePaths(paths));
  const unstage = (paths: string[]) => run(() => ipc.unstagePaths(paths));
  const doCommit = () =>
    run(async () => {
      await ipc.commit(message.trim(), amend);
      setMessage("");
      setAmend(false);
    });

  // Normal commit needs staged files + a message; amend can re-commit with neither.
  const canCommit =
    !busy && (amend || (staged.length > 0 && message.trim().length > 0));

  if (!repoPath) return null;

  return (
    <div className="flex h-full flex-col">
      {error && (
        <div className="m-3 rounded-md border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-300">
          {error}
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4">
        {conflicted.length > 0 && (
          <Section title="Conflicts" count={conflicted.length} icon={AlertTriangle} accent="text-red-400">
            {conflicted.map((f) => (
              <Row key={f.path} path={f.path} label="conflicted" letter="!" color="text-red-400" />
            ))}
            <p className="px-2 text-[11px] text-zinc-500">
              Resolve these in the Conflicts tab before committing.
            </p>
          </Section>
        )}

        <Section
          title="Staged"
          count={staged.length}
          action={
            staged.length
              ? { label: "Unstage all", onClick: () => unstage(staged.map((f) => f.path)) }
              : undefined
          }
        >
          {staged.length === 0 ? (
            <Empty text="Nothing staged" />
          ) : (
            staged.map((f) => (
              <Row
                key={f.path}
                path={f.path}
                label={f.index_status}
                letter={STATUS_LETTER[f.index_status] ?? "?"}
                color={STATUS_COLOR[f.index_status] ?? "text-zinc-400"}
                button={{ icon: Minus, title: "Unstage", onClick: () => unstage([f.path]) }}
              />
            ))
          )}
        </Section>

        <Section
          title="Changes"
          count={unstaged.length}
          action={
            unstaged.length
              ? { label: "Stage all", onClick: () => stage(unstaged.map((f) => f.path)) }
              : undefined
          }
        >
          {unstaged.length === 0 ? (
            <Empty text="No unstaged changes" />
          ) : (
            unstaged.map((f) => (
              <Row
                key={f.path}
                path={f.path}
                label={f.worktree_status}
                letter={STATUS_LETTER[f.worktree_status] ?? "?"}
                color={STATUS_COLOR[f.worktree_status] ?? "text-zinc-400"}
                button={{ icon: Plus, title: "Stage", onClick: () => stage([f.path]) }}
              />
            ))
          )}
        </Section>
      </div>

      <div className="border-t border-zinc-800 p-3">
        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder={amend ? "Amend message (blank keeps the current one)" : "Commit message"}
          rows={3}
          className="w-full resize-none rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-emerald-500/50 focus:outline-none"
        />
        <div className="mt-2 flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-xs text-zinc-400">
            <input
              type="checkbox"
              checked={amend}
              onChange={(e) => setAmend(e.target.checked)}
              className="accent-emerald-500"
            />
            Amend last commit
          </label>
          <button
            onClick={doCommit}
            disabled={!canCommit}
            className="ml-auto flex items-center gap-1.5 rounded-md bg-emerald-500 px-3 py-1.5 text-sm font-medium text-emerald-950 transition-colors hover:bg-emerald-400 disabled:cursor-not-allowed disabled:opacity-40"
          >
            <GitCommit className="h-4 w-4" />
            {amend ? "Amend" : "Commit"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Section({
  title,
  count,
  icon: Icon,
  accent,
  action,
  children,
}: {
  title: string;
  count: number;
  icon?: ComponentType<{ className?: string }>;
  accent?: string;
  action?: { label: string; onClick: () => void };
  children: ReactNode;
}) {
  return (
    <div>
      <div className="mb-1.5 flex items-center gap-2">
        {Icon && <Icon className={"h-3.5 w-3.5 " + (accent ?? "text-zinc-400")} />}
        <span className="text-xs font-semibold uppercase tracking-wide text-zinc-400">{title}</span>
        <span className="text-[10px] text-zinc-600">{count}</span>
        {action && (
          <button
            onClick={action.onClick}
            className="ml-auto text-[11px] text-emerald-400 hover:text-emerald-300"
          >
            {action.label}
          </button>
        )}
      </div>
      <div className="space-y-0.5">{children}</div>
    </div>
  );
}

function Row({
  path,
  label,
  letter,
  color,
  button,
}: {
  path: string;
  label: string;
  letter: string;
  color: string;
  button?: { icon: ComponentType<{ className?: string }>; title: string; onClick: () => void };
}) {
  const Icon = button?.icon;
  return (
    <div className="group flex items-center gap-2 rounded px-2 py-1 hover:bg-zinc-800/50">
      <span className={"w-3 shrink-0 text-center text-xs font-bold " + color} title={label}>
        {letter}
      </span>
      <span className="min-w-0 flex-1 truncate font-mono text-xs text-zinc-300">{path}</span>
      {button && Icon && (
        <button
          onClick={button.onClick}
          title={button.title}
          className="shrink-0 rounded p-0.5 text-zinc-500 opacity-0 hover:bg-zinc-700 hover:text-zinc-200 group-hover:opacity-100"
        >
          <Icon className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="px-2 py-1 text-xs text-zinc-600">{text}</p>;
}
