import { useEffect, useState } from "react";
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
import { useWorkspace } from "./hooks/useWorkspace";
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
  const { projects, activePath, error, openProject, closeProject, setActive } =
    useWorkspace();
  const [view, setView] = useState<View>("timeline");
  const [selected, setSelected] = useState<CommitInfo | null>(null);

  const { commits, refresh } = useCommits(activePath);

  // Ambient trigger: refresh the active graph when any repo changes on disk.
  useTauriEvent<string>(EVENTS.repoChanged, () => {
    void refresh();
  });

  // Switching (or closing) the active project invalidates the selected commit.
  useEffect(() => {
    setSelected(null);
  }, [activePath]);

  return (
    <RadixTooltip.Provider delayDuration={300}>
      <div className="flex h-screen w-screen overflow-hidden bg-zinc-950 text-zinc-100">
        <Sidebar
          view={view}
          onNavigate={setView}
          projects={projects}
          activePath={activePath}
          onOpenProject={openProject}
          onCloseProject={closeProject}
          onSelectProject={setActive}
        />

        <div className="flex min-w-0 flex-1 flex-col">
          {!activePath ? (
            <WelcomeScreen onOpenRepo={openProject} error={error} />
          ) : (
            <>
              <TopBar
                title={TITLES[view]}
                repoPath={activePath}
                live
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
                  {view === "merge" && <MergeConflictEditor repoPath={activePath} />}
                  {view === "dashboard" && <CollaborationDashboard repoPath={activePath} />}
                  {view === "settings" && <SettingsPanel repoPath={activePath} />}
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
