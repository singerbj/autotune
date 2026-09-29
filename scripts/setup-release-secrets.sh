#!/usr/bin/env bash
# One-time setup for signed releases and auto-update (docs/RELEASING.md).
#
# Generates the updater signing key pair on YOUR machine and stores it in the
# GitHub repository: the public key as the TAURI_UPDATER_PUBKEY variable, the
# private key (and its password) as secrets. The private key never leaves
# your machine except to GitHub's encrypted secret store.
#
# Requirements: node/npm, and the GitHub CLI logged in (`gh auth login`)
# with admin access to the repository.
#
# Usage: scripts/setup-release-secrets.sh [owner/repo]
set -euo pipefail

repo="${1:-singerbj/tunedup}"
key_dir="${HOME}/.tauri"
key="${key_dir}/tunedup.key"

command -v gh >/dev/null || { echo "Install the GitHub CLI: https://cli.github.com" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "Run 'gh auth login' first." >&2; exit 1; }

if [ -e "$key" ]; then
  echo "Using existing key $key"
else
  mkdir -p "$key_dir"
  read -r -s -p "Password for the new signing key (empty for none): " password; echo
  npx --yes @tauri-apps/cli@2 signer generate --ci -w "$key" -p "$password"
  chmod 600 "$key"
  printf '%s' "$password" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo "$repo"
fi

gh variable set TAURI_UPDATER_PUBKEY --repo "$repo" --body "$(cat "$key.pub")"
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo "$repo" < "$key"

cat <<MSG

Done. Stored in $repo:
  variable TAURI_UPDATER_PUBKEY
  secret   TAURI_SIGNING_PRIVATE_KEY
  secret   TAURI_SIGNING_PRIVATE_KEY_PASSWORD

BACK UP $key (and its password) somewhere safe, e.g. a password manager.
If it is lost, installed copies of the app can never update again.
MSG
