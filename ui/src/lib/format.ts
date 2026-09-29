import type { BackendTier, SupervisorState, VoiceRange } from "@/bindings";

/** FR-02: human labels for the capture fallback chain. */
export const TIER_LABELS: Record<BackendTier, string> = {
  asio: "ASIO",
  wasapiExclusive: "WASAPI exclusive",
  wasapiSharedLowLatency: "WASAPI shared (low latency)",
  wasapiShared: "WASAPI shared",
  mock: "Simulated",
};

export function tierLabel(tier: BackendTier): string {
  return TIER_LABELS[tier];
}

export const SUPERVISOR_LABELS: Record<SupervisorState, string> = {
  running: "Running",
  recovering: "Recovering",
  stopped: "Stopped",
};

/** FR-08: voice range presets (lowest tracked pitch → fixed DSP latency). */
export const VOICE_RANGES: ReadonlyArray<{
  value: VoiceRange;
  label: string;
  lowestHz: number;
  latencyMs: number;
}> = [
  { value: "low", label: "Low", lowestHz: 70, latencyMs: 14 },
  { value: "mid", label: "Mid", lowestHz: 100, latencyMs: 10 },
  { value: "high", label: "High", lowestHz: 150, latencyMs: 7 },
];

/** Frames at a sample rate → milliseconds. */
export function framesToMs(frames: number, sampleRate: number): number {
  return sampleRate > 0 ? (frames / sampleRate) * 1000 : 0;
}

export function formatMs(ms: number, digits = 1): string {
  return `${ms.toFixed(digits)} ms`;
}

export function formatPercent(fraction: number): string {
  return `${Math.round(fraction * 100)}%`;
}

export function formatGate(db: number): string {
  return db <= -100 ? "Off" : `${Math.round(db)} dBFS`;
}

export function formatHz(hz: number): string {
  return hz > 0 ? `${hz.toFixed(1)} Hz` : "—";
}
