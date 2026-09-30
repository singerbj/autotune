// Minisign signatures for the in-app updater, with Node's built-in crypto only.
//
//   node scripts/release/minisign.ts sign <file> --version <version>
//       signs with $UPDATE_SIGNING_KEY (password: $UPDATE_SIGNING_KEY_PASSWORD, if any), writes
//       <file>.minisig, and checks it against $UPDATE_PUBKEY when that is set
//   node scripts/release/minisign.ts verify <file> [--version <version>]
//       checks <file>.minisig against $UPDATE_PUBKEY (and the version it was signed for, if given)
//   node scripts/release/minisign.ts pubkey [--format minisign|tauri|bare]
//       prints $UPDATE_PUBKEY in the given format (default tauri: what the apps bake in)
//
// Keys and signatures may be minisign text (`minisign -G`), the base64 of that text (what
// `tauri signer generate` writes), or a bare `RW…` public key line. Encrypted keys (scrypt)
// and unencrypted ones (`minisign -W`) both work. Signatures are prehashed (BLAKE2b-512, the
// `ED` algorithm), the same as `minisign -S` and `tauri signer sign`.
//
// The trusted comment is signed too. Releases use Tauri's format,
// `timestamp:<unix>\tfile:<name>\tversion:<version>` (what `tauri signer sign` writes when it
// bundles), and every app checks the version in it against the one it was offered, so a signed
// older build can't be passed off as a newer one.
import { createPrivateKey, createPublicKey, createHash, randomBytes, scryptSync, sign, verify } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";

export type PublicKey = { keyId: Buffer; key: Buffer };
export type SecretKey = { keyId: Buffer; seed: Buffer; publicKey: Buffer };
export type Signature = { trustedComment: string };

const ED25519_PKCS8 = Buffer.from("302e020100300506032b657004220420", "hex");
const ED25519_SPKI = Buffer.from("302a300506032b6570032100", "hex");

class MinisignError extends Error {}

function fail(message: string): never {
  throw new MinisignError(message);
}

/** Minisign text of `value`: itself if it already is, else its base64 decoding. */
export function unwrap(value: string): string {
  const text = value.trim();
  if (text.includes("untrusted comment:") || /^RW[A-Za-z0-9+/=]+$/.test(text)) return text;
  const decoded = Buffer.from(text, "base64").toString("utf8").trim();
  if (!decoded.includes("untrusted comment:")) fail("not minisign text, or the base64 of it");
  return decoded;
}

/** The base64 line of minisign text (the line after the untrusted comment). */
function dataLine(text: string, what: string): Buffer {
  const lines = text.split(/\r?\n/).map((l) => l.trim());
  const line = lines[0].startsWith("untrusted comment:") ? lines[1] : lines[0];
  if (!line || !/^[A-Za-z0-9+/]+={0,2}$/.test(line)) fail(`bad ${what}: no base64 key line`);
  return Buffer.from(line, "base64");
}

export function decodePublicKey(value: string): PublicKey {
  const bytes = dataLine(unwrap(value), "public key");
  if (bytes.length !== 42 || bytes.toString("latin1", 0, 2) !== "Ed")
    fail("bad public key: not an Ed25519 minisign key");
  return { keyId: bytes.subarray(2, 10), key: bytes.subarray(10) };
}

/** libsodium's crypto_pwhash_scryptsalsa208sha256 parameters for `opslimit` and `memlimit`. */
export function scryptParams(opslimit: number, memlimit: number): { N: number; r: number; p: number } {
  const ops = Math.max(opslimit, 32768);
  const r = 8;
  const log2 = (maxN: number) => {
    let n = 1;
    while (n < 63 && 2 ** n <= maxN / 2) n++;
    return n;
  };
  if (ops < memlimit / 32) {
    return { N: 2 ** log2(ops / (r * 4)), r, p: 1 };
  }
  const nLog2 = log2(memlimit / (r * 128));
  const maxrp = Math.min(Math.floor(ops / 4 / 2 ** nLog2), 0x3fffffff);
  return { N: 2 ** nLog2, r, p: Math.max(1, Math.floor(maxrp / r)) };
}

function publicKeyOf(seed: Buffer): Buffer {
  const key = createPrivateKey({ key: Buffer.concat([ED25519_PKCS8, seed]), format: "der", type: "pkcs8" });
  return createPublicKey(key).export({ format: "der", type: "spki" }).subarray(-32);
}

export function decodeSecretKey(value: string, password = ""): SecretKey {
  const bytes = dataLine(unwrap(value), "secret key");
  if (bytes.length !== 158 || bytes.toString("latin1", 0, 2) !== "Ed")
    fail("bad secret key: not an Ed25519 minisign key");
  const kdf = bytes.toString("latin1", 2, 4);
  const salt = bytes.subarray(6, 38);
  const opslimit = Number(bytes.readBigUInt64LE(38));
  const memlimit = Number(bytes.readBigUInt64LE(46));
  let keynum = Buffer.from(bytes.subarray(54));
  if (kdf === "Sc") {
    const { N, r, p } = scryptParams(opslimit, memlimit);
    const stream = scryptSync(password, salt, keynum.length, { N, r, p, maxmem: 128 * N * r * p + 64 * 1024 * 1024 });
    keynum = Buffer.from(keynum.map((b, i) => b ^ stream[i]));
  } else if (kdf !== "\0\0") {
    fail(`bad secret key: unknown key derivation "${kdf}"`);
  }
  const seed = keynum.subarray(8, 40);
  const publicKey = keynum.subarray(40, 72);
  if (!publicKeyOf(seed).equals(publicKey)) {
    fail(kdf === "Sc" ? "can't decrypt the secret key: wrong password?" : "bad secret key: damaged");
  }
  return { keyId: keynum.subarray(0, 8), seed, publicKey };
}

/** Tauri's trusted comment for `file` signed as `version`. */
export function trustedComment(file: string, version: string, date = new Date()): string {
  return `timestamp:${Math.floor(date.getTime() / 1000)}\tfile:${basename(file)}\tversion:${version}`;
}

/** The `version:` field of a trusted comment in Tauri's format. */
export function signedVersion(trustedComment: string): string | undefined {
  return trustedComment
    .split("\t")
    .find((field) => field.startsWith("version:"))
    ?.slice("version:".length);
}

/** Minisign signature text for `data`. */
export function signData(data: Buffer, key: SecretKey, trustedComment: string): string {
  if (/[\r\n]/.test(trustedComment)) fail("the trusted comment must be one line");
  const privateKey = createPrivateKey({ key: Buffer.concat([ED25519_PKCS8, key.seed]), format: "der", type: "pkcs8" });
  const hash = createHash("blake2b512").update(data).digest();
  const signature = sign(null, hash, privateKey);
  const global = sign(null, Buffer.concat([signature, Buffer.from(trustedComment, "utf8")]), privateKey);
  return [
    "untrusted comment: signature from minisign secret key",
    Buffer.concat([Buffer.from("ED", "latin1"), key.keyId, signature]).toString("base64"),
    `trusted comment: ${trustedComment}`,
    global.toString("base64"),
    "",
  ].join("\n");
}

/** Checks `signatureText` (minisign text or its base64) over `data`; returns the trusted comment. */
export function verifyData(data: Buffer, signatureText: string, key: PublicKey): Signature {
  const text = unwrap(signatureText);
  const lines = text.split(/\r?\n/).map((l) => l.trimEnd());
  if (lines.length < 4 || !lines[0].startsWith("untrusted comment:") || !lines[2].startsWith("trusted comment: ")) {
    fail("bad signature: not a minisign signature");
  }
  const bytes = Buffer.from(lines[1], "base64");
  const algorithm = bytes.toString("latin1", 0, 2);
  if (bytes.length !== 74 || (algorithm !== "ED" && algorithm !== "Ed"))
    fail("bad signature: not an Ed25519 signature");
  if (!bytes.subarray(2, 10).equals(key.keyId)) fail("the signature was made with a different key");
  const publicKey = createPublicKey({ key: Buffer.concat([ED25519_SPKI, key.key]), format: "der", type: "spki" });
  const signature = bytes.subarray(10);
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, message, publicKey, signature)) fail("the signature doesn't match the file");
  const trustedComment = lines[2].slice("trusted comment: ".length);
  const global = Buffer.from(lines[3], "base64");
  if (!verify(null, Buffer.concat([signature, Buffer.from(trustedComment, "utf8")]), publicKey, global)) {
    fail("the trusted comment's signature doesn't match");
  }
  return { trustedComment };
}

/** Formats a public key: `minisign` (the .pub file), `tauri` (base64 of that) or `bare` (`RW…`). */
export function formatPublicKey(key: PublicKey, format: "minisign" | "tauri" | "bare"): string {
  const bare = Buffer.concat([Buffer.from("Ed", "latin1"), key.keyId, key.key]).toString("base64");
  const id = Buffer.from(key.keyId).reverse().toString("hex").toUpperCase();
  const text = `untrusted comment: minisign public key: ${id}\n${bare}\n`;
  return format === "bare" ? bare : format === "minisign" ? text : Buffer.from(text).toString("base64");
}

/** A new key pair (unencrypted secret key), for tests. */
export function generateTestKey(): { secret: string; public: string } {
  const seed = randomBytes(32);
  const publicKey = publicKeyOf(seed);
  const keyId = randomBytes(8);
  // The checksum isn't checked here (the public key is), so it's left zero.
  const keynum = Buffer.concat([keyId, seed, publicKey, Buffer.alloc(32)]);
  const header = Buffer.concat([Buffer.from("Ed\0\0B2", "latin1"), Buffer.alloc(32), Buffer.alloc(16)]);
  const secret = `untrusted comment: minisign secret key\n${Buffer.concat([header, keynum]).toString("base64")}\n`;
  return { secret, public: formatPublicKey({ keyId, key: publicKey }, "minisign") };
}

function env(name: string): string | undefined {
  const value = process.env[name]?.trim();
  return value ? value : undefined;
}

function main(args: string[]): void {
  const [command, ...rest] = args;
  const option = (name: string) => {
    const i = rest.indexOf(name);
    if (i < 0) return undefined;
    const value = rest[i + 1] ?? fail(`${name} needs a value`);
    rest.splice(i, 2);
    return value;
  };
  const version = option("--version");
  const format = option("--format") ?? "tauri";
  const file = rest[0];
  const pubkey = env("UPDATE_PUBKEY");
  switch (command) {
    case "sign": {
      if (!file || !version) fail("usage: minisign.ts sign <file> --version <version>");
      const comment = trustedComment(file, version);
      const key = decodeSecretKey(
        env("UPDATE_SIGNING_KEY") ?? fail("UPDATE_SIGNING_KEY is not set"),
        process.env.UPDATE_SIGNING_KEY_PASSWORD ?? "",
      );
      const data = readFileSync(file);
      const signature = signData(data, key, comment);
      if (pubkey) {
        const expected = decodePublicKey(pubkey);
        if (!expected.key.equals(key.publicKey)) fail("UPDATE_SIGNING_KEY doesn't belong to UPDATE_PUBKEY");
        verifyData(data, signature, expected);
      }
      writeFileSync(`${file}.minisig`, signature);
      console.log(`signed ${file} (${JSON.stringify(comment)})${pubkey ? ", checked against UPDATE_PUBKEY" : ""}`);
      return;
    }
    case "verify": {
      if (!file) fail("usage: minisign.ts verify <file> [--version <version>]");
      const key = decodePublicKey(pubkey ?? fail("UPDATE_PUBKEY is not set"));
      const comment = verifyData(readFileSync(file), readFileSync(`${file}.minisig`, "utf8"), key).trustedComment;
      const signed = signedVersion(comment);
      if (version !== undefined && signed !== version)
        fail(`the signature is for version ${signed ?? "(none)"}, not ${version}`);
      console.log(`${file}: good signature (${JSON.stringify(comment)})`);
      return;
    }
    case "pubkey": {
      if (format !== "minisign" && format !== "tauri" && format !== "bare") fail(`unknown format ${format}`);
      process.stdout.write(formatPublicKey(decodePublicKey(pubkey ?? fail("UPDATE_PUBKEY is not set")), format));
      return;
    }
    default:
      fail("usage: minisign.ts sign|verify|pubkey …");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`minisign: ${e instanceof MinisignError ? e.message : e}`);
    process.exit(1);
  }
}
