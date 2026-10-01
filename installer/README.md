# Installer

One per-machine NSIS installer produced by the Tauri bundler, with hooks
that set up VB-Cable (FR-15, ADR 0012). The user sees one UAC prompt and,
only when a freshly installed VB-Cable doesn't come up without one, a reboot
after which the app opens in the setup wizard.

| File | Purpose |
| --- | --- |
| `nsis/hooks.nsh` | `NSIS_HOOK_POSTINSTALL`, `NSIS_HOOK_PREUNINSTALL`, `NSIS_HOOK_POSTUNINSTALL` |
| `resources/install-vbcable.ps1` | Detect / install / repair / uninstall VB-Cable; bundled into `$INSTDIR\installer\` (the app's Repair button runs it too) |
| `resources/user-registry.ps1` | Writes / removes the signed-in user's `RunOnce` and autostart entries |
| `tests/*.Tests.ps1` | Pester tests (CI) |

The installer runs elevated, and its `HKCU` is the elevated account's. Under
Windows 11 Administrator Protection, or when another admin approves the UAC
prompt, that isn't the signed-in user, so per-user registry values go through
`user-registry.ps1`, which uses `HKEY_USERS\<SID>` of the session's user
(see `docs/decisions/0009-elevated-as-another-account.md`).

## Install sequence

1. The app is installed per machine (elevated; `bundle.windows.nsis.installMode = perMachine`).
2. `install-vbcable.ps1 -Action Install -App <tunedup.exe>`:
   - exits `0` if VB-Cable is already present;
   - uses the bundled driver pack in `installer\vbcable\` or downloads it from VB-Audio;
   - verifies the setup program is signed by VB-Audio;
   - adds VB-Audio's publisher certificate to `LocalMachine\TrustedPublisher`;
   - saves the default audio devices (`tunedup.exe --cable snapshot`);
   - runs `VBCABLE_Setup_x64.exe -i -h` and fails (`1`, no marker) if the
     driver isn't installed afterwards;
   - writes `HKLM\Software\TunedUp\InstalledVBCable = 1`;
   - waits up to 20 s for both cable endpoints to be active, restarting the
     Windows Audio services once if they aren't;
   - puts back defaults Windows moved onto the cable (`--cable settle`);
   - exits `0` when the cable works, `3010` when Windows needs a restart.
3. On `3010` the installer sets the reboot flag (the finish page offers
   "Reboot now") and adds a `RunOnce` entry to the signed-in user's hive
   (`user-registry.ps1 -Action SetRunOnce`; `HKCU` if that fails) that starts
   `tunedup.exe --first-run` after the reboot.
4. Updates (`/UPDATE`, run by the auto-updater) skip all of the above.

Set the environment variable `TUNEDUP_SKIP_VBCABLE=1` to skip step 2
(used by the CI install smoke test).

## Uninstall sequence

1. `tunedup.exe --restore-defaults` restores any default recording
   devices changed by "Use for all apps" (NFR-08). Elevated as another
   account, it reads the signed-in user's config.
2. `user-registry.ps1 -Action Cleanup` removes the signed-in user's
   `RunOnce` and autostart (`Run\TunedUp`) entries.
3. If `InstalledVBCable = 1`, the user is asked whether to remove VB-Cable
   too, with the apps still recording from CABLE Output named
   (`tunedup.exe --cable users`); silent/passive uninstalls keep it. On Yes,
   `tunedup.exe --cable release` moves every default device off the cable.
4. Tauri removes the app, shortcuts, the elevated account's autostart entry and (optionally) app data.
5. On Yes, `install-vbcable.ps1 -Action Uninstall` runs `VBCABLE_Setup_x64.exe -u -h`
   and asks for a reboot only if the cable endpoints don't go away (after
   one restart of the audio services).

## Repair (from the app)

The setup wizard and the Audio tab offer "Install / Repair VB-Cable" when the
cable is missing or broken. The app turns disabled cable endpoints back on
itself; if that isn't enough, it runs `install-vbcable.ps1 -Action Repair`
elevated (one UAC prompt). That installs VB-Cable if it's missing, otherwise
restarts the Windows Audio services and, if needed, runs the setup program
again. It exits `0`, or `3010` when Windows needs a restart.

Silent install/uninstall:

```powershell
.\tunedup-setup.exe /S
& "C:\Program Files\TunedUp\uninstall.exe" /S
```

## Bundling VB-Cable

Bundling requires a distribution license from VB-Audio (see
`docs/decisions/0003-vb-cable-distribution.md`). When licensed, set the
repository variable `BUNDLE_VBCABLE=true`; the release workflow then places
the driver pack in `installer/resources/vbcable/` before building.
