import { Plus, SquareTerminal, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { TerminalSession } from "./TerminalSession";

const MIN_HEIGHT = 120;
const MAX_HEIGHT = 640;

type Tab = { key: number };

/**
 * VS Code–style collapsible terminal drawer with multiple tabbed shell sessions. Every
 * session stays mounted (inactive ones hidden via CSS) so the shell + scrollback survive tab
 * switches and collapse/expand. Rendered only when a repo is active, so new shells spawn in
 * the active repo's working directory (resolved backend-side).
 */
export function TerminalPanel({
  height,
  onHeightChange,
  onClose,
}: {
  height: number;
  onHeightChange: (h: number) => void;
  onClose: () => void;
}) {
  const nextKey = useRef(1);
  const [tabs, setTabs] = useState<Tab[]>(() => [{ key: 0 }]);
  const [activeKey, setActiveKey] = useState(0);

  const addTab = useCallback(() => {
    const key = nextKey.current++;
    setTabs((t) => [...t, { key }]);
    setActiveKey(key);
  }, []);

  const closeTab = useCallback((key: number) => {
    setTabs((prev) => prev.filter((t) => t.key !== key));
  }, []);

  // Keep the active tab valid; collapse the drawer once the last tab is gone.
  useEffect(() => {
    if (tabs.length === 0) {
      onClose();
    } else if (!tabs.some((t) => t.key === activeKey)) {
      setActiveKey(tabs[tabs.length - 1].key);
    }
  }, [tabs, activeKey, onClose]);

  // Drag the top edge to resize the drawer height.
  const onDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startY = e.clientY;
      const startH = height;
      const onMove = (ev: MouseEvent) => {
        const h = Math.min(Math.max(startH + (startY - ev.clientY), MIN_HEIGHT), MAX_HEIGHT);
        onHeightChange(h);
      };
      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [height, onHeightChange],
  );

  return (
    <div
      className="flex shrink-0 flex-col border-t border-zinc-800 bg-zinc-950"
      style={{ height }}
    >
      <div
        onMouseDown={onDragStart}
        className="h-1 w-full shrink-0 cursor-row-resize hover:bg-emerald-500/40"
        title="Drag to resize"
      />

      <div className="flex shrink-0 items-center gap-1 border-b border-zinc-800 px-2 py-1.5">
        <SquareTerminal className="mx-1 h-4 w-4 shrink-0 text-emerald-400" />
        <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {tabs.map((t, i) => {
            const active = t.key === activeKey;
            return (
              <div
                key={t.key}
                role="button"
                tabIndex={0}
                onClick={() => setActiveKey(t.key)}
                onKeyDown={(e) =>
                  (e.key === "Enter" || e.key === " ") && setActiveKey(t.key)
                }
                className={
                  "flex shrink-0 items-center gap-1.5 rounded px-2 py-1 text-xs transition-colors " +
                  (active
                    ? "bg-emerald-500/15 text-emerald-300 ring-1 ring-emerald-500/30"
                    : "text-zinc-400 hover:bg-zinc-800/70")
                }
              >
                <span>{i + 1}: shell</span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(t.key);
                  }}
                  aria-label={`Close terminal ${i + 1}`}
                  className="rounded p-0.5 text-zinc-500 hover:bg-zinc-700 hover:text-zinc-200"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            );
          })}
          <button
            onClick={addTab}
            title="New terminal"
            aria-label="New terminal"
            className="shrink-0 rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>
        <button
          onClick={onClose}
          title="Hide terminal (Ctrl+`)"
          aria-label="Hide terminal"
          className="ml-auto shrink-0 rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="relative min-h-0 flex-1">
        {tabs.map((t) => (
          <div
            key={t.key}
            className={"absolute inset-0 p-1 " + (t.key === activeKey ? "" : "hidden")}
          >
            <TerminalSession visible={t.key === activeKey} onExit={() => closeTab(t.key)} />
          </div>
        ))}
      </div>
    </div>
  );
}
