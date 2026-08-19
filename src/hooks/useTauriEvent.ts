import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Subscribe to a backend-emitted Tauri event for the lifetime of the component.
 * The latest `handler` is always invoked without re-subscribing on every render.
 */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void) {
  const saved = useRef(handler);
  saved.current = handler;

  useEffect(() => {
    let active = true;
    let unlisten: UnlistenFn | undefined;

    listen<T>(event, (e) => saved.current(e.payload)).then((fn) => {
      if (active) unlisten = fn;
      else fn();
    });

    return () => {
      active = false;
      unlisten?.();
    };
  }, [event]);
}
