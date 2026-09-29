# 0009 — Elevated as another account (Administrator Protection)

**Status:** Accepted

The installer runs elevated and the architecture assumes that elevated code
sees the signed-in user's `HKCU`, `%APPDATA%` and `%LOCALAPPDATA%`. That only
holds for plain UAC. Windows 11's Administrator Protection runs elevated
programs as a hidden admin account, and over-the-shoulder UAC (another admin
types their password) runs them as that admin. No Drama Llama hit this in the
field: its elevated WebView2 windows failed with "Microsoft Edge can't read
and write to its data directory", because WebView2 drops elevation to the
signed-in user, who can't write the other account's profile folder.

For TunedUp that meant:

- the post-reboot `RunOnce` entry went to the hidden account's `HKCU` and
  never ran, so the wizard didn't reopen after the VB-Cable reboot;
- the uninstaller's `--restore-defaults` read the hidden account's config,
  so it couldn't find the user's routing backup (NFR-08);
- the uninstaller removed the hidden account's autostart entry, leaving the
  user's `Run\TunedUp` pointing at a deleted exe;
- an elevated launch ("Run as administrator") used a fresh config and a
  WebView2 profile the user can't write.

Decision:

- `tuner_win::other_signed_in_user()` detects the case as no-drama-llama does:
  the process is elevated and either its linked token or the Windows
  session's user (`WTSQuerySessionInformation`) is another account.
- The app then uses that user's `%APPDATA%` / `%LOCALAPPDATA%` for its
  config, logs and WebView2 profile (the main window is created in `setup`
  with `create: false` in `tauri.conf.json` so its data directory can be set).
- The NSIS hooks write and remove per-user registry values through
  `installer/resources/user-registry.ps1`, which uses `HKEY_USERS\<SID>` of
  the owner of the session's `explorer.exe`, falling back to `HKCU`.
- Launches from the installer (finish page, `/R` after an update) already go
  through Tauri's `nsis_tauri_utils::RunAsUser`, so they run as the user.
- CI starts the installed app as the runner's user and elevated as a second
  admin account and checks the window opens with its profile and logs in the
  signed-in user's folders (`scripts/app-window-e2e.ps1`).
