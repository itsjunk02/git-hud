import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type IDisposable } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";

import { ipc, ptyExitEvent, ptyOutputEvent } from "../services/tauri-ipc";

/** xterm palette matched to the app's dark + emerald design system. */
const THEME = {
  background: "#09090b", // zinc-950, same as the app shell
  foreground: "#e4e4e7", // zinc-200
  cursor: "#10b981", // emerald-500 accent
  cursorAccent: "#09090b",
  selectionBackground: "rgba(16, 185, 129, 0.25)",
};

const FONT =
  'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace';

/**
 * One xterm.js instance bound to one backend PTY session. Owns the full lifecycle: it
 * generates the session id, subscribes to output *before* spawning (so the first prompt is
 * never lost), streams keystrokes to the shell, reflows on resize, and kills the PTY on
 * unmount. StrictMode-safe: a spawn that resolves after unmount is torn down immediately.
 */
export function TerminalSession({
  visible,
  onExit,
}: {
  visible: boolean;
  onExit: () => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const idRef = useRef<string | null>(null);
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let disposed = false;
    let id: string | null = null;
    let dataDisposable: IDisposable | undefined;
    const unlisteners: UnlistenFn[] = [];

    const term = new Terminal({
      theme: THEME,
      fontFamily: FONT,
      fontSize: 13,
      scrollback: 1000,
      cursorBlink: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    termRef.current = term;
    fitRef.current = fit;

    const safeFit = () => {
      if (container.clientHeight > 0 && container.clientWidth > 0) {
        try {
          fit.fit();
        } catch {
          /* container not measured yet */
        }
      }
    };
    safeFit();

    const clientId = crypto.randomUUID();
    void (async () => {
      // Subscribe before spawning — events aren't buffered, so this avoids missing the
      // shell's initial prompt.
      const offOutput = await listen<number[]>(ptyOutputEvent(clientId), (e) => {
        term.write(new Uint8Array(e.payload));
      });
      const offExit = await listen(ptyExitEvent(clientId), () => {
        term.write("\r\n\x1b[90m[process exited]\x1b[0m\r\n");
        onExitRef.current();
      });
      if (disposed) {
        offOutput();
        offExit();
        return;
      }
      unlisteners.push(offOutput, offExit);

      try {
        await ipc.ptySpawn(clientId, term.cols || 80, term.rows || 24);
      } catch (e) {
        if (!disposed) {
          term.writeln(`\x1b[31mfailed to start shell: ${String(e)}\x1b[0m`);
        }
        return;
      }
      if (disposed) {
        // Unmounted while spawning (e.g. StrictMode) — kill the orphan.
        void ipc.ptyKill(clientId);
        return;
      }
      id = clientId;
      idRef.current = clientId;
      dataDisposable = term.onData((data) => void ipc.ptyWrite(clientId, data));
    })();

    // A single ResizeObserver reflows the pty for every container size change — drag-resize,
    // window resize, and hidden→visible transitions all funnel through here.
    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        safeFit();
        if (id) void ipc.ptyResize(id, term.cols, term.rows);
      });
    });
    ro.observe(container);

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
      dataDisposable?.dispose();
      for (const un of unlisteners) un();
      if (id) void ipc.ptyKill(id);
      idRef.current = null;
      termRef.current = null;
      fitRef.current = null;
      term.dispose();
    };
  }, []);

  // Refit + focus when this tab becomes the active (visible) one.
  useEffect(() => {
    if (!visible) return;
    const raf = requestAnimationFrame(() => {
      const term = termRef.current;
      const fit = fitRef.current;
      const container = containerRef.current;
      if (!term || !fit || !container) return;
      if (container.clientHeight > 0 && container.clientWidth > 0) {
        try {
          fit.fit();
        } catch {
          /* ignore */
        }
        if (idRef.current) void ipc.ptyResize(idRef.current, term.cols, term.rows);
      }
      term.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [visible]);

  return <div ref={containerRef} className="h-full w-full" />;
}
