/** Pure pitch/level math shared by the meters and the Tune panel (FR-06, FR-17). */

export const NOTE_NAMES = [
  "C",
  "C#",
  "D",
  "D#",
  "E",
  "F",
  "F#",
  "G",
  "G#",
  "A",
  "A#",
  "B",
] as const;

export type NoteName = (typeof NOTE_NAMES)[number];

/** Pitch classes that are black keys on a piano. */
export const BLACK_KEYS: ReadonlySet<number> = new Set([1, 3, 6, 8, 10]);

const A4_HZ = 440;
const A4_MIDI = 69;

/** Positive modulo so negative inputs still land in `0..n-1`. */
export function mod(value: number, n: number): number {
  return ((value % n) + n) % n;
}

/** Pitch-class name for `0..11` (wraps any integer). */
export function pitchClassName(pc: number): NoteName {
  return NOTE_NAMES[mod(Math.round(pc), 12)] ?? "C";
}

/**
 * MIDI note number → scientific pitch name ("A4" = 69, "C4" = 60).
 * Returns `null` for "no note" (negative, e.g. `targetMidi === -1`) or non-finite input.
 */
export function midiToNoteName(midi: number): string | null {
  if (!Number.isFinite(midi) || midi < 0) return null;
  const m = Math.round(midi);
  return `${pitchClassName(m)}${Math.floor(m / 12) - 1}`;
}

/** Frequency → fractional MIDI note. `null` when unvoiced (≤ 0 Hz). */
export function hzToMidi(hz: number): number | null {
  if (!Number.isFinite(hz) || hz <= 0) return null;
  return A4_MIDI + 12 * Math.log2(hz / A4_HZ);
}

/** MIDI note → frequency in Hz. */
export function midiToHz(midi: number): number {
  return A4_HZ * 2 ** ((midi - A4_MIDI) / 12);
}

/** Nearest note name for a frequency, e.g. 220 Hz → "A3". */
export function hzToNoteName(hz: number): string | null {
  const midi = hzToMidi(hz);
  return midi === null ? null : midiToNoteName(midi);
}

export function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.min(max, Math.max(min, value));
}

/** dBFS → 0..1 bar fraction over `[minDb, maxDb]` (default −60..0 dB). */
export function dbToFraction(db: number, minDb = -60, maxDb = 0): number {
  if (Number.isNaN(db) || db === -Infinity) return 0;
  return clamp((db - minDb) / (maxDb - minDb), 0, 1);
}

/** Clamp a correction to ±`limit` cents (the needle's range). */
export function clampCents(cents: number, limit = 100): number {
  if (!Number.isFinite(cents)) return Number.isNaN(cents) ? 0 : Math.sign(cents) * limit;
  return clamp(cents, -limit, limit);
}

/** Needle position 0..1 for a correction in cents (0.5 = centred). */
export function centsToFraction(cents: number, limit = 100): number {
  return (clampCents(cents, limit) + limit) / (2 * limit);
}

/** All 12 pitch classes enabled. */
export const FULL_MASK = 0xfff;

/** Bit `pc` of a custom scale mask (C = bit 0). */
export function maskHas(mask: number, pc: number): boolean {
  return ((mask >> mod(pc, 12)) & 1) === 1;
}

/** Flip pitch class `pc` in a 12-bit custom scale mask. */
export function toggleMaskBit(mask: number, pc: number): number {
  return (mask ^ (1 << mod(pc, 12))) & FULL_MASK;
}

/** Build a mask from pitch classes. */
export function maskFromPitchClasses(pcs: readonly number[]): number {
  return pcs.reduce((mask, pc) => mask | (1 << mod(pc, 12)), 0);
}

export function maskBitCount(mask: number): number {
  let count = 0;
  for (let pc = 0; pc < 12; pc++) if (maskHas(mask, pc)) count++;
  return count;
}

const MAJOR_STEPS = [0, 2, 4, 5, 7, 9, 11];
const NATURAL_MINOR_STEPS = [0, 2, 3, 5, 7, 8, 10];

/** Absolute pitch-class mask of a scale in a key (custom returns `customMask`). */
export function scaleMask(
  scale: "chromatic" | "major" | "naturalMinor" | "custom",
  key: number,
  customMask = FULL_MASK,
): number {
  switch (scale) {
    case "chromatic":
      return FULL_MASK;
    case "major":
      return maskFromPitchClasses(MAJOR_STEPS.map((s) => s + key));
    case "naturalMinor":
      return maskFromPitchClasses(NATURAL_MINOR_STEPS.map((s) => s + key));
    case "custom":
      return customMask & FULL_MASK;
  }
}

/**
 * Nearest MIDI note whose pitch class is enabled in `mask` (ties go down).
 * Returns −1 when the mask is empty or the input is not finite.
 */
export function nearestTargetMidi(midi: number, mask: number): number {
  if (!Number.isFinite(midi) || (mask & FULL_MASK) === 0) return -1;
  const base = Math.round(midi);
  for (let offset = 0; offset <= 6; offset++) {
    const candidates =
      midi >= base ? [base - offset, base + offset] : [base + offset, base - offset];
    let best = -1;
    let bestDistance = Infinity;
    for (const candidate of candidates) {
      if (candidate >= 0 && maskHas(mask, candidate)) {
        const distance = Math.abs(candidate - midi);
        if (distance < bestDistance) {
          best = candidate;
          bestDistance = distance;
        }
      }
    }
    if (best >= 0) return best;
  }
  return -1;
}
