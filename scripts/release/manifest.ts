// Writes the release's `latest.json`: the updater manifest in Tauri's format, which every app's
// updater reads.
//
//   node scripts/release/manifest.ts <version> <signature file> [notes file] > latest.json
//
// {"version", "notes", "pub_date", "platforms": {"windows-x86_64": {"url", "signature"}}}, where
// `signature` is the base64 of the minisign signature (Tauri's encoding) and `url` the release
// asset's download link.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { config } from "./config.ts";
import { signedVersion, unwrap } from "./minisign.ts";
import { parse, stripTag } from "./version.ts";

export type Manifest = {
  version: string;
  notes: string;
  pub_date: string;
  platforms: Record<string, { url: string; signature: string }>;
};

export function downloadUrl(version: string): string {
  return `https://github.com/${config.repo}/releases/download/${config.tagPrefix}${version}/${config.asset}`;
}

export function manifest(version: string, signature: string, notes: string, date = new Date()): Manifest {
  parse(version);
  const text = unwrap(signature);
  const comment = text
    .split(/\r?\n/)
    .find((l) => l.startsWith("trusted comment: "))
    ?.slice("trusted comment: ".length);
  if (!comment || signedVersion(comment) !== version) throw new Error(`the signature isn't for version ${version}`);
  const entry = { url: downloadUrl(version), signature: Buffer.from(text.trim() + "\n").toString("base64") };
  return {
    version,
    notes,
    pub_date: date.toISOString(),
    platforms: Object.fromEntries(config.platforms.map((p) => [p, entry])),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, signatureFile, notesFile] = process.argv.slice(2);
  if (!version || !signatureFile) {
    console.error("usage: manifest.ts <version> <signature file> [notes file]");
    process.exit(2);
  }
  try {
    const notes = notesFile ? readFileSync(notesFile, "utf8").trim() : "";
    const result = manifest(stripTag(version), readFileSync(signatureFile, "utf8"), notes);
    console.log(JSON.stringify(result, null, 2));
  } catch (e) {
    console.error(`manifest: ${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}
