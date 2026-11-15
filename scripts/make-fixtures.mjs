// Extracts sample media from ExifTool's own test images (in the pinned, hash-verified source
// package) into src-tauri/tests/fixtures/, for the end-to-end tests. Nothing binary is
// committed: run `npm run fetch-engine -- all` then `npm run fixtures`.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(readFileSync(join(root, "engine", "exiftool.lock.json"), "utf8"));
const tgz = join(root, "engine", "cache", lock.source.file);
const out = join(root, "src-tauri", "tests", "fixtures");
mkdirSync(out, { recursive: true });

const WANT = [
  "Canon.jpg",
  "GPS.jpg",
  "XMP.jpg",
  "IPTC.jpg",
  "Apple.jpg",
  "QuickTime.mov",
  "QuickTime.heic",
  "QuickTime.m4a",
  "PNG.png",
  "RIFF.webp",
  "GIF.gif",
  "DNG.dng",
  "CanonRaw.cr2",
  "PDF.pdf",
  "MP3.mp3",
];

let data;
try {
  data = readFileSync(tgz);
} catch {
  console.error(`Missing ${tgz}. Run: npm run fetch-engine -- all`);
  process.exit(1);
}
if (createHash("sha256").update(data).digest("hex") !== lock.source.sha256) {
  console.error("Source package hash mismatch. Delete engine/cache and fetch again.");
  process.exit(1);
}
const tar = gunzipSync(data);
let found = 0;
for (let off = 0; off + 512 <= tar.length; ) {
  const name = tar.toString("utf8", off, off + 100).replace(/\0.*$/s, "");
  if (!name) break;
  const prefix = tar.toString("utf8", off + 345, off + 500).replace(/\0.*$/s, "");
  const full = prefix ? `${prefix}/${name}` : name;
  const size = parseInt(tar.toString("utf8", off + 124, off + 136).replace(/\0.*$/s, "").trim() || "0", 8);
  const type = tar[off + 156];
  const base = full.split("/").pop();
  if ((type === 48 || type === 0) && full.includes("/t/images/") && WANT.includes(base)) {
    writeFileSync(join(out, base), tar.subarray(off + 512, off + 512 + size));
    found++;
  }
  off += 512 + Math.ceil(size / 512) * 512;
}
console.log(`Extracted ${found} of ${WANT.length} fixtures into ${out}`);
