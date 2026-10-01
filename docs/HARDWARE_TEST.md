# Hardware test checklist

Automated CI has no audio devices, so these checks need a real Windows PC.
Record results in the release PR (or an issue) with the "Copy diagnostics"
report attached.

## Get a build

- Latest CI installer: open the newest green **CI** run → *Artifacts* →
  `installer-<sha>` (unsigned; SmartScreen: *More info → Run anyway*), or
- `npm install && npm run build` on the Windows PC
  (`target/release/bundle/nsis/`).

## 1. Install (FR-15, NFR-09)

- [ ] Start a timer. Run the installer; exactly one UAC prompt.
- [ ] If VB-Cable was missing: usually no reboot is offered and the cable
      works right away. If one is offered, after it the app opens in the
      setup wizard.
- [ ] Sound settings: the default speakers and mic are what they were
      before the install (not CABLE Input / CABLE Output).
- [ ] Wizard shows VB-Cable installed and no conflicts (or the expected ones).
- [ ] Stop the timer when Discord hears you tuned (step 4). Target < 5 min.

## 2. Latency (NFR-01, FR-16)

Headset: ______________________ (wired USB, ~3 ms periods)

- [ ] Header shows the capture tier (expect *WASAPI exclusive*) and an estimate.
- [ ] Diagnostics → *Measure latency* with an earcup on the mic.
      Result: ______ ms. Target ≤ 20 ms at Mid (≤ 15 ms on an ASIO interface).
- [ ] Or headless: `cargo test -p tuner-engine --release --test hardware -- --ignored --nocapture latency`

## 3. Stability (NFR-02, NFR-04, NFR-05)

- [ ] `set TUNER_SOAK_SECS=3600` then
      `cargo test -p tuner-engine --release --test hardware -- --ignored --nocapture soak`
      → prints xruns every 5 s; passes only with 0 xruns.
- [ ] Or run the app for 60 min singing/talking; Diagnostics xruns stay 0.
- [ ] Window hidden: Task Manager CPU < 5 % of one core, memory < 200 MB.

## 4. Discord (FR-13, FR-04)

- [ ] Discord → Voice: input *Default* or *CABLE Output*; noise suppression,
      echo cancellation and AGC off.
- [ ] Wizard/Setup shows the green "Discord detected" check.
- [ ] Discord *Mic Test* (or a friend) hears the tuned voice; toggling bypass
      (UI, tray, hotkey Ctrl+Alt+B) is audible and click-free.
- [ ] 60 min call: no drift glitches (Diagnostics cable underruns stay 0).

## 5. Use for all apps (FR-14, NFR-08)

- [ ] Turn on *Use for all apps*: Windows Sound settings now show
      *CABLE Output* as the default recording device (both default and
      communications).
- [ ] Quit from the tray: the previous defaults are back.
- [ ] Turn it on again, then kill `tunedup.exe` in Task Manager; relaunch:
      previous defaults are restored.

## 6. Device changes (FR-05)

- [ ] Unplug the headset while running: the app recovers on another device
      within 2 s; replug: it switches back within 2 s.

## 7. Repair (ADR 0012)

- [ ] Sound settings → disable CABLE Output: the Audio tab shows "Turn
      VB-Cable back on"; it works without a UAC prompt.
- [ ] Set CABLE Input as the default speakers: the Audio tab warns; "Switch
      back to my speakers" restores the previous speakers.
- [ ] Remove VB-Cable with its own setup (`-u`), no reboot: "Install
      VB-Cable" reinstalls it with one UAC prompt.

## 8. Uninstall

- [ ] With Discord on CABLE Output and *Use for all apps* on, uninstall from
      Settings → Apps: default mic restored, the VB-Cable prompt (only if we
      installed it) names Discord.
- [ ] Answer Yes: afterwards the default speakers and mic are real devices,
      usually with no reboot asked for. App and shortcuts gone.
