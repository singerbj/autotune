import { describe, expect, it } from "vitest";

import {
  centsToFraction,
  clampCents,
  dbToFraction,
  hzToMidi,
  hzToNoteName,
  maskBitCount,
  maskFromPitchClasses,
  maskHas,
  midiToHz,
  midiToNoteName,
  nearestTargetMidi,
  scaleMask,
  toggleMaskBit,
} from "./music";

describe("music math (FR-06, FR-17)", () => {
  it("names MIDI notes with octave", () => {
    expect(midiToNoteName(69)).toBe("A4");
    expect(midiToNoteName(60)).toBe("C4");
    expect(midiToNoteName(57)).toBe("A3");
    expect(midiToNoteName(61)).toBe("C#4");
    expect(midiToNoteName(0)).toBe("C-1");
    expect(midiToNoteName(59.6)).toBe("C4");
  });

  it("treats −1 / non-finite MIDI as no note", () => {
    expect(midiToNoteName(-1)).toBeNull();
    expect(midiToNoteName(Number.NaN)).toBeNull();
  });

  it("converts Hz to MIDI and back", () => {
    expect(hzToMidi(440)).toBeCloseTo(69, 10);
    expect(hzToMidi(220)).toBeCloseTo(57, 10);
    expect(hzToMidi(261.6256)).toBeCloseTo(60, 3);
    expect(hzToMidi(0)).toBeNull();
    expect(hzToMidi(-5)).toBeNull();
    expect(midiToHz(69)).toBeCloseTo(440, 10);
    expect(midiToHz(hzToMidi(123.4) ?? 0)).toBeCloseTo(123.4, 8);
  });

  it("names frequencies by nearest note", () => {
    expect(hzToNoteName(220)).toBe("A3");
    expect(hzToNoteName(225)).toBe("A3"); // +39¢
    expect(hzToNoteName(227)).toBe("A#3"); // past the +50¢ boundary (~226.5 Hz)
    expect(hzToNoteName(233.08)).toBe("A#3");
    expect(hzToNoteName(0)).toBeNull();
  });

  it("maps dBFS to a 0..1 bar over −60..0 dB", () => {
    expect(dbToFraction(0)).toBe(1);
    expect(dbToFraction(-60)).toBe(0);
    expect(dbToFraction(-30)).toBeCloseTo(0.5);
    expect(dbToFraction(-90)).toBe(0);
    expect(dbToFraction(6)).toBe(1);
    expect(dbToFraction(-Infinity)).toBe(0);
    expect(dbToFraction(Number.NaN)).toBe(0);
  });

  it("clamps cents to ±100 and maps to a needle position", () => {
    expect(clampCents(42)).toBe(42);
    expect(clampCents(250)).toBe(100);
    expect(clampCents(-250)).toBe(-100);
    expect(clampCents(Infinity)).toBe(100);
    expect(clampCents(Number.NaN)).toBe(0);
    expect(centsToFraction(0)).toBe(0.5);
    expect(centsToFraction(-100)).toBe(0);
    expect(centsToFraction(100)).toBe(1);
    expect(centsToFraction(500)).toBe(1);
  });

  it("toggles custom mask bits (bit n = pitch class n, C = 0)", () => {
    expect(toggleMaskBit(0, 0)).toBe(1);
    expect(toggleMaskBit(0, 11)).toBe(0x800);
    expect(toggleMaskBit(0b101, 2)).toBe(0b001);
    expect(toggleMaskBit(0, 13)).toBe(0b10); // wraps to C#
    expect(maskHas(0xab5, 0)).toBe(true);
    expect(maskHas(0xab5, 1)).toBe(false);
    expect(maskBitCount(0xab5)).toBe(7);
    expect(maskFromPitchClasses([0, 2, 4, 5, 7, 9, 11])).toBe(0xab5);
  });

  it("builds scale masks in a key", () => {
    expect(scaleMask("major", 0)).toBe(0xab5);
    expect(scaleMask("chromatic", 5)).toBe(0xfff);
    // A natural minor has the same notes as C major.
    expect(scaleMask("naturalMinor", 9)).toBe(0xab5);
    expect(scaleMask("custom", 3, 0x3)).toBe(0x3);
  });

  it("finds the nearest in-scale target note", () => {
    const cMajor = 0xab5;
    expect(nearestTargetMidi(60.2, cMajor)).toBe(60);
    expect(nearestTargetMidi(61.4, cMajor)).toBe(62); // C# → D (closer)
    expect(nearestTargetMidi(60.6, cMajor)).toBe(60); // C# side → C (closer than D)
    expect(nearestTargetMidi(60, 0)).toBe(-1);
    expect(nearestTargetMidi(Number.NaN, cMajor)).toBe(-1);
  });
});
