import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import * as Tabs from "@radix-ui/react-tabs";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  CheckCircle2,
  CircleDashed,
  ExternalLink,
  Loader2,
  ShieldAlert,
  ShieldCheck,
  XCircle,
} from "lucide-react";

import { useTauriEvent } from "../hooks/useTauriEvent";
import {
  EVENTS,
  ipc,
  type CiStatus,
  type CommitInfo,
  type ContributorStat,
  type ReviewerStat,
} from "../services/tauri-ipc";

function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? "?") + (parts[1]?.[0] ?? "")).toUpperCase();
}

/**
 * "Who Did What" dashboard (Variable Reward layer). Commit counts and CI/compliance are
 * real (DCO from local trailers, build/test from the GitHub Checks API), and review
 * throughput is pulled per reviewer from the GitHub PR/reviews API via `gh`.
 */
export function CollaborationDashboard({ repoPath }: { repoPath: string | null }) {
  const [stats, setStats] = useState<ContributorStat[]>([]);
  const [ci, setCi] = useState<CiStatus[]>([]);
  const [reviews, setReviews] = useState<ReviewerStat[]>([]);
  const [commits, setCommits] = useState<CommitInfo[]>([]);
  const [ghBase, setGhBase] = useState<string | null>(null);
  const [selectedEmail, setSelectedEmail] = useState<string | null>(null);

  const loadCi = useCallback(() => {
    ipc.getCiStatus().then(setCi).catch(() => setCi([]));
  }, []);
  const loadAll = useCallback(() => {
    ipc.contributorStats().then(setStats).catch(() => setStats([]));
    ipc.reviewStats().then(setReviews).catch(() => setReviews([]));
    ipc.listCommits(500).then(setCommits).catch(() => setCommits([]));
    ipc.githubRepoUrl().then(setGhBase).catch(() => setGhBase(null));
    loadCi();
  }, [loadCi]);

  useEffect(() => {
    if (repoPath) loadAll();
  }, [repoPath, loadAll]);

  // Option A: auto-refresh instead of only on mount. A single git operation writes many
  // files under .git, so debounce the repo-changed burst into one refresh.
  const debounce = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useTauriEvent<string>(EVENTS.repoChanged, () => {
    if (!repoPath) return;
    clearTimeout(debounce.current);
    debounce.current = setTimeout(loadAll, 400);
  });
  // The 30s heartbeat re-polls CI (live GitHub Checks for the current HEAD).
  useTauriEvent<void>(EVENTS.ciPoll, () => {
    if (repoPath) loadCi();
  });

  const total = stats.reduce((sum, s) => sum + s.commits, 0) || 1;
  const max = stats.reduce((m, s) => Math.max(m, s.commits), 0) || 1;

  // Reset the contributor drill-down when the active repo changes.
  useEffect(() => setSelectedEmail(null), [repoPath]);

  const selectedPerson = selectedEmail
    ? stats.find((s) => s.author_email === selectedEmail) ?? null
    : null;
  const selectedCommits = selectedEmail
    ? commits.filter((c) => c.author_email === selectedEmail)
    : [];

  return (
    <Tabs.Root defaultValue="contributors" className="flex h-full flex-col">
      <Tabs.List className="flex gap-1 border-b border-zinc-800 px-4">
        <Trigger value="contributors">Contributors</Trigger>
        <Trigger value="reviews">Reviews</Trigger>
        <Trigger value="ci">CI &amp; Compliance</Trigger>
      </Tabs.List>

      <Tabs.Content value="contributors" className="flex-1 overflow-auto p-5">
        {selectedEmail ? (
          <>
            <div className="mb-4 flex items-center gap-3">
              <button
                onClick={() => setSelectedEmail(null)}
                className="flex items-center gap-1 rounded-md border border-zinc-700 px-2 py-1 text-xs text-zinc-300 hover:bg-zinc-800"
              >
                <ArrowLeft className="h-3.5 w-3.5" /> Back
              </button>
              <span className="truncate text-sm font-medium text-zinc-100">
                {selectedPerson?.author_name || selectedEmail}
              </span>
              <span className="shrink-0 text-xs text-zinc-500">
                {selectedCommits.length} commit{selectedCommits.length === 1 ? "" : "s"}
                {ghBase ? " · click to open on GitHub" : " · not a GitHub repo"}
              </span>
            </div>
            <div className="space-y-1.5">
              {selectedCommits.map((c) => (
                <button
                  key={c.id}
                  disabled={!ghBase}
                  onClick={() => ghBase && void openUrl(`${ghBase}/commit/${c.id}`)}
                  title={ghBase ? "Open on GitHub" : c.id}
                  className={
                    "flex w-full items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-left " +
                    (ghBase ? "hover:border-zinc-700 hover:bg-zinc-800/60" : "cursor-default")
                  }
                >
                  <code className="shrink-0 text-[11px] text-emerald-400">{c.short_id}</code>
                  <span className="min-w-0 flex-1 truncate text-sm text-zinc-200">
                    {c.summary || "(no message)"}
                  </span>
                  <span className="shrink-0 text-[11px] text-zinc-500">{ago(c.timestamp)}</span>
                  {ghBase && <ExternalLink className="h-3.5 w-3.5 shrink-0 text-zinc-500" />}
                </button>
              ))}
              {selectedCommits.length === 0 && (
                <p className="text-sm text-zinc-500">
                  No commits found for this author in the loaded history.
                </p>
              )}
            </div>
          </>
        ) : (
          <>
            <p className="mb-4 text-xs text-zinc-500">
              Ownership by commit attribution across reachable history. Click a contributor to
              see their commits.
            </p>
            <div className="space-y-2">
              {stats.map((s) => {
                const share = Math.round((s.commits / total) * 100);
                const heat = 0.15 + 0.85 * (s.commits / max);
                return (
                  <button
                    key={s.author_email || s.author_name}
                    onClick={() => setSelectedEmail(s.author_email)}
                    className="flex w-full items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-left hover:border-zinc-700 hover:bg-zinc-800/60"
                  >
                    <div
                      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold text-emerald-950"
                      style={{ backgroundColor: `rgba(52,211,153,${heat})` }}
                    >
                      {initials(s.author_name)}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline justify-between gap-2">
                        <span className="truncate text-sm text-zinc-100">
                          {s.author_name || s.author_email || "Unknown"}
                        </span>
                        <span className="shrink-0 text-xs text-zinc-500">
                          {s.commits.toLocaleString()} commits · {share}%
                        </span>
                      </div>
                      <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-zinc-800">
                        <div
                          className="h-full rounded-full bg-emerald-400"
                          style={{ width: `${(s.commits / max) * 100}%` }}
                        />
                      </div>
                    </div>
                  </button>
                );
              })}
              {stats.length === 0 && (
                <p className="text-sm text-zinc-500">No contributor data yet.</p>
              )}
            </div>
          </>
        )}
      </Tabs.Content>

      <Tabs.Content value="reviews" className="flex-1 overflow-auto p-5">
        <p className="mb-4 text-xs text-zinc-500">
          Review throughput per reviewer, from the GitHub PR/reviews API (requires a GitHub
          remote and the <code>gh</code> CLI).
        </p>
        <div className="space-y-2">
          {reviews.map((r) => (
            <div
              key={r.login}
              className="flex items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/50 px-3 py-2"
            >
              <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-blue-500/20 text-[11px] font-semibold text-blue-300">
                {r.login.slice(0, 2).toUpperCase()}
              </div>
              <span className="min-w-0 flex-1 truncate text-sm text-zinc-100">{r.login}</span>
              <span className="shrink-0 text-xs text-zinc-500">
                {r.reviews.toLocaleString()} reviews · {r.approvals.toLocaleString()} approvals
              </span>
            </div>
          ))}
          {reviews.length === 0 && (
            <p className="text-sm text-zinc-500">
              No review data (non-GitHub repo, no PRs, or <code>gh</code> unavailable).
            </p>
          )}
        </div>
      </Tabs.Content>

      <Tabs.Content value="ci" className="flex-1 overflow-auto p-5">
        <p className="mb-4 text-xs text-zinc-500">
          Pipeline health from the GitHub Checks API; DCO/CLA compliance from local commit
          trailers. A gray badge means the status could not be verified — not that it passed.
        </p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {ci.map((c, i) => (
            <CiCard key={`${c.pipeline}-${i}`} status={c} />
          ))}
        </div>
      </Tabs.Content>
    </Tabs.Root>
  );
}

function Trigger({ value, children }: { value: string; children: ReactNode }) {
  return (
    <Tabs.Trigger
      value={value}
      className="border-b-2 border-transparent px-3 py-2.5 text-sm text-zinc-400 data-[state=active]:border-emerald-400 data-[state=active]:text-zinc-100"
    >
      {children}
    </Tabs.Trigger>
  );
}

// Only genuine success/verification is green. Anything unverified is neutral gray, never
// a green tick — the badge must not imply a check that never ran.
function kindFor(status: string) {
  switch (status) {
    case "success":
      return { Icon: CheckCircle2, color: "text-emerald-400", ring: "ring-emerald-500/30", spin: false };
    case "verified":
      return { Icon: ShieldCheck, color: "text-emerald-400", ring: "ring-emerald-500/30", spin: false };
    case "failed":
      return { Icon: XCircle, color: "text-red-400", ring: "ring-red-500/30", spin: false };
    case "running":
      return { Icon: Loader2, color: "text-amber-400", ring: "ring-amber-500/30", spin: true };
    case "unsigned":
      return { Icon: ShieldAlert, color: "text-amber-400", ring: "ring-amber-500/30", spin: false };
    default: // "unknown"
      return { Icon: CircleDashed, color: "text-zinc-500", ring: "ring-zinc-700/40", spin: false };
  }
}

function ago(epochSeconds: number | null | undefined): string | null {
  if (!epochSeconds) return null;
  const secs = Math.max(0, Math.floor(Date.now() / 1000 - epochSeconds));
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function CiCard({ status }: { status: CiStatus }) {
  const { Icon, color, ring, spin } = kindFor(status.status);
  const stale = ago(status.updated_at);

  return (
    <div className={"flex items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/50 px-4 py-3 ring-1 " + ring}>
      <Icon className={"h-5 w-5 " + color + (spin ? " animate-spin" : "")} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-zinc-100">{status.pipeline}</div>
        <div className="text-xs text-zinc-500">
          {status.status} · {status.badge}
        </div>
      </div>
      {stale && <span className="shrink-0 text-[10px] text-zinc-600">{stale}</span>}
    </div>
  );
}
