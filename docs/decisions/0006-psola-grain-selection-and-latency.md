# 0006 — PSOLA grain selection and latency

**Status:** Accepted

## Decision

- Output is delayed by `L = ceil(fs / lowest_pitch)` (Low 686, Mid 480,
  High 320 samples at 48 kHz = 14.3 / 10 / 6.7 ms), matching the table in
  the architecture.
- Analysis marks sit on local maxima one detected period apart; each
  synthesis mark uses the nearest *complete* analysis grain (two periods,
  Hann). This keeps the delay at one period but lets the wet signal's phase
  offset vary by up to half a period relative to the dry path.
- The overlap-add window sum is divided out, so level is flat for any ratio.
- Shifts are capped at ±7 semitones (risk: PSOLA artifacts on big shifts).
- Unvoiced audio and bypass use the dry signal delayed by exactly `L`, so
  toggling never changes timing; transitions are crossfaded (4 ms voicing,
  10 ms bypass).
- Changing the voice range fades out for 5 ms, resets the shifter with the
  new latency, and fades back in.
