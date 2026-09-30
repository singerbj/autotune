// Checks that a code-signed exe is exactly the exe CI built, plus an Authenticode signature.
// The release workflow's codesign job runs it on the file the signer returns, before the
// minisign step, so the updater's signature never covers code that wasn't built here.
//
//   node scripts/release/check-signed-exe.ts <unsigned.exe> <signed.exe>
//
// Signing may only change the PE checksum and the certificate table entry, pad the file to
// 8 bytes, and append the certificate table. Every other byte must match. It doesn't check the
// signature itself: Get-AuthenticodeSignature does that on Windows.
import { readFileSync } from "node:fs";

type Pe = { checksum: number; certEntry: number; certAddr: number; certSize: number };

function fail(message: string): never {
  console.error(`check-signed-exe: ${message}`);
  process.exit(1);
}

function parsePe(file: Buffer, name: string): Pe {
  if (file.length < 0x40 || file.toString("latin1", 0, 2) !== "MZ") fail(`${name} is not an exe`);
  const pe = file.readUInt32LE(0x3c);
  if (pe + 24 + 2 > file.length || file.toString("latin1", pe, pe + 4) !== "PE\0\0") {
    fail(`${name} has no PE header`);
  }
  const optional = pe + 24;
  const magic = file.readUInt16LE(optional);
  // Data directories start after the fixed part of the optional header (PE32+ or PE32).
  const dataDirs = optional + (magic === 0x20b ? 112 : magic === 0x10b ? 96 : fail(`${name}: unknown PE magic`));
  const certEntry = dataDirs + 4 * 8; // IMAGE_DIRECTORY_ENTRY_SECURITY
  if (certEntry + 8 > file.length) fail(`${name} has a truncated PE header`);
  return {
    checksum: optional + 64,
    certEntry,
    certAddr: file.readUInt32LE(certEntry),
    certSize: file.readUInt32LE(certEntry + 4),
  };
}

const [unsignedPath, signedPath] = process.argv.slice(2);
if (!unsignedPath || !signedPath) fail("usage: node scripts/release/check-signed-exe.ts <unsigned.exe> <signed.exe>");

const unsigned = readFileSync(unsignedPath);
const signed = readFileSync(signedPath);
const u = parsePe(unsigned, unsignedPath);
const s = parsePe(signed, signedPath);

if (u.certAddr !== 0 || u.certSize !== 0) fail(`${unsignedPath} is already signed`);
if (s.checksum !== u.checksum || s.certEntry !== u.certEntry) fail("the PE headers moved");
if (s.certSize === 0) fail(`${signedPath} is not signed`);

// The certificate table must start right after the original bytes (8-byte aligned, zero padded)
// and run to the end of the file.
const padding = s.certAddr - unsigned.length;
if (padding < 0 || padding >= 8 || s.certAddr % 8 !== 0)
  fail("the certificate table is not appended to the original file");
if (s.certAddr + s.certSize !== signed.length) fail("there is data after the certificate table");
if (signed.subarray(unsigned.length, s.certAddr).some((b) => b !== 0))
  fail("the padding before the certificate table is not zero");

// Every original byte except the checksum and the certificate table entry is unchanged.
const same = (from: number, to: number) => unsigned.subarray(from, to).equals(signed.subarray(from, to));
if (!same(0, u.checksum) || !same(u.checksum + 4, u.certEntry) || !same(u.certEntry + 8, unsigned.length)) {
  fail(`${signedPath} is not ${unsignedPath} plus a signature`);
}

console.log(`${signedPath} is ${unsignedPath} plus a ${s.certSize}-byte signature`);
