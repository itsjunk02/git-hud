import { useCallback, useEffect, useState } from "react";
import { FileWarning, GitMerge, ShieldCheck } from "lucide-react";

import {
  ipc,
  type ConflictAuthor,
  type ConflictHunk,
  type MergeConflictSummary,
} from "../services/tauri-ipc";

/** Comma-joined usernames for an author set (em dash when a side has no attributable lines). */
function handles(authors: ConflictAuthor[]): string {
  return authors.map((a) => a.username).join(", ") || "—";
}

/**
 * Merge Conflict Editor (Action layer). Shows the three-way sides from the git2 index and,
 * from blame, *who* is in conflict with *whom* — per file, per <<<<<<< region, and rolled up
 * into merge-wide who-vs-whom pairs. You can take one whole side (yours/incoming) or hand-edit
 * the merged result and save it — both write + stage the file, clearing the conflict.
 */
export function MergeConflictEditor({ repoPath }: { repoPath: string | null }) {
  const [conflicts, setConflicts] = useState<ConflictHunk[]>([]);
  const [summary, setSummary] = useState<MergeConflictSummary | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [draft, setDraft] = useState("");

  const load = useCallback(() => {
    if (!repoPath) return;
    setLoading(true);
    ipc
      .listConflicts()
      .then((c) => {
        setConflicts(c);
        setSelected((prev) => (c.some((x) => x.file === prev) ? prev : c[0]?.file ?? null));
      })
      .catch(() => setConflicts([]))
      .finally(() => setLoading(false));
    ipc.conflictSummary().then(setSummary).catch(() => setSummary(null));
  }, [repoPath]);

  useEffect(() => {
    load();
  }, [load]);

  const active = conflicts.find((c) => c.file === selected) ?? null;

  // Seed the editable merged pane whenever the selected conflict changes.
  useEffect(() => {
    setDraft(active?.merged ?? "");
  }, [active?.file, active?.merged]);

  const resolveSide = async (file: string, choice: "ours" | "theirs") => {
    await ipc.resolveConflict(file, choice).catch(() => undefined);
    load();
  };

  const saveManual = async () => {
    if (!active) return;
    await ipc.saveConflictResolution(active.file, draft).catch(() => undefined);
    load();
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

  return (
    <div className="flex h-full flex-col">
      <ConflictBanner summary={summary} />

      <div className="flex min-h-0 flex-1">
        <div className="w-64 shrink-0 overflow-auto border-r border-zinc-800">
          {conflicts.map((c) => (
            <button
              key={c.file}
              onClick={() => setSelected(c.file)}
              className={
                "flex w-full flex-col gap-1 px-3 py-2 text-left text-xs " +
                (c.file === selected ? "bg-zinc-800 text-zinc-100" : "text-zinc-400 hover:bg-zinc-800/60")
              }
            >
              <span className="flex items-center gap-2">
                <FileWarning className="h-3.5 w-3.5 shrink-0 text-amber-400" />
                <span className="truncate">{c.file}</span>
              </span>
              <span className="truncate pl-5 text-[11px] text-zinc-500">
                <span className="text-emerald-300/90">{handles(c.ours_authors)}</span>
                <span className="mx-1 text-zinc-600">↔</span>
                <span className="text-blue-300/90">{handles(c.theirs_authors)}</span>
              </span>
            </button>
          ))}
        </div>

        <div className="flex min-w-0 flex-1 flex-col">
          {active ? (
            <>
              {/* Read-only reference: the two sides, each with its authors. */}
              <div className="grid h-40 shrink-0 grid-cols-2 gap-px overflow-hidden border-b border-zinc-800 bg-zinc-800">
                <DiffPane
                  title="Your changes"
                  subtitle={handles(active.ours_authors)}
                  body={active.ours}
                  tint="text-emerald-300"
                />
                <DiffPane
                  title="Incoming changes"
                  subtitle={handles(active.theirs_authors)}
                  body={active.theirs}
                  tint="text-blue-300"
                />
              </div>

              <RegionBreakdown hunk={active} />

              {/* Editable merged result, seeded with the working-tree conflict markers. */}
              <div className="flex min-h-0 flex-1 flex-col">
                <div className="border-b border-zinc-800 px-4 py-2 text-xs font-medium text-zinc-300">
                  Merged result — edit, then save
                </div>
                <textarea
                  value={draft}
                  onChange={(e) => setDraft(e.currentTarget.value)}
                  spellCheck={false}
                  className="flex-1 resize-none bg-zinc-950 p-4 font-mono text-xs text-zinc-200 outline-none"
                />
              </div>

              <div className="flex items-center gap-2 border-t border-zinc-800 px-4 py-3">
                <span className="mr-auto truncate text-xs text-zinc-400">{active.file}</span>
                <button
                  onClick={() => resolveSide(active.file, "ours")}
                  className="rounded-md bg-emerald-500/15 px-3 py-1.5 text-xs text-emerald-300 ring-1 ring-emerald-500/30 hover:bg-emerald-500/25"
                >
                  Accept yours
                </button>
                <button
                  onClick={() => resolveSide(active.file, "theirs")}
                  className="rounded-md bg-blue-500/15 px-3 py-1.5 text-xs text-blue-300 ring-1 ring-blue-500/30 hover:bg-blue-500/25"
                >
                  Accept incoming
                </button>
                <button
                  onClick={saveManual}
                  className="rounded-md bg-emerald-500 px-3 py-1.5 text-xs font-medium text-emerald-950 hover:bg-emerald-400"
                >
                  Save resolution
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
    </div>
  );
}

/** Top banner: what's being merged, and the merge-wide who-vs-whom pairs. */
function ConflictBanner({ summary }: { summary: MergeConflictSummary | null }) {
  if (!summary || !summary.in_merge) return null;
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-zinc-800 bg-zinc-900/40 px-4 py-2.5">
      <span className="flex items-center gap-2 text-xs text-zinc-300">
        <GitMerge className="h-4 w-4 text-amber-400" />
        {summary.incoming_branch ? (
          <>
            Merging <code className="text-amber-300">{summary.incoming_branch}</code>
            {summary.ours_branch ? <> into <code className="text-zinc-300">{summary.ours_branch}</code></> : null}
          </>
        ) : (
          <>Merge in progress</>
        )}
      </span>
      <div className="flex flex-wrap items-center gap-1.5">
        {summary.pairs.length === 0 ? (
          <span className="text-[11px] text-zinc-500">No attributable authors</span>
        ) : (
          summary.pairs.map((p) => (
            <span
              key={`${p.ours}->${p.theirs}`}
              className="flex items-center gap-1 rounded-full bg-zinc-800/70 px-2 py-0.5 text-[11px] ring-1 ring-zinc-700"
              title={`${p.ours} vs ${p.theirs} across ${p.regions} region${p.regions === 1 ? "" : "s"}`}
            >
              <span className="text-emerald-300">{p.ours}</span>
              <span className="text-zinc-500">↔</span>
              <span className="text-blue-300">{p.theirs}</span>
              <span className="text-zinc-500">·{p.regions}</span>
            </span>
          ))
        )}
      </div>
    </div>
  );
}

/** Per-region who-vs-whom breakdown for the selected file. */
function RegionBreakdown({ hunk }: { hunk: ConflictHunk }) {
  if (hunk.regions.length === 0) return null;
  return (
    <div className="max-h-28 shrink-0 overflow-auto border-b border-zinc-800 bg-zinc-950/60 px-4 py-2">
      <div className="mb-1 text-[11px] font-medium uppercase tracking-wide text-zinc-500">
        {hunk.regions.length} conflict region{hunk.regions.length === 1 ? "" : "s"}
      </div>
      <div className="space-y-0.5">
        {hunk.regions.map((r) => {
          const from = r.ours_start || r.theirs_start;
          const to = r.theirs_end || r.ours_end;
          return (
            <div key={r.index} className="flex items-center gap-2 text-[11px] text-zinc-400">
              <span className="w-14 shrink-0 text-zinc-500">region {r.index}</span>
              <span className="w-20 shrink-0 text-zinc-600">
                {from ? `L${from}–${to}` : "—"}
              </span>
              <span className="text-emerald-300/90">{handles(r.ours_authors)}</span>
              <span className="text-zinc-600">↔</span>
              <span className="text-blue-300/90">{handles(r.theirs_authors)}</span>
              {r.approximate && (
                <span className="text-amber-500/70" title="Blame line-matching fell back to whole-side authors">
                  ~approx
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function DiffPane({
  title,
  subtitle,
  body,
  tint,
}: {
  title: string;
  subtitle?: string;
  body: string;
  tint: string;
}) {
  return (
    <div className="flex flex-col overflow-hidden bg-zinc-950">
      <div className={"flex items-baseline gap-2 border-b border-zinc-800 px-4 py-2 text-xs font-medium " + tint}>
        <span>{title}</span>
        {subtitle && <span className="truncate text-[11px] font-normal text-zinc-500">{subtitle}</span>}
      </div>
      <pre className="flex-1 overflow-auto p-4 text-xs text-zinc-400">{body || "(empty)"}</pre>
    </div>
  );
}
