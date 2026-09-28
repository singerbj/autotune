import { useSyncExternalStore } from "react";

export type ToastVariant = "default" | "success" | "error";

export interface ToastItem {
  id: number;
  title: string;
  description?: string;
  variant: ToastVariant;
}

export interface ToastInput {
  title: string;
  description?: string;
  variant?: ToastVariant;
  durationMs?: number;
}

type Listener = () => void;

let items: readonly ToastItem[] = [];
let nextId = 1;
const listeners = new Set<Listener>();
const timers = new Map<number, ReturnType<typeof setTimeout>>();

function publish(next: readonly ToastItem[]): void {
  items = next;
  for (const listener of listeners) listener();
}

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) clearTimeout(timer);
  timers.delete(id);
  publish(items.filter((item) => item.id !== id));
}

/** Show a toast; returns its id. At most five are kept on screen. */
export function toast({ title, description, variant = "default", durationMs }: ToastInput): number {
  const id = nextId++;
  const item: ToastItem =
    description === undefined ? { id, title, variant } : { id, title, description, variant };
  publish([...items, item].slice(-5));
  const duration = durationMs ?? (variant === "error" ? 8000 : 4000);
  timers.set(
    id,
    setTimeout(() => dismissToast(id), duration),
  );
  return id;
}

export function toastError(title: string, description?: string): number {
  return toast(
    description === undefined
      ? { title, variant: "error" }
      : { title, description, variant: "error" },
  );
}

export function clearToasts(): void {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  publish([]);
}

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot(): readonly ToastItem[] {
  return items;
}

export function useToasts(): readonly ToastItem[] {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
