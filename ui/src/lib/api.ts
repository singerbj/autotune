/**
 * The one import point for backend calls. Inside Tauri it re-exports the
 * generated bindings; in a plain browser (`vite dev`) or Vitest it swaps in a
 * stateful mock with exactly the same types, so the UI can be developed and
 * tested without the Rust side.
 */
import { commands as tauriCommands, events as tauriEvents } from "@/bindings";

import { createMockBackend, type MockBackend } from "./mock";

export type Commands = typeof tauriCommands;
export type Events = typeof tauriEvents;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export const isTauri: boolean =
  typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;

/** The in-memory backend, or `null` when running inside Tauri. */
export const mockBackend: MockBackend | null = isTauri ? null : createMockBackend();

export const commands: Commands = mockBackend?.commands ?? tauriCommands;
export const events: Events = mockBackend?.events ?? tauriEvents;
