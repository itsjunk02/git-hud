import { useEffect, useState } from "react";
import * as RadixTooltip from "@radix-ui/react-tooltip";

import { ChangesPanel } from "./components/ChangesPanel";
import { CollaborationDashboard } from "./components/CollaborationDashboard";
import { CommitDetail } from "./components/CommitDetail";
import { CommitTimeline } from "./components/CommitTimeline";
import { MergeConflictEditor } from "./components/MergeConflictEditor";
import { Sidebar } from "./components/Sidebar";
import { SettingsPanel } from "./components/SettingsPanel";
import { TerminalPanel } from "./components/TerminalPanel";
import { TopBar } from "./components/TopBar";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { useCommits } from "./hooks/useCommits";
import { useWorkspace } from "./hooks/useWorkspace";
import { useTauriEvent } from "./hooks/useTauriEvent";
import type { View } from "./lib/views";
import { EVENTS, type CommitInfo } from "./services/tauri-ipc";

const TITLES: Record<View, string> = {
  timeline: "Commit Timeline",
  changes: "Changes",
  merge: "Merge Conflicts",
  dashboard: "Collaboration",
  settings: "Settings",
};

function App() {
  const { projects, activePath, error, openProject, closeProject, setActive } =
    useWorkspace();
  const [view, setView] = useState<View>("timeline");
  const [selected, setSelected] = useState<CommitInfo | null>(null);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [terminalHeight, setTerminalHeight] = useState(260);

  const { commits, refresh } = useCommits(activePath);

  // Ambient trigger: refresh the active graph when any repo changes on disk.
  useTauriEvent<string>(EVENTS.repoChanged, () => {
    void refresh();
  });

  // VS Code–style terminal toggle. Capture phase + stopPropagation so it works even when
  // the terminal has keyboard focus (xterm would otherwise swallow the keystroke).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key === "`") {
        e.preventDefault();
        e.stopPropagation();
        setTerminalOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

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
                terminalOpen={terminalOpen}
                onToggleTerminal={() => setTerminalOpen((v) => !v)}
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
                  {view === "changes" && <ChangesPanel repoPath={activePath} />}
                  {view === "merge" && <MergeConflictEditor repoPath={activePath} />}
                  {view === "dashboard" && <CollaborationDashboard repoPath={activePath} />}
                  {view === "settings" && <SettingsPanel repoPath={activePath} />}
                </main>

                {view === "timeline" && selected && (
                  <CommitDetail commit={selected} onClose={() => setSelected(null)} />
                )}
              </div>
              {terminalOpen && (
                <TerminalPanel
                  height={terminalHeight}
                  onHeightChange={setTerminalHeight}
                  onClose={() => setTerminalOpen(false)}
                />
              )}
            </>
          )}
        </div>
      </div>
    </RadixTooltip.Provider>
  );
}

export default App;
