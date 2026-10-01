import type { BackendTier, DelayDivision, FxRoute, SupervisorState, VoiceRange } from "@/bindings";

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

/** FR-26: where a time-based effect is heard. */
export const FX_ROUTES: ReadonlyArray<{ value: FxRoute; label: string; long: string }> = [
  { value: "both", label: "Both", long: "in both" },
  { value: "headphones", label: "Headphones", long: "in headphones only" },
  { value: "discord", label: "Discord", long: "in Discord only" },
];

/** FR-25: echo lengths as note values. */
export const DELAY_DIVISIONS: ReadonlyArray<{ value: DelayDivision; label: string }> = [
  { value: "quarter", label: "1/4 note" },
  { value: "eighth", label: "1/8 note" },
  { value: "dottedEighth", label: "Dotted 1/8" },
  { value: "sixteenth", label: "1/16 note" },
];

/** Signed decibels, e.g. "+3 dB", "0 dB", "−2.5 dB". */
export function formatDb(db: number): string {
  const rounded = Math.round(db * 10) / 10;
  if (rounded === 0) return "0 dB";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded)} dB`;
}

/** Signed semitones for the formant control. */
export function formatSemitones(st: number): string {
  const rounded = Math.round(st * 10) / 10;
  if (rounded === 0) return "0 st";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded)} st`;
}
