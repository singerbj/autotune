// node --test scripts/release/*.test.ts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { config } from "./config.ts";
import { manifest } from "./manifest.ts";
import {
  decodePublicKey,
  decodeSecretKey,
  formatPublicKey,
  generateTestKey,
  scryptParams,
  signData,
  signedVersion,
  trustedComment,
  unwrap,
  verifyData,
} from "./minisign.ts";
import {
  bump,
  cargoVersion,
  compare,
  format,
  parse,
  previousTag,
  rollChangelog,
  stripTag,
  updateLock,
} from "./version.ts";

// `tauri signer generate -p hunter2` and `tauri signer sign` output.
const TAURI_KEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5UmVUSWVrNDNsRkFhWTIwT0tuMzh0aVhvbGJlRDlFdmhzT1BvZXVHanN2QUFBQkFBQUFBQUFBQUFBQUlBQUFBQUhJclRvWXZKZGlZM1ZwZlhDRDZwTHp6QVFjbFJtV1FOWVJPcXFhRWp6SHl3RlduS0E5Q3ZYZjJPdGxIdHcyV3NQUGNaWnJ2c1cxbEhaNGVxd0JyUXhCSnlHZHF1RllQeUNFYzZldm51NDJTZFZNUVZpQlhzKytyZGpSR3Q0eGRRbzdJcXNUTTZKUTQ9Cg==";
const TAURI_PUB =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI3NkVGQjY2RDM1OUU4QzQKUldURTZGblRadnR1SjhsOHF2Tmt4L3dYZ3Y4Z3padXd0SnUwaXU0bi9XQ2RacURuYlNwZXJacVAK";

describe("minisign", () => {
  it("decrypts a tauri key and signs with it", () => {
    const secret = decodeSecretKey(TAURI_KEY, "hunter2");
    const pub = decodePublicKey(TAURI_PUB);
    assert.ok(secret.publicKey.equals(pub.key));
    const data = Buffer.from("update payload");
    const signature = signData(data, secret, "version:2.1.0");
    assert.equal(verifyData(data, signature, pub).trustedComment, "version:2.1.0");
    // Tauri's encoding (base64 of the text) verifies too.
    assert.equal(verifyData(data, Buffer.from(signature).toString("base64"), pub).trustedComment, "version:2.1.0");
    assert.throws(() => decodeSecretKey(TAURI_KEY, "wrong"), /wrong password/);
  });

  it("rejects other data, other keys and a swapped trusted comment", () => {
    const key = generateTestKey();
    const secret = decodeSecretKey(key.secret);
    const pub = decodePublicKey(key.public);
    const signature = signData(Buffer.from("a"), secret, "version:1.0.0");
    assert.throws(() => verifyData(Buffer.from("b"), signature, pub), /doesn't match the file/);
    assert.throws(() => verifyData(Buffer.from("a"), signature, decodePublicKey(TAURI_PUB)), /different key/);
    const forged = signature.replace("trusted comment: version:1.0.0", "trusted comment: version:9.0.0");
    assert.throws(() => verifyData(Buffer.from("a"), forged, pub), /trusted comment/);
    assert.throws(() => signData(Buffer.from("a"), secret, "two\nlines"), /one line/);
  });

  it("writes and reads Tauri's trusted comment", () => {
    const comment = trustedComment("dist/app.exe", "2.1.0-beta.1", new Date("2026-01-02T03:04:05Z"));
    assert.equal(comment, "timestamp:1767323045\tfile:app.exe\tversion:2.1.0-beta.1");
    assert.equal(signedVersion(comment), "2.1.0-beta.1");
    assert.equal(signedVersion("timestamp:1\tfile:x"), undefined);
  });

  it("reads every public key format", () => {
    const pub = decodePublicKey(TAURI_PUB);
    for (const format of ["minisign", "tauri", "bare"] as const) {
      assert.ok(decodePublicKey(formatPublicKey(pub, format)).key.equals(pub.key));
    }
    assert.match(unwrap(TAURI_PUB), /^untrusted comment: minisign public key: 276EFB66D359E8C4\n/);
    assert.throws(() => decodePublicKey("nonsense"), /minisign/);
  });

  it("picks libsodium's scrypt parameters", () => {
    // minisign's defaults (OPSLIMIT/MEMLIMIT_SENSITIVE) and rsign's (what tauri uses).
    assert.deepEqual(scryptParams(33554432, 1073741824), { N: 2 ** 20, r: 8, p: 1 });
    assert.deepEqual(scryptParams(1048576, 33554432), { N: 2 ** 15, r: 8, p: 1 });
    assert.deepEqual(scryptParams(16384, 1073741824), { N: 2 ** 10, r: 8, p: 1 });
  });
});

describe("version", () => {
  it("parses, formats and orders semver", () => {
    for (const v of ["2.0.0", "2.1.0-beta.2", "10.20.30-rc.1.x"]) assert.equal(format(parse(v)), v);
    assert.throws(() => parse("2.0"), /not a semver/);
    assert.throws(() => parse("v2.0.0"), /not a semver/);
    const ordered = [
      "1.0.0-alpha",
      "1.0.0-alpha.1",
      "1.0.0-beta",
      "1.0.0-beta.2",
      "1.0.0-beta.11",
      "1.0.0",
      "1.0.1",
      "1.10.0",
    ];
    for (let i = 1; i < ordered.length; i++) assert.ok(compare(ordered[i - 1], ordered[i]) < 0, ordered[i]);
    assert.equal(compare("2.0.0", "2.0.0"), 0);
  });

  it("bumps like npm's semver.inc", () => {
    const cases: [string, string, string][] = [
      ["2.0.0", "patch", "2.0.1"],
      ["2.0.0", "minor", "2.1.0"],
      ["2.3.4", "major", "3.0.0"],
      ["2.1.0-beta.2", "patch", "2.1.0"],
      ["2.1.0-beta.2", "minor", "2.1.0"],
      ["2.0.0-beta.2", "major", "2.0.0"],
      ["2.0.0", "prerelease", "2.0.1-beta.0"],
      ["2.0.1-beta.0", "prerelease", "2.0.1-beta.1"],
      ["2.0.0", "prepatch", "2.0.1-beta.0"],
      ["2.0.0", "preminor", "2.1.0-beta.0"],
      ["2.0.0", "premajor", "3.0.0-beta.0"],
    ];
    for (const [from, kind, to] of cases) assert.equal(bump(from, kind), to, `${kind} on ${from}`);
    assert.equal(bump("2.0.1-beta.3", "prerelease", "rc"), "2.0.1-rc.0");
    assert.throws(() => bump("2.0.0", "huge"), /unknown bump/);
  });

  it("strips the tag prefix", () => {
    assert.equal(stripTag(`${config.tagPrefix}1.2.3`), "1.2.3");
    assert.equal(stripTag("v1.2.3-dev.1"), "1.2.3-dev.1");
    assert.equal(stripTag("1.2.3"), "1.2.3");
  });

  it("finds the previous release", () => {
    const tags = ["1.0.0", "1.1.0-beta.0", "1.1.0-beta.1", "1.1.0", "1.2.0-beta.0", "junk"].map(
      (v) => config.tagPrefix + v,
    );
    assert.equal(previousTag(tags, "1.2.0"), `${config.tagPrefix}1.1.0`);
    assert.equal(previousTag(tags, "1.2.0-beta.1"), `${config.tagPrefix}1.2.0-beta.0`);
    assert.equal(previousTag(tags, "1.1.0"), `${config.tagPrefix}1.0.0`);
    assert.equal(previousTag(tags, "1.0.0"), undefined);
  });

  it("finds the version in [package] and [workspace.package]", () => {
    assert.equal(cargoVersion('[package]\nname = "a"\nversion = "1.2.3"\n\n[dependencies]\nx = "9"\n'), "1.2.3");
    assert.equal(
      cargoVersion('[workspace]\nmembers = []\n\n[workspace.package]\nversion = "2.0.0"\nedition = "2024"\n'),
      "2.0.0",
    );
  });

  it("moves only this repo's crates in Cargo.lock", () => {
    const lock = [
      '[[package]]\nname = "app"\nversion = "1.0.0"\ndependencies = []\n',
      '[[package]]\nname = "app-core"\nversion = "1.0.0"\n',
      '[[package]]\nname = "app-registry"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n',
      '[[package]]\nname = "other"\nversion = "1.0.0"\n',
    ].join("\n");
    const updated = updateLock(lock, "1.0.0", "1.1.0", /^app/);
    assert.match(updated, /name = "app"\nversion = "1.1.0"/);
    assert.match(updated, /name = "app-core"\nversion = "1.1.0"/);
    assert.match(updated, /name = "app-registry"\nversion = "1.0.0"/);
    assert.match(updated, /name = "other"\nversion = "1.0.0"/);
    assert.throws(() => updateLock(lock, "3.0.0", "3.1.0", /^app/), /no/);
  });

  it("rolls the changelog", () => {
    const log = "# Changelog\n\n## [Unreleased]\n\n- Fix\n\n## [1.0.0]\n\n- First\n";
    assert.equal(
      rollChangelog(log, "1.1.0"),
      "# Changelog\n\n## [Unreleased]\n\n## [1.1.0]\n\n- Fix\n\n## [1.0.0]\n\n- First\n",
    );
    assert.throws(() => rollChangelog("## [Unreleased]\n\n## [1.0.0]\n", "1.1.0"), /empty/);
    assert.throws(() => rollChangelog(log, "1.0.0"), /already has/);
  });
});

describe("latest.json", () => {
  it("lists the signed asset for every platform", () => {
    const key = generateTestKey();
    const signature = signData(Buffer.from("exe"), decodeSecretKey(key.secret), trustedComment(config.asset, "2.1.0"));
    const m = manifest("2.1.0", signature, "Notes", new Date("2026-01-02T03:04:05Z"));
    assert.equal(m.version, "2.1.0");
    assert.equal(m.pub_date, "2026-01-02T03:04:05.000Z");
    assert.deepEqual(Object.keys(m.platforms), config.platforms);
    for (const entry of Object.values(m.platforms)) {
      assert.equal(
        entry.url,
        `https://github.com/${config.repo}/releases/download/${config.tagPrefix}2.1.0/${config.asset}`,
      );
      const { trustedComment: comment } = verifyData(Buffer.from("exe"), entry.signature, decodePublicKey(key.public));
      assert.equal(signedVersion(comment), "2.1.0");
    }
    assert.throws(() => manifest("2.2.0", signature, ""), /isn't for/);
  });
});
