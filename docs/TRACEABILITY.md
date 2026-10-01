# Requirement traceability

Where each requirement is implemented and how it is verified. Automated
checks run in CI; hardware checks are part of the per-release manual matrix.

| ID | Implementation | Verification |
| --- | --- | --- |
| FR-01 | `tuner-audio::wasapi::enumerate`, `tuner-win` DeviceWatcher, `devices-changed-event` | `types::tests::fr01_*`, mock backend tests, manual |
| FR-02 | `tuner-audio` fallback chain (`open_with_fallback`, `wasapi::stream`, `asio`) | `backend::tests::fr02_*`, `mock::tests::fr02_*`, `engine_mock::fr02_*` |
| FR-03 | `tuner-engine` monitor ring + `RingReader`, monitor gain (0 while bypassed) | `engine_mock::fr03_*`, `controls::tests::fr03_*` |
| FR-04 | cable render + adaptive resampler (PI on fill) | `resample::tests::fr04_*`, `engine_mock::fr04_*` |
| FR-05 | `tuner-engine::Supervisor` | `engine_mock::fr05_*` (unplug/replug < 2 s) |
| FR-06 | `tuner-dsp::params`, `scale` | `params::tests::fr06_*`, `scale::tests::fr06_*`, `pipeline::fr06_*` |
| FR-07 | retune glide + seeded humanize | `pipeline::fr07_*`, `params::tests::fr07_*` |
| FR-08 | `VoiceRange` | `params::tests::fr08_*`, `pipeline::fr08_*` |
| FR-09 | voicing decision, `NoiseGate` | `filters::tests::fr09_*`, `pipeline::fr09_*` |
| FR-10 | bypass ramp (10 ms), tray, `src-tauri::hotkey` (live change, fallback), `set_bypass`, off at launch | `filters::tests::fr10_*`, `pipeline::fr10_*`, `controls::tests::fr10_*`, `hotkey::tests::fr10_*`, `state::tests::fr10_*`, UI tests |
| FR-11 | smoothers (≥ 10 ms), atomic params | `filters::tests::fr11_*`, `pipeline::fr10_fr11_*`, `engine_mock::fr11_*` |
| FR-12 | setup wizard, `run_setup_check`, `tuner-win::setup` | `setup::tests::fr12_*`, UI wizard tests |
| FR-13 | Discord session detection | `setup::tests::fr13_*`, manual |
| FR-14 | `RouteAllApps` + `IPolicyConfig` | `routing::tests::fr14_*`, manual |
| FR-15 | `installer/` hooks + `install-vbcable.ps1`, `tuner-win::cable`, `src-tauri::cable` (`--cable` helpers, `repair_virtual_mic`, `fix_playback_device`) | Pester tests, `cable::tests::fr15_*`, UI `CableHealth` tests, installer smoke test, manual clean VM |
| FR-16 | estimate in `EngineStatus`; loopback test | `latency::tests::fr16_*`, `engine_mock::fr16_*`, manual |
| FR-17 | `DspMeters`, `meters-event`, canvas meter | `engine_mock::fr02_fr03_fr04_*`, UI tests |
| FR-18 | `CallbackStats`, diagnostics panel/report | `stats::tests::fr18_*`, `diagnostics::tests::fr18_*` |
| FR-19 | tray, close-to-tray, single instance, autostart | manual |
| FR-20 | versioned `AppConfig` + migrations | `config::tests::fr20_*` |
| FR-21 | presets | UI tests, manual |
| FR-22 | `tauri-plugin-updater`, release workflow | release workflow asset verification, manual |
| FR-23 | `TuningParams::hard_tune`, `Tuner::analyze`/`glide`, `NoteSnapper::set_hysteresis` | `scale::tests::fr23_*`, `pipeline::fr23_*`, `styles::tests::fr23_*`, `commands::tests::fr23_*`, UI tests |
| FR-24 | `Psola::overlap_add_resampled`, `formant_semitones` | `psola::tests::fr24_*`, `pipeline::fr24_*`, `params::tests::fr24_*`, UI tests |
| FR-25 | `tuner-dsp::fx` (EQ, compressor, doubler, echo, Dattorro plate), Effects tab | `fx::*::tests::fr25_*`, `filters::tests::fr25_*`, `params::tests::fr25_*`, `config::tests::fr20_fr25_*`, UI tests; `styled_mid` benchmark |
| FR-26 | `FxRoute`, `Tuner::process_split`, engine monitor/cable buffers | `fx::tests::fr26_*`, `params::tests::fr26_*`, `pipeline::fr26_*`, `engine_mock::fr26_*`, UI tests |
| FR-27 | `tuner_dsp::Style`, `list_styles`/`apply_style`, Styles card, CLI `--style` | `styles::tests::fr27_*`, `pipeline::fr27_*`, `commands::tests::fr27_*`, UI tests |
| NFR-01 | latency budget | FR-16 measurement on the hardware matrix |
| NFR-02 | zero xruns in 60 min | diagnostics counters, soak run |
| NFR-03 | 128-sample block < 0.27 ms | `cargo bench` + `scripts/bench-gate.ps1` in CI |
| NFR-04/05 | CPU < 5 % hidden, < 200 MB | hidden control loop at 7.5 Hz, no meter events; manual |
| NFR-06 | ±5 cents | `mpm::tests::nfr06_*`, `pipeline::nfr06_*`, golden files |
| NFR-07 | audio < 1 s after start | `engine_mock::nfr07_*` |
| NFR-08 | no changed default survives crash | `crash_restore::nfr08_*` (kills a child process), `--restore-defaults` in uninstall |
| NFR-09 | < 5 min install → Discord | manual usability run |
