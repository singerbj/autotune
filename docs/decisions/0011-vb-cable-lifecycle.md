# 0011 — Managing VB-Cable's install, removal and repair

**Status:** Accepted

VB-Cable worked, but managing it was rough:

- the installer always asked for a reboot after installing it, even when
  the cable already worked;
- Windows often makes a newly installed cable the default playback device,
  so every app played into "CABLE Input" and the user heard nothing;
- removing it while it was a default device (or while "Use for all apps"
  had it as the default mic) left Windows to choose replacements, often the
  wrong ones, and apps such as Discord still pointed at "CABLE Output";
- the installer script ignored the setup program's result, so a failed
  install still recorded `InstalledVBCable=1` and asked for a reboot;
- once installed, the only fixes for a missing or disabled cable were manual
  steps or re-running the installer.

Our own driver (the v2 SYSVAD plan) is still out of scope, so TunedUp
manages VB-Cable more carefully from user mode instead.

## Decision

**Install** (`install-vbcable.ps1 -Action Install`, run by the NSIS hooks):

- `tunedup.exe --cable snapshot` saves every default endpoint (playback
  and recording, all three roles) to the signed-in user's
  `%APPDATA%\<identifier>\cable-install-defaults.json`.
- After `VBCABLE_Setup_x64.exe -i -h`, the driver must be present
  (`Get-PnpDevice`) or the install fails with code 1 and no marker.
- The script waits up to 20 s for both cable endpoints to become active
  (`DeviceState` in the MMDevices registry). If they don't, it restarts the
  Windows Audio services (`AudioEndpointBuilder`, `Audiosrv`) and waits
  again. Only then does it exit `3010` and ask for a reboot; otherwise it
  exits `0`.
- `tunedup.exe --cable settle` puts back every role the install moved onto
  the cable. The app does the same at launch and on every device change,
  as the signed-in user, until 120 s after the cable first appears. That
  covers installs finished by a reboot, and Administrator Protection, where
  the elevated installer may see another account's defaults. A cable
  default the user picks later is left alone.

**Uninstall** (NSIS hooks): the "remove VB-Cable?" question moves to
`NSIS_HOOK_PREUNINSTALL`, while `tunedup.exe` still exists:

- `--cable users <file>` lists apps recording from CABLE Output, and the
  question names them;
- on Yes, `--cable release` moves every default off the cable (to the real
  device another role already uses, else the first real one) before
  `-Action Uninstall` removes the driver in `NSIS_HOOK_POSTUNINSTALL`;
- removal waits for the endpoints to go away (restarting the audio services
  once if needed) and only asks for a reboot when they don't.

**App:**

- The setup check reports `playbackOnCable`; the wizard and the Audio tab
  offer "Switch back to my speakers" (`fix_playback_device`).
- `repair_virtual_mic` turns disabled cable endpoints back on
  (`IPolicyConfig::SetEndpointVisibility`, no elevation needed). If the cable
  is still missing, it runs `install-vbcable.ps1 -Action Repair` elevated
  (one UAC prompt). That installs VB-Cable if needed, otherwise restarts the
  audio services and, as a last resort, runs the setup program again.
- With "Use for all apps" on, the user's own mic comes back as soon as the
  cable disappears, and the cable is the default mic again when it returns.

## Consequences

- Most installs and removals need no reboot. When one is needed, the
  wizard still reopens after it (unchanged `RunOnce` flow).
- Restarting the audio services cuts everyone's sound for a second or two.
  It only happens when the cable didn't come up (or go away) by itself.
- `DeviceState` and endpoint names are read from the MMDevices registry,
  which Windows doesn't document as an API. If that breaks, the cable just
  looks inactive, so the script asks for a reboot (the old behaviour).
- After a removal without a reboot, the driver may stay loaded until the
  next restart. That's harmless; a reinstall in that state waits, restarts
  the audio services and asks for a reboot if it still has to.
- FR-15 now reads "a reboot only when Windows needs one".
