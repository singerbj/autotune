# Installer

One per-machine NSIS installer produced by the Tauri bundler, with hooks
that set up VB-Cable (FR-15). The user sees one UAC prompt and, when VB-Cable
had to be installed, one reboot after which the app opens in the setup wizard.

| File | Purpose |
| --- | --- |
| `nsis/hooks.nsh` | `NSIS_HOOK_POSTINSTALL`, `NSIS_HOOK_PREUNINSTALL`, `NSIS_HOOK_POSTUNINSTALL` |
| `resources/install-vbcable.ps1` | Detect / install / uninstall VB-Cable; bundled into `$INSTDIR\installer\` |
| `resources/user-registry.ps1` | Writes / removes the signed-in user's `RunOnce` and autostart entries |
| `tests/*.Tests.ps1` | Pester tests (CI) |

The installer runs elevated, and its `HKCU` is the elevated account's. Under
Windows 11 Administrator Protection, or when another admin approves the UAC
prompt, that isn't the signed-in user, so per-user registry values go through
`user-registry.ps1`, which uses `HKEY_USERS\<SID>` of the session's user
(see `docs/decisions/0009-elevated-as-another-account.md`).

## Install sequence

1. The app is installed per machine (elevated; `bundle.windows.nsis.installMode = perMachine`).
2. `install-vbcable.ps1 -Action Install`:
   - exits `0` if VB-Cable is already present;
   - uses the bundled driver pack in `installer\vbcable\` or downloads it from VB-Audio;
   - verifies the setup program is signed by VB-Audio;
   - adds VB-Audio's publisher certificate to `LocalMachine\TrustedPublisher`;
   - runs `VBCABLE_Setup_x64.exe -i -h`;
   - writes `HKLM\Software\TunedUp\InstalledVBCable = 1` and exits `3010`.
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
3. Tauri removes the app, shortcuts, the elevated account's autostart entry and (optionally) app data.
4. If `InstalledVBCable = 1`, the user is asked whether to remove VB-Cable
   too (`VBCABLE_Setup_x64.exe -u -h`); silent/passive uninstalls keep it.

Silent install/uninstall:

```powershell
.\TunedUp_x.y.z_x64-setup.exe /S
& "C:\Program Files\TunedUp\uninstall.exe" /S
```

## Bundling VB-Cable

Bundling requires a distribution license from VB-Audio (see
`docs/decisions/0003-vb-cable-distribution.md`). When licensed, set the
repository variable `BUNDLE_VBCABLE=true`; the release workflow then places
the driver pack in `installer/resources/vbcable/` before building.
