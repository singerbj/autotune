// The app's version: one source (the Cargo manifest), mirrored in Cargo.lock and any
// package.json / package-lock.json the config lists. The Prepare release workflow runs it; it
// works locally too.
//
//   node scripts/release/version.ts get
//   node scripts/release/version.ts bump <patch|minor|major|prepatch|preminor|premajor|prerelease> [preid]
//   node scripts/release/version.ts set <x.y.z[-pre.n]>
//   node scripts/release/version.ts tag            the release tag for the current version
//   node scripts/release/version.ts previous-tag   the release before it (release notes start there)
//
// `bump` and `set` rewrite every file and print the new version; they refuse a version that
// isn't newer. Bumps follow npm's `semver.inc`: `patch` on 2.1.0-beta.2 releases 2.1.0,
// `prerelease` on 2.0.0 gives 2.0.1-beta.0, and on 2.0.1-beta.0 gives 2.0.1-beta.1. With a
// changelog configured, its [Unreleased] notes move under the new version (and must not be empty).
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { config } from "./config.ts";

export type Version = { major: number; minor: number; patch: number; pre: string[] };
export type Bump = "major" | "minor" | "patch" | "premajor" | "preminor" | "prepatch" | "prerelease";

const ROOT = join(fileURLToPath(import.meta.url), "..", "..", "..");
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

class VersionError extends Error {}

function fail(message: string): never {
  throw new VersionError(message);
}

export function parse(version: string): Version {
  const m = SEMVER.exec(version) ?? fail(`not a semver version: ${version}`);
  return { major: +m[1], minor: +m[2], patch: +m[3], pre: m[4] ? m[4].split(".") : [] };
}

export function format({ major, minor, patch, pre }: Version): string {
  return `${major}.${minor}.${patch}${pre.length ? "-" + pre.join(".") : ""}`;
}

/** Semver precedence: negative when a < b. */
export function compare(a: string, b: string): number {
  const x = parse(a);
  const y = parse(b);
  const core = x.major - y.major || x.minor - y.minor || x.patch - y.patch;
  if (core) return core;
  if (!x.pre.length || !y.pre.length) return y.pre.length - x.pre.length;
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const [p, q] = [x.pre[i], y.pre[i]];
    if (p === undefined || q === undefined) return p === undefined ? -1 : 1;
    if (p === q) continue;
    const [pn, qn] = [/^\d+$/.test(p), /^\d+$/.test(q)];
    if (pn && qn) return +p - +q;
    if (pn !== qn) return pn ? -1 : 1;
    return p < q ? -1 : 1;
  }
  return 0;
}

export function bump(version: string, kind: string, preid = "beta"): string {
  const v = parse(version);
  const release = (major: number, minor: number, patch: number): Version => ({ major, minor, patch, pre: [] });
  switch (kind) {
    case "major":
      return format(v.pre.length && !v.minor && !v.patch ? release(v.major, 0, 0) : release(v.major + 1, 0, 0));
    case "minor":
      return format(v.pre.length && !v.patch ? release(v.major, v.minor, 0) : release(v.major, v.minor + 1, 0));
    case "patch":
      return format(v.pre.length ? release(v.major, v.minor, v.patch) : release(v.major, v.minor, v.patch + 1));
    case "premajor":
      return format({ ...release(v.major + 1, 0, 0), pre: [preid, "0"] });
    case "preminor":
      return format({ ...release(v.major, v.minor + 1, 0), pre: [preid, "0"] });
    case "prepatch":
      return format({ ...release(v.major, v.minor, v.patch + 1), pre: [preid, "0"] });
    case "prerelease": {
      if (!v.pre.length) return format({ ...release(v.major, v.minor, v.patch + 1), pre: [preid, "0"] });
      const last = v.pre.at(-1) ?? "";
      const pre = v.pre[0] === preid && /^\d+$/.test(last) ? [...v.pre.slice(0, -1), String(+last + 1)] : [preid, "0"];
      return format({ ...v, pre });
    }
    default:
      fail(`unknown bump: ${kind} (patch, minor, major, prepatch, preminor, premajor or prerelease)`);
  }
}

/** `[package]` or `[workspace.package]` version line of a Cargo manifest. */
const CARGO_VERSION = /(^\[(?:workspace\.)?package\][^[]*?^version\s*=\s*")([^"]+)(")/m;

export function cargoVersion(toml: string): string {
  return CARGO_VERSION.exec(toml)?.[2] ?? fail(`no version in ${config.cargoToml}`);
}

/** Cargo.lock with the version of this repo's own crates (no `source`) set to `version`. */
export function updateLock(lock: string, from: string, version: string, crates: RegExp): string {
  let found = false;
  const updated = lock.replace(
    /(\[\[package\]\]\nname = "([^"]+)"\nversion = ")([^"]+)("\n(?!source))/g,
    (all, head: string, name: string, current: string, tail: string) => {
      if (!crates.test(name) || current !== from) return all;
      found = true;
      return `${head}${version}${tail}`;
    },
  );
  if (!found) fail(`${config.cargoLock} has no ${crates} crate at ${from}`);
  return updated;
}

/** CHANGELOG with the [Unreleased] notes moved under `version`. */
export function rollChangelog(changelog: string, version: string): string {
  const text = changelog.replace(/\r\n/g, "\n");
  const unreleased =
    /^## \[Unreleased\]\n([\s\S]*?)(?=^## \[|(?![\s\S]))/m.exec(text) ??
    fail(`no ## [Unreleased] section in ${config.changelog}`);
  if (!unreleased[1].trim()) fail(`the ${config.changelog} [Unreleased] section is empty: nothing to release`);
  if (text.includes(`## [${version}]`)) fail(`${config.changelog} already has a ${version} section`);
  return text.replace(unreleased[0], `## [Unreleased]\n\n## [${version}]\n${unreleased[1]}`);
}

const read = (path: string) => readFileSync(join(ROOT, path), "utf8");
const write = (path: string, text: string) => writeFileSync(join(ROOT, path), text);

export function current(): string {
  return cargoVersion(read(config.cargoToml));
}

function setVersion(version: string): void {
  const from = current();
  parse(version);
  if (compare(version, from) <= 0) fail(`${version} is not newer than ${from}`);
  // Check everything before writing anything.
  const toml = read(config.cargoToml).replace(CARGO_VERSION, `$1${version}$3`);
  const lock = updateLock(read(config.cargoLock).replace(/\r\n/g, "\n"), from, version, config.crates);
  const changelog = config.changelog ? rollChangelog(read(config.changelog), version) : undefined;
  const packages = config.packageJsons.map((path) => {
    const pkg = JSON.parse(read(path));
    pkg.version = version;
    return [path, JSON.stringify(pkg, null, 2) + "\n"] as const;
  });
  let packageLock: string | undefined;
  if (config.packageLock) {
    const lockJson = JSON.parse(read(config.packageLock));
    lockJson.version = version;
    for (const path of config.packageJsons) {
      const dir = path.replace(/(^|\/)package\.json$/, "");
      const entry = lockJson.packages?.[dir] ?? fail(`${config.packageLock} has no "${dir}" package`);
      entry.version = version;
    }
    packageLock = JSON.stringify(lockJson, null, 2) + "\n";
  }
  write(config.cargoToml, toml);
  write(config.cargoLock, lock);
  if (config.changelog && changelog) write(config.changelog, changelog);
  for (const [path, text] of packages) write(path, text);
  if (config.packageLock && packageLock) write(config.packageLock, packageLock);
}

/**
 * The newest of `tags` below `version`: any release for a pre-release, else the newest full
 * release (so a release's notes cover its betas too).
 */
export function previousTag(tags: string[], version: string): string | undefined {
  const pre = parse(version).pre.length > 0;
  return tags
    .filter((t) => t.startsWith(config.tagPrefix) && SEMVER.test(t.slice(config.tagPrefix.length)))
    .map((t) => t.slice(config.tagPrefix.length))
    .filter((v) => compare(v, version) < 0 && (pre || !parse(v).pre.length))
    .sort(compare)
    .map((v) => config.tagPrefix + v)
    .at(-1);
}

/** `x.y.z` from `x.y.z`, `vx.y.z` or the release tag. */
export function stripTag(value: string): string {
  const v = value.trim();
  return v.startsWith(config.tagPrefix) ? v.slice(config.tagPrefix.length) : v.replace(/^v(?=\d)/, "");
}

function main([command, arg, preid]: string[]): void {
  switch (command) {
    case "get":
      console.log(current());
      return;
    case "tag":
      console.log(`${config.tagPrefix}${current()}`);
      return;
    case "previous-tag": {
      const tags = execFileSync("git", ["tag", "--list", `${config.tagPrefix}*`], { cwd: ROOT, encoding: "utf8" });
      const previous = previousTag(tags.split("\n").filter(Boolean), current());
      if (previous) console.log(previous);
      return;
    }
    case "bump":
    case "set": {
      if (!arg) fail(`usage: version.ts ${command} <${command === "bump" ? "kind" : "version"}>`);
      const next = command === "bump" ? bump(current(), arg, preid || "beta") : stripTag(arg);
      setVersion(next);
      console.log(next);
      return;
    }
    default:
      fail("usage: version.ts get | tag | previous-tag | bump <kind> [preid] | set <version>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`version: ${e instanceof VersionError ? e.message : e}`);
    process.exit(1);
  }
}
