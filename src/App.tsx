import { useState } from "react";
import * as RadixTooltip from "@radix-ui/react-tooltip";

import { CollaborationDashboard } from "./components/CollaborationDashboard";
import { CommitDetail } from "./components/CommitDetail";
import { CommitTimeline } from "./components/CommitTimeline";
import { MergeConflictEditor } from "./components/MergeConflictEditor";
import { Sidebar } from "./components/Sidebar";
import { SettingsPanel } from "./components/SettingsPanel";
import { TopBar } from "./components/TopBar";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { useCommits } from "./hooks/useCommits";
import { useRepository } from "./hooks/useRepository";
import { useTauriEvent } from "./hooks/useTauriEvent";
import type { View } from "./lib/views";
import { EVENTS, type CommitInfo } from "./services/tauri-ipc";

const TITLES: Record<View, string> = {
  timeline: "Commit Timeline",
  merge: "Merge Conflicts",
  dashboard: "Collaboration",
  settings: "Settings",
};

function App() {
  const { repo, error, pickAndOpen } = useRepository();
  const [view, setView] = useState<View>("timeline");
  const [selected, setSelected] = useState<CommitInfo | null>(null);

  const repoPath = repo?.path ?? null;
  const { commits, refresh } = useCommits(repoPath);

  // Ambient trigger: refresh the graph when the repo changes on disk.
  useTauriEvent<string>(EVENTS.repoChanged, () => {
    void refresh();
  });

  return (
    <RadixTooltip.Provider delayDuration={300}>
      <div className="flex h-screen w-screen overflow-hidden bg-zinc-950 text-zinc-100">
        <Sidebar view={view} onNavigate={setView} repo={repo} onOpenRepo={pickAndOpen} />

        <div className="flex min-w-0 flex-1 flex-col">
          {!repo ? (
            <WelcomeScreen onOpenRepo={pickAndOpen} error={error} />
          ) : (
            <>
              <TopBar
                title={TITLES[view]}
                repoPath={repoPath}
                live={Boolean(repoPath)}
                onRefresh={view === "timeline" ? () => void refresh() : undefined}
              />
              <div className="flex min-h-0 flex-1">
                <main className="min-w-0 flex-1">
                  {view === "timeline" && (
                    <CommitTimeline
                      commits={commits}
                      selectedId={selected?.id ?? null}
                      onSelect={setSelected}
                    />
                  )}
                  {view === "merge" && <MergeConflictEditor repoPath={repoPath} />}
                  {view === "dashboard" && <CollaborationDashboard repoPath={repoPath} />}
                  {view === "settings" && <SettingsPanel repoPath={repoPath} />}
                </main>

                {view === "timeline" && selected && (
                  <CommitDetail commit={selected} onClose={() => setSelected(null)} />
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </RadixTooltip.Provider>
  );
}

export default App;
