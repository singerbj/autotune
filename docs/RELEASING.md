# Releasing and auto-update

Installed apps update themselves from GitHub Releases (FR-22):

1. On launch (+20 s) and every 6 h the app fetches
   `https://github.com/singerbj/tunedup/releases/latest/download/latest.json`.
2. If a newer version is listed it downloads the NSIS installer in the
   background and verifies its **minisign signature** against the public key
   compiled into the app.
3. The header and tray show "Update ready". Installing (button, tray, or
   simply quitting the app) restores the user's audio defaults, then runs the
   installer in passive mode with `/UPDATE` (no VB-Cable step, no reboot,
   settings kept) and relaunches.

Automatic checks can be turned off in Settings; "Check for updates" is always
available. Pre-releases are never marked "latest", so they are not offered to
installed apps.

## One-time setup

**Shortcut:** with the GitHub CLI logged in, run
`scripts/setup-release-secrets.sh` on your own machine. It generates the key
pair locally and sets the variable and both secrets below. Then back up
`~/.tauri/tunedup.key`.

Manual steps:

1. Generate the updater signing key (keep the private key safe — losing it
   means installed apps can never update again):

   ```sh
   npx tauri signer generate -w ~/.tauri/tunedup.key
   ```

2. In the GitHub repository settings:

   | Kind | Name | Value |
   | --- | --- | --- |
   | Variable | `TAURI_UPDATER_PUBKEY` | contents of `tunedup.key.pub` |
   | Secret | `TAURI_SIGNING_PRIVATE_KEY` | contents of `tunedup.key` |
   | Secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the key password (if any) |
   | Secret *(optional)* | `WINDOWS_CERTIFICATE` | base64 of the Authenticode `.pfx` |
   | Secret *(optional)* | `WINDOWS_CERTIFICATE_PASSWORD` | `.pfx` password |
   | Variable *(optional)* | `BUNDLE_VBCABLE` | `true` once VB-Audio's bundle license is in place |

   Without the certificate the installer is unsigned and SmartScreen warns.

3. Protect `main` and require the **CI success** check.

## Before a release

Run through [`HARDWARE_TEST.md`](HARDWARE_TEST.md) on a real PC with a
USB headset and Discord.

## Cutting a release

**Preferred:** Actions → **Release** → *Run workflow* → enter `1.2.3`
(tick *pre-release* for betas). The workflow bumps every version
(`scripts/set-version.sh`), commits `chore(release): v1.2.3`, tags, builds
and signs the installer, creates the release with generated notes, uploads
the installer, its `.sig` and `latest.json`, verifies them and publishes.

**Alternative:** bump locally with `scripts/set-version.sh 1.2.3`, commit,
then `git tag v1.2.3 && git push --follow-tags`. The workflow refuses tags
that don't match `Cargo.toml`.

## Rolling back

Delete (or mark as pre-release) the bad release so the previous one becomes
"latest", then publish a fixed higher version — the updater only moves
forward.
