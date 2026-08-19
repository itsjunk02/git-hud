import { GitCommit, X } from "lucide-react";

import type { CommitInfo } from "../services/tauri-ipc";

function formatDate(seconds: number | null): string {
  if (!seconds) return "unknown date";
  return new Date(seconds * 1000).toLocaleString();
}

/** Right-hand inspector for the commit selected in the timeline. */
export function CommitDetail({
  commit,
  onClose,
}: {
  commit: CommitInfo | null;
  onClose: () => void;
}) {
  if (!commit) return null;

  return (
    <div className="flex h-full w-80 flex-col border-l border-zinc-800 bg-zinc-900/60">
      <div className="flex items-center gap-2 border-b border-zinc-800 px-4 py-3">
        <GitCommit className="h-4 w-4 text-emerald-400" />
        <span className="text-sm font-medium">Commit</span>
        <button
          onClick={onClose}
          className="ml-auto rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
          aria-label="Close"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="flex-1 space-y-4 overflow-auto p-4 text-sm">
        <p className="font-medium leading-snug text-zinc-100">
          {commit.summary || "(no message)"}
        </p>
        {commit.body && (
          <pre className="whitespace-pre-wrap rounded-md bg-zinc-950/60 p-3 text-xs text-zinc-400">
            {commit.body}
          </pre>
        )}

        <dl className="space-y-2 text-xs">
          <Row label="Author" value={`${commit.author_name} <${commit.author_email}>`} />
          <Row label="Date" value={formatDate(commit.timestamp)} />
          <Row label="Commit" value={commit.id} mono />
          <Row
            label={commit.parent_ids.length === 1 ? "Parent" : "Parents"}
            value={commit.parent_ids.map((p) => p.slice(0, 8)).join(", ") || "root"}
            mono
          />
        </dl>
      </div>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <dt className="text-zinc-500">{label}</dt>
      <dd className={"break-all text-zinc-300" + (mono ? " font-mono" : "")}>{value}</dd>
    </div>
  );
}
