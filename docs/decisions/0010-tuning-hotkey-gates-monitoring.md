# 0010 — The tuning hotkey turns tuning and monitoring on together, off at launch

**Status:** Accepted

FR-10 asks for a click-free bypass from the UI, tray and a global hotkey, and
FR-03 for a headphone monitor with its own on/off toggle. Used as a live
effect, the useful switch is one key that does both: by default nothing is
tuned and nothing of your voice plays in your headphones; pressing the hotkey
turns tuning on and starts monitoring, pressing it again turns both off.

- **Off at launch.** `AppState::new` forces `params.bypass = true` whatever was
  saved, so every start is off. The persisted value no longer matters.
- **Monitoring follows the switch.** `SharedControls::monitor_gain` is 0 while
  bypassed; the capture thread already ramps gain changes, so muting is
  click-free. The FR-03 "Monitor in headphones" setting and volume still apply
  while tuning is on. The latency test plays its chirp at unity regardless.
- **The virtual mic keeps the dry voice.** VB-Cable still receives the
  (bypassed, dry) signal while off, so a call doesn't drop out; only tuning
  changes.
- **Hotkey changes apply live**, following the Rekt Clipz design: the new key
  is registered before the old one is dropped, so an invalid or taken key is
  rejected with the old one still working and nothing saved. At startup a
  configured key that can't be registered falls back to
  `CommandOrControl+Alt+B`, and the problem shows under the setting.
  Settings records the key by capture ("Change…", then press the keys).
- Internally the flag keeps its `bypass` name (DSP, config and events); the UI
  and tray call it "Tuning" on/off. `bypassHotkey` keeps its config key, so no
  migration is needed.
