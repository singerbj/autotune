import { useEffect, useState } from "react";

import type { MeterSnapshot } from "@/bindings";

/**
 * Latest 30 Hz meter snapshot (FR-17). Written by the `metersEvent` listener
 * and read by the canvas on each animation frame, so React never re-renders
 * at meter rate.
 */
class MeterStore {
  #latest: MeterSnapshot | null = null;
  #receivedAt = 0;
  #version = 0;

  get latest(): MeterSnapshot | null {
    return this.#latest;
  }

  /** `performance.now()` of the last snapshot, 0 if none. */
  get receivedAt(): number {
    return this.#receivedAt;
  }

  get version(): number {
    return this.#version;
  }

  set(snapshot: MeterSnapshot): void {
    this.#latest = snapshot;
    this.#receivedAt = performance.now();
    this.#version++;
  }

  clear(): void {
    this.#latest = null;
    this.#receivedAt = 0;
    this.#version++;
  }
}

export const meterStore = new MeterStore();

/**
 * Low-rate React view of the meters (counters, diagnostics). Polls the store
 * every `intervalMs` and only re-renders when a new snapshot arrived.
 */
export function useMeterSnapshot(intervalMs = 250): MeterSnapshot | null {
  const [snapshot, setSnapshot] = useState<MeterSnapshot | null>(() => meterStore.latest);
  useEffect(() => {
    let seen = meterStore.version;
    const id = setInterval(() => {
      if (meterStore.version !== seen) {
        seen = meterStore.version;
        setSnapshot(meterStore.latest);
      }
    }, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return snapshot;
}
