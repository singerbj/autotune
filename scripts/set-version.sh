#!/usr/bin/env bash
# Set the app version everywhere it lives (single source: Cargo workspace).
# Usage: scripts/set-version.sh 1.2.3
set -euo pipefail
version="${1:?usage: set-version.sh <semver>}"
version="${version#v}"
if ! echo "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
  echo "not a semantic version: $version" >&2
  exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Cargo workspace (Tauri reads the app version from src-tauri's Cargo.toml).
sed -i.bak -E '/^\[workspace\.package\]/,/^\[/ s/^version = "[^"]*"/version = "'"$version"'"/' Cargo.toml
rm -f Cargo.toml.bak
cargo update --workspace --quiet

# npm workspace + lockfile.
npm version "$version" --no-git-tag-version --allow-same-version --workspaces --include-workspace-root >/dev/null

echo "version set to $version"
