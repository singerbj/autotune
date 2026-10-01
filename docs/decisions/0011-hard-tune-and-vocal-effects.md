# 0011 — Hard tune, formant shift and a routed vocal effects chain

**Status:** Accepted

The tuner corrected pitch accurately, but out of the box it sounded subtle
next to the hard-tuned pop vocals people expect. The tuning defaults are set
for gentle correction: 20 ms retune, chromatic scale, and a 20-cent snap
hysteresis. Record vocals also never reach listeners dry: they get EQ,
compression, doubling, echo and reverb. This ADR adds both (FR-23 – FR-27)
without touching the latency budget.

## Hard tune (FR-23)

A single switch rather than "set retune to 0":

- retune and humanize are treated as 0 whatever the sliders say;
- the snapper's hysteresis is 0, so the note flips exactly at the midpoint
  (the audible "jump" of the effect);
- voicing accepts clarity ≥ 0.70 / stays on down to 0.50 (normally 0.80 /
  0.65), and a voiced note is held through up to 40 ms of failed detections
  while there is still sound. Breathy notes stay corrected instead of
  dropping back to the dry voice mid-word;
- the dry→tuned crossfade at a note onset is 1 ms instead of 4 ms.

Defaults are unchanged, so existing users and the golden files keep their
sound. The built-in styles turn it on.

## Formant shift (FR-24)

PSOLA grains are two source periods long. With a formant ratio `k` ≠ 1 the
grain is resampled to `2·period/k` output samples (linear interpolation),
which scales the spectral envelope by `k`. Grain spacing still sets the
pitch, so the note doesn't move. The range is limited to ±4 semitones so that
at the largest pitch shifts (±7 semitones) neighbouring grains still overlap.
A ratio of exactly 1 takes the original code path (bit-identical output).

## Effects chain (FR-25, FR-26)

`tuner_dsp::fx`, after the dry/wet mix and before the limiter:

- **Inserts, heard everywhere:** a presence peak at 3.5 kHz and an air shelf
  at 10 kHz (RBJ biquads), then a 3 ms / 80 ms peak compressor with half
  makeup gain. They shape the voice itself, so routing them makes little
  sense.
- **Sends, routed:** a two-voice doubler (delays modulated by slow sines, so
  `detune = 2πfA`), an echo synced to BPM × note value with a band-limited
  feedback loop, and a Dattorro plate reverb (pre-delay, four input
  diffusers, figure-eight tank, both output tap sets summed to mono). They
  run in parallel from the insert output, so one computation feeds both
  destinations: `monitor = voice + Σ sends routed to headphones`,
  `cable = voice + Σ sends routed to Discord`.

`Tuner::process_split` writes both outputs; the engine pushes them to the
monitor and cable rings. `Tuner::process` (CLI, tests) applies every send.

Real-time rules hold: all lines are allocated in `Tuner::new`, there is no
trig per sample (rotating-phasor LFOs), and the echo and reverb loops carry a
tiny offset so tails never become denormals. Every effect is exactly
transparent at its default, and a send at rest on level 0 is skipped
entirely. When it is turned up again its lines are cleared first, so no stale
audio plays. Benchmarks on the dev container: plain tuning ≈ 17 µs per
128-sample block (unchanged), a full style with formant and split outputs
≈ 50 µs, against the 270 µs NFR-03 budget.

Parameters travel to the audio thread like the others: `FxParams` encodes to
18 `u32` words held in atomics in `SharedControls`. The UI sends
field-by-field `FxPatch`es. Saved configs and presets from before this change
get the new fields at their neutral defaults on load.

## Styles (FR-27)

`tuner_dsp::Style` holds four built-in sounds: *Chrome Snap* (hard-tuned
robotic croon), *Velvet Echo* (smooth fast tune, doubled), *Night Drive*
(hard tune, heavy compression, bright, short plate) and *Natural*. A style
sets the tuning feel, formant and effects but keeps key, voice range, gate,
bypass, echo tempo and routes. Snapping styles move a chromatic scale to
major in the same key, because the jumps between scale notes are what make
the effect audible. They are named after the sound, not after artists.
