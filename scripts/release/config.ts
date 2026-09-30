// This repository's release settings. Everything else in scripts/release/ is the same in every
// desktop app (no-drama-llama, rekt-clipz, tunedup): change it there too.
export const config = {
  /** Lowercase name: release asset names, the signing key's file name. */
  slug: "tunedup",
  productName: "TunedUp",
  repo: "singerbj/tunedup",
  /** Release tags are `<tagPrefix><version>`. */
  tagPrefix: "v",
  /** What users download and the updater installs: the NSIS installer. */
  asset: "tunedup-setup.exe",
  /** `latest.json` platform keys the asset is listed under (the updater tries `-nsis` first). */
  platforms: ["windows-x86_64-nsis", "windows-x86_64"],
  /** Holds the version (`[package]` or `[workspace.package]`). */
  cargoToml: "Cargo.toml",
  cargoLock: "Cargo.lock",
  /** This repo's crates in Cargo.lock (they move with the version). */
  crates: /^(tunedup|tuner-[a-z]+)$/,
  packageJsons: ["package.json", "ui/package.json"],
  packageLock: "package-lock.json" as string | undefined,
  /** Keep-a-Changelog file whose [Unreleased] notes each release takes, if any. */
  changelog: undefined as string | undefined,
};
