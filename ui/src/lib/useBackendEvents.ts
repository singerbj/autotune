import { useQueryClient } from "@tanstack/react-query";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { events } from "./api";
import { meterStore } from "./meters";
import { queryKeys, setBypassCache, setEngineStatusCache } from "./queries";
import { toastError } from "./toast";

/**
 * Subscribes to every backend event once and folds it into the query cache
 * (or the ref-based meter store). Rust stays the source of truth.
 */
export function useBackendEvents(): void {
  const qc = useQueryClient();

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const keep = (pending: Promise<UnlistenFn>) => {
      pending.then(
        (unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        },
        (error: unknown) => console.error("Failed to subscribe to backend event", error),
      );
    };

    keep(events.engineStatusEvent.listen((e) => setEngineStatusCache(qc, e.payload)));
    keep(events.metersEvent.listen((e) => meterStore.set(e.payload)));
    keep(events.bypassEvent.listen((e) => setBypassCache(qc, e.payload)));
    keep(events.updateEvent.listen((e) => qc.setQueryData(queryKeys.updateStatus, e.payload)));
    keep(events.devicesChangedEvent.listen((e) => qc.setQueryData(queryKeys.devices, e.payload)));
    keep(events.errorEvent.listen((e) => toastError("Voice Tuner", e.payload)));

    return () => {
      disposed = true;
      for (const unlisten of unlisteners.splice(0)) unlisten();
    };
  }, [qc]);
}
