import { useEffect, useState, type ReactNode } from "react";
import * as Tabs from "@radix-ui/react-tabs";
import {
  CheckCircle2,
  CircleDashed,
  Loader2,
  ShieldAlert,
  ShieldCheck,
  XCircle,
} from "lucide-react";

import { ipc, type CiStatus, type ContributorStat } from "../services/tauri-ipc";

function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? "?") + (parts[1]?.[0] ?? "")).toUpperCase();
}

/**
 * "Who Did What" dashboard (Variable Reward layer). Commit counts are real; ownership +
 * review throughput are scaffold placeholders. CI/compliance badges are real: DCO is
 * computed locally from commit trailers, build/test from the GitHub Checks API.
 */
export function CollaborationDashboard({ repoPath }: { repoPath: string | null }) {
  const [stats, setStats] = useState<ContributorStat[]>([]);
  const [ci, setCi] = useState<CiStatus[]>([]);

  useEffect(() => {
    if (!repoPath) return;
    ipc.contributorStats().then(setStats).catch(() => setStats([]));
    ipc.getCiStatus().then(setCi).catch(() => setCi([]));
  }, [repoPath]);

  const total = stats.reduce((sum, s) => sum + s.commits, 0) || 1;
  const max = stats.reduce((m, s) => Math.max(m, s.commits), 0) || 1;

  return (
    <Tabs.Root defaultValue="contributors" className="flex h-full flex-col">
      <Tabs.List className="flex gap-1 border-b border-zinc-800 px-4">
        <Trigger value="contributors">Contributors</Trigger>
        <Trigger value="ci">CI &amp; Compliance</Trigger>
      </Tabs.List>

      <Tabs.Content value="contributors" className="flex-1 overflow-auto p-5">
        <p className="mb-4 text-xs text-zinc-500">
          Ownership by commit attribution across reachable history.
        </p>
        <div className="space-y-2">
          {stats.map((s) => {
            const share = Math.round((s.commits / total) * 100);
            const heat = 0.15 + 0.85 * (s.commits / max);
            return (
              <div
                key={s.author_email || s.author_name}
                className="flex items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/50 px-3 py-2"
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
              </div>
            );
          })}
          {stats.length === 0 && (
            <p className="text-sm text-zinc-500">No contributor data yet.</p>
          )}
        </div>
      </Tabs.Content>

      <Tabs.Content value="ci" className="flex-1 overflow-auto p-5">
        <p className="mb-4 text-xs text-zinc-500">
          Pipeline health from the GitHub Checks API; DCO/CLA compliance from local commit
          trailers. A gray badge means the status could not be verified — not that it passed.
        </p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {ci.map((c) => (
            <CiCard key={c.pipeline} status={c} />
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
