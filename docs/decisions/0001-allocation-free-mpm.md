# 0001 — Own allocation-free McLeod detector

**Status:** Accepted

## Context

ARCHITECTURE.md names the `pitch-detection` crate's McLeod detector as the
starting point, "vendor and trim it if profiling shows allocation or excess
cost per call". Its `get_pitch` allocates its FFT buffers and planner on
every call, which breaks the hard rule "no allocation on audio threads".

## Decision

`tuner-dsp::PitchDetector` implements MPM (NSDF via FFT autocorrelation,
key-maxima peak picking with k = 0.88, parabolic interpolation) with the FFT
plans (`realfft`, pure Rust) and all buffers created in `new()`. `detect()`
takes the analysis window as two slices so the tuner can pass its ring buffer
without copying.

## Consequences

- `process()` is verified allocation-free under `assert_no_alloc` in tests
  and in a one-hour soak in CI.
- Measured cost at 48 kHz: ~20 µs per 128-sample block for the whole pipeline
  (budget 270 µs, NFR-03).
- Accuracy is covered by NFR-06 tests (±5 cents on sines and harmonic tones).
