import { useEffect, useState } from "react";
import { FileWarning, ShieldCheck } from "lucide-react";

import { ipc, type ConflictHunk } from "../services/tauri-ipc";

/**
 * Merge Conflict Editor (Action layer).
 *
 * Conflict *detection* is real (from the git2 index). The side-by-side content and the
 * resolve action are scaffold stubs — the layout + IPC wiring are in place so the split
 * diff can be filled in without changing the contract.
 */
export function MergeConflictEditor({ repoPath }: { repoPath: string | null }) {
  const [conflicts, setConflicts] = useState<ConflictHunk[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!repoPath) return;
    setLoading(true);
    ipc
      .listConflicts()
      .then((c) => {
        setConflicts(c);
        setSelected(c[0]?.file ?? null);
      })
      .catch(() => setConflicts([]))
      .finally(() => setLoading(false));
  }, [repoPath]);

  const resolve = async (file: string, choice: "ours" | "theirs") => {
    await ipc.resolveConflict(file, choice).catch(() => undefined);
    setConflicts((prev) => prev.filter((c) => c.file !== file));
  };

  if (!loading && conflicts.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center text-center">
        <ShieldCheck className="mb-3 h-10 w-10 text-emerald-400" />
        <p className="text-sm font-medium text-zinc-200">No merge conflicts</p>
        <p className="mt-1 text-xs text-zinc-500">Your working tree is clean.</p>
      </div>
    );
  }

  const active = conflicts.find((c) => c.file === selected) ?? null;

  return (
    <div className="flex h-full">
      <div className="w-64 shrink-0 overflow-auto border-r border-zinc-800">
        {conflicts.map((c) => (
          <button
            key={c.file}
            onClick={() => setSelected(c.file)}
            className={
              "flex w-full items-center gap-2 px-3 py-2 text-left text-xs " +
              (c.file === selected ? "bg-zinc-800 text-zinc-100" : "text-zinc-400 hover:bg-zinc-800/60")
            }
          >
            <FileWarning className="h-3.5 w-3.5 text-amber-400" />
            <span className="truncate">{c.file}</span>
          </button>
        ))}
      </div>

      <div className="flex flex-1 flex-col">
        <div className="border-b border-amber-500/20 bg-amber-500/5 px-4 py-2 text-[11px] text-amber-300/80">
          Scaffold: three-way content extraction is stubbed. Choose a side to simulate resolution.
        </div>

        {active ? (
          <>
            <div className="grid flex-1 grid-cols-2 gap-px overflow-hidden bg-zinc-800">
              <DiffPane title="Your changes" body={active.ours} tint="text-emerald-300" />
              <DiffPane title="Incoming changes" body={active.theirs} tint="text-blue-300" />
            </div>
            <div className="flex items-center gap-2 border-t border-zinc-800 px-4 py-3">
              <span className="mr-auto truncate text-xs text-zinc-400">{active.file}</span>
              <button
                onClick={() => resolve(active.file, "ours")}
                className="rounded-md bg-emerald-500/15 px-3 py-1.5 text-xs text-emerald-300 ring-1 ring-emerald-500/30 hover:bg-emerald-500/25"
              >
                Accept yours
              </button>
              <button
                onClick={() => resolve(active.file, "theirs")}
                className="rounded-md bg-blue-500/15 px-3 py-1.5 text-xs text-blue-300 ring-1 ring-blue-500/30 hover:bg-blue-500/25"
              >
                Accept incoming
              </button>
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center text-sm text-zinc-500">
            Select a conflicted file.
          </div>
        )}
      </div>
    </div>
  );
}

function DiffPane({ title, body, tint }: { title: string; body: string; tint: string }) {
  return (
    <div className="flex flex-col overflow-hidden bg-zinc-950">
      <div className={"border-b border-zinc-800 px-4 py-2 text-xs font-medium " + tint}>{title}</div>
      <pre className="flex-1 overflow-auto p-4 text-xs text-zinc-400">
        {body || "// content extraction not implemented in the scaffold"}
      </pre>
    </div>
  );
}
