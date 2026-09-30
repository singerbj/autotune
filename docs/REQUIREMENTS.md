# Requirements — Real-Time Voice Tuner (v1)

Build the app described in `ARCHITECTURE.md`, milestone by milestone. Every requirement has an ID so commits, tests, and PRs can cite it.

## Working instructions for Claude Code

Build in milestone order, keep every milestone shippable, and never trade real-time safety for speed of delivery.

- Read `ARCHITECTURE.md` before writing code; it is the source of truth for structure and APIs.
- Work one milestone at a time. A milestone is done only when all its acceptance criteria pass and are covered by tests where testable.
- Cite requirement IDs (for example `FR-07`) in commit messages, test names, and PR descriptions.
- When a requirement conflicts with what the hardware or an API allows, stop and write the conflict into `docs/decisions/` as a short ADR instead of silently diverging.
- Language defaults: Rust for everything below the UI; TypeScript (never plain JavaScript) for the frontend.
- Target Windows 10 22H2+ and Windows 11 x64. Builds and tests must run on GitHub Actions `windows-latest`.

## Functional requirements

All are required for v1 except FR-21, which is a stretch goal.

| ID | Area | Requirement |
| --- | --- | --- |
| FR-01 | Audio | List capture and render devices, flag ASIO drivers and VB-Cable endpoints, and update the list live on device changes. |
| FR-02 | Audio | Capture the selected mic through the fallback chain: ASIO, WASAPI exclusive, IAudioClient3 shared, standard shared. Show the active tier. |
| FR-03 | Audio | Monitor the tuned signal to the selected headphones in low-latency shared mode, with an on/off toggle and volume. Monitoring is silent while tuning is bypassed (ADR 0010). |
| FR-04 | Audio | Render the tuned signal to CABLE Input whenever the engine runs, bridged with an adaptive resampler. |
| FR-05 | Audio | Recover from unplug, replug, and default-device changes within 2 s, with no app restart. |
| FR-06 | Tuning | Pitch-correct in real time to a chosen key (12) and scale: chromatic, major, natural minor, or a custom note mask. |
| FR-07 | Tuning | Retune speed from 0 to 200 ms, and humanize from 0 to 100%. |
| FR-08 | Tuning | Voice range preset Low, Mid, or High sets the lowest tracked pitch to 70, 100, or 150 Hz. |
| FR-09 | Tuning | Unvoiced audio passes through unshifted; a noise gate has an adjustable threshold. |
| FR-10 | Tuning | Dry/wet mix, and a click-free bypass (at least 5 ms crossfade) from the UI, tray, and a global hotkey that can be changed while the app runs. Tuning starts bypassed at launch (ADR 0010). |
| FR-11 | Tuning | Parameter changes apply live, smoothed over at least 10 ms, with no clicks. |
| FR-12 | Setup | First-run wizard: choose mic and headphones, detect VB-Cable and conflicts, warn about sidetone and Bluetooth, walk through the Discord checklist, run a latency test. |
| FR-13 | Setup | Detect a Discord capture session on CABLE Output and show its status. |
| FR-14 | Setup | "Use for all apps" sets CABLE Output as the default recording device (console and communications roles) and restores the previous defaults on quit and after a crash. |
| FR-15 | Setup | The installer silently installs VB-Cable when it's missing, with one UAC prompt and one reboot, then resumes the wizard after reboot. |
| FR-16 | Diagnostics | Show estimated latency live in the header; a "Measure" button runs an acoustic loopback test and reports ms. |
| FR-17 | Diagnostics | Meters at 30 Hz: input level, detected pitch, target note, correction in cents. |
| FR-18 | Diagnostics | Diagnostics panel with backend tier, periods, xruns, and callback p99/max, plus a "Copy diagnostics" button. |
| FR-19 | Shell | Tray icon, close-to-tray, single instance, optional launch at login. |
| FR-20 | Shell | Settings persist across restarts in a versioned config schema. |
| FR-21 | Shell | Save and load named tuning presets (stretch). |
| FR-22 | Shell | Auto-update through a signed updater. |

## Non-functional requirements

Latency and stability are the product; every target here has a named way to verify it.

| ID | Requirement | Verified by |
| --- | --- | --- |
| NFR-01 | Monitor round trip of 20 ms or less at the Mid preset on a wired USB headset that supports \~3 ms periods; 15 ms or less on an ASIO interface. | FR-16 acoustic loopback test on the hardware matrix |
| NFR-02 | Zero xruns in a 60-minute session at default settings on the reference PC. | Diagnostics counters during a soak run |
| NFR-03 | One 128-sample block processes in under 0.27 ms (10% of its duration at 48 kHz). | `criterion` benchmark gate in CI |
| NFR-04 | Whole app uses under 5% of one CPU core while running with the window hidden. | Task Manager / ETW sample on the reference PC |
| NFR-05 | Working set under 200 MB including WebView2. | Task Manager on the reference PC |
| NFR-06 | Corrected output lands within ±5 cents of the target note on synthetic tones. | DSP unit tests |
| NFR-07 | Engine goes from start command to audible audio in under 1 s. | Integration test with timestamps |
| NFR-08 | No changed default device survives app exit or a crash. | Test that kills the process, then relaunches |
| NFR-09 | A first-time user goes from installer to tuned voice in Discord in under 5 minutes, reboot included. | Manual usability run |

The reference PC spec is still to be defined (see Open questions in `ARCHITECTURE.md`).

## Hard engineering rules

These are non-negotiable; a PR that breaks one is rejected even if features work.

**Real-time safety (audio threads)**

- No allocation, locks, logging, file or network I/O, or blocking syscalls. The only waits allowed are on the stream's own WASAPI/ASIO event.
- Cross-thread data moves only through `rtrb`, `triple_buffer`, or atomics.
- All buffers are sized at stream start for the worst case (lowest pitch, largest period).
- No panics: no `unwrap()` or `expect()` in `tuner-dsp`, `tuner-audio`, or `tuner-engine` outside tests.
- Debug builds run audio callbacks under `assert_no_alloc`.

**Rust conventions**

- Errors are typed with `thiserror`; the Tauri layer maps them to user-facing messages.
- `unsafe` appears only in `tuner-audio` and `tuner-win` FFI code, each block with a `// SAFETY:` comment.
- Undocumented Windows APIs (`IPolicyConfig`, `IAudioPolicyConfigFactory`) live only in `tuner-win`, behind a trait, and every call has a fallback path.
- The DSP is deterministic: the same input gives the same output. Humanize uses a seeded RNG.
- `rustfmt` and `cargo clippy -- -D warnings` pass on every commit.

**Frontend conventions**

- Strict TypeScript, no `any`; oxlint and oxfmt clean.
- TypeScript command and event bindings are generated by `tauri-specta`, never written by hand.
- The UI holds no source of truth: it renders state from Rust and sends commands.

## Milestones

Seven milestones, in build order. DSP is proven offline before it touches live audio, and live audio is proven clean before the DSP goes in.

| # | Scope | Covers | Done when |
| --- | --- | --- | --- |
| M0 | Scaffold: Cargo workspace, Tauri 2 with React + TypeScript + Vite, `tauri-specta` bindings, CI | — | CI is green on `windows-latest`; the window opens; a typed `ping` command round-trips |
| M1 | Offline DSP: `tuner-dsp` and `tuner-cli` | FR-06–FR-09, FR-11, NFR-03, NFR-06 | The CLI tunes a WAV with key, scale, and speed flags; unit, golden-file, and benchmark gates pass |
| M2 | Audio passthrough: WASAPI capture fallback chain, monitor output, engine threads, no DSP | FR-01–FR-03, FR-05 | Passthrough runs 60 min with zero xruns; the active tier shows in the UI; unplug and replug recover in under 2 s |
| M3 | Live tuning: DSP in the engine, live params, meters, bypass | FR-10, FR-11, FR-16 (estimate), FR-17, NFR-01 | Tuned monitoring works live; NFR-01 measured on at least one USB headset; no allocations under `assert_no_alloc` |
| M4 | Virtual mic: cable writer and resampler, VB-Cable detection and conflict check, default-device control with restore, Discord session detection | FR-04, FR-13, FR-14, NFR-08 | Discord hears the tuned voice for 60 min with no drift glitches; the kill-and-relaunch test restores defaults |
| M5 | Setup and shell: wizard, tray, hotkey, single instance, autostart, config, diagnostics panel, measure button | FR-12, FR-16, FR-18–FR-20 | A fresh profile completes the wizard end to end; settings survive restart |
| M6 | Distribution and ASIO: NSIS installer with VB-Cable hooks, signing, updater, ASIO backend behind the `asio` feature | FR-02 (ASIO tier), FR-15, FR-22, NFR-09 | A clean Windows VM goes from installer to tuned voice in Discord in under 5 minutes |

FR-21 (presets) can land after M5 if time allows.

## Out of scope for v1

Do not build these, even partially, without a new requirement.

- A custom virtual audio driver (SYSVAD-based); VB-Cable only.
- Exclusive-mode headphone output and in-app mixing of system audio ("pro mode").
- Per-app audio routing via `IAudioPolicyConfigFactory`.
- macOS, Linux, and Windows on Arm builds.
- VST/CLAP plugin builds, recording, harmonies, and MIDI control.
- Accounts, telemetry servers, or any network calls besides the updater.
