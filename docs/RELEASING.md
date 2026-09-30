# Releasing and auto-update

No Drama Llama, rekt clipz and TunedUp release, sign and update the same way: the same
workflows, the same [`scripts/release/`](../scripts/release/) (only `config.ts` differs), the
same settings names and the same updater rules. Change one, change all three.

A release is a GitHub release tagged `vX.Y.Z` (the `[workspace.package] version` in
`Cargo.toml`, mirrored in `Cargo.lock`, `package.json`, `ui/package.json` and
`package-lock.json`) holding:

| File | What it is |
| --- | --- |
| `tunedup-setup.exe` | The NSIS installer (per machine; see [installer/README.md](../installer/README.md)). Authenticode-signed, with the app and uninstaller inside it, once a certificate is set up. |
| `tunedup-setup.exe.minisig` | Its minisign signature. The trusted comment names the version (Tauri's format, `timestamp:…\tfile:…\tversion:X.Y.Z`). |
| `latest.json` | The updater manifest in Tauri's format: version, notes, and the installer's URL and signature. Installed apps read it. |
| `SHA256SUMS` | Checksums of the three files above. |

## Cutting a release

First run through [`HARDWARE_TEST.md`](HARDWARE_TEST.md) on a real PC with a USB headset and
Discord. Then **Actions → Prepare release → Run workflow** on `main`. Pick the bump (`patch`, `minor`,
`major`, or a `pre*` bump for a beta) or type an exact version, and tick *Dry run* to see the
diff without pushing. After you approve the `release-prep` environment it:

1. bumps the version everywhere ([`scripts/release/version.ts`](../scripts/release/version.ts);
   npm's rules: `patch` on 1.2.0-beta.1 releases 1.2.0, `prerelease` on 1.1.0 gives
   1.1.1-beta.0).
2. commits `chore(release): vX.Y.Z` to `main` and pushes the tag `vX.Y.Z` in one atomic push,
   so the tag only exists if `main` took the commit.
3. starts **Release** on the tag.

**Release** then runs three jobs:

1. **Version**: the tag must match `Cargo.toml` and point at a commit on `main`.
2. **Build** (Windows, on the `codesign` environment): runs the tests and builds the installer
   with the updater's public key baked into the app. With a certificate set up (below), Tauri
   Authenticode-signs the app, the uninstaller and the installer while it bundles, and the job
   checks the signatures are valid and timestamped. The NSIS bundle is why code signing isn't a
   job of its own here, as it is for the single-exe apps.
3. **Publish** (waits for approval on the `release` environment): signs the installer for the
   updater and checks the signature against `UPDATE_PUBKEY`, writes `latest.json` (notes generated from
   the PRs since the previous release) and `SHA256SUMS`, uploads everything to a draft release,
   downloads it again to check the checksums, and publishes it. Versions with a pre-release
   part (`1.2.0-beta.1`) are published as GitHub pre-releases and never become *latest*.

By hand instead: `node scripts/release/version.ts bump patch` (or `set 1.2.3`), commit, and
push the tag `node scripts/release/version.ts tag` prints. To retry a release that failed, run
**Release** on its tag (*Use workflow from → Tags*). A release that's already published is
never changed: release a new version instead.

## How installed apps update

With *Update automatically* on (Settings), the app reads
`https://github.com/singerbj/tunedup/releases/latest/download/latest.json` a minute after it
starts and every 6 hours (30 minutes after a failed check); *Check for updates* is always there.
`tauri-plugin-updater` downloads a release in the background only if:

1. it's newer than the running version and not a pre-release (those are never *latest*);
2. the installer's minisign signature checks out against the public key compiled into the app
   (`TUNEDUP_UPDATE_PUBKEY`, from the `UPDATE_PUBKEY` variable);
3. the signature names that exact version (`requireSignedVersion`), so a validly signed old
   build can't be passed off as a new one.

The header and tray then show "Update ready". Installing (the button, the tray, or quitting)
restores the user's audio defaults and runs the installer in passive mode with `/UPDATE` (no
VB-Cable step, no reboot, settings kept), then relaunches. Builds without the key never update.

## One-time setup

### Updater signing key

Run this on your own machine, with the [GitHub CLI](https://cli.github.com) logged in as a
repository admin:

```sh
node scripts/release/setup-secrets.ts
```

It creates a password-protected key pair in `~/.release-keys/` (with `tauri signer generate`,
or pass `--key <file>` to use one you have), and stores:

| Where | Name | Value |
| --- | --- | --- |
| Repository variable | `UPDATE_PUBKEY` | the public key (any minisign format) |
| `release` environment secret | `UPDATE_SIGNING_KEY` | the secret key (minisign or `tauri signer` format) |
| `release` environment secret | `UPDATE_SIGNING_KEY_PASSWORD` | its password |

It also creates the `release-prep`, `codesign` and `release` environments, limited to `main`
or to `v*` tags, with you as a required reviewer of `release-prep` and `release`.

Back up the key file and its password. The public key is compiled into every build, so if the
key is lost, installed copies can never update themselves again, and a new key only reaches
users who install a new build by hand.

Then, under **Settings → Rules → Rulesets**, add a tag ruleset for `v*` that restricts
creation, update and deletion to administrators (and the release App below), protect `main`,
and require the **CI success** check.

### Release App (optional)

Without it, Prepare release pushes with `GITHUB_TOKEN`, which needs Actions to be allowed to
push to `main` and can't bypass rulesets. With a GitHub App it can:

1. Create a GitHub App (**Settings → Developer settings → GitHub Apps**) with no webhook and only
   the repository permission **Contents: Read and write**, and install it on this repository.
2. Add the repository variable `RELEASE_APP_CLIENT_ID` (its client ID) and the `release-prep`
   environment secret `RELEASE_APP_PRIVATE_KEY` (a private key it generates).
3. Add the App to the bypass list of the `v*` tag ruleset and of whatever protects `main`.

### Code signing (optional)

Without an Authenticode signature, SmartScreen shows "Windows protected your PC" when the
installer runs. Add the `codesign` environment secrets `WINDOWS_CERTIFICATE` (the base64 of the
`.pfx`) and `WINDOWS_CERTIFICATE_PASSWORD`; Tauri signs with SHA-256 and a DigiCert timestamp
(`bundle.windows` in `src-tauri/tauri.conf.json`). SignPath can't sign inside the NSIS bundle,
so setting `SIGNPATH_ORGANIZATION_ID` fails the release.

### VB-Cable (optional)

Set the repository variable `BUNDLE_VBCABLE=true` once VB-Audio's bundle license is in place
([installer/README.md](../installer/README.md#bundling-vb-cable)).

## Moving from the old settings

The updater key is the same; only the names changed. Until you move them, releases still read
the old names and warn. Store the same values under the new names
(`setup-secrets.ts --key ~/.tauri/tunedup.key` does it), then delete the old ones:

| Old | New |
| --- | --- |
| variable `TAURI_UPDATER_PUBKEY` | variable `UPDATE_PUBKEY` |
| secret `TAURI_SIGNING_PRIVATE_KEY` | `release` environment secret `UPDATE_SIGNING_KEY` |
| secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `release` environment secret `UPDATE_SIGNING_KEY_PASSWORD` |
| secrets `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | the same, on the `codesign` environment (repository secrets still work) |

The installer is now published as `tunedup-setup.exe` (a stable name, so
`releases/latest/download/tunedup-setup.exe` always works) instead of
`TunedUp_X.Y.Z_x64-setup.exe`. Installed copies read the download URL from `latest.json`, so
they update either way.

## Rolling back

Installed apps never downgrade. To stop a bad release from spreading, mark it a pre-release
(or delete it) on GitHub so the previous release is *latest* again, then release a fixed,
higher version.
