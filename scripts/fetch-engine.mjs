// Downloads the pinned ExifTool release(s) into engine/cache and verifies their SHA-256.
// Usage: node scripts/fetch-engine.mjs [windows|source|all]   (default: windows)
// Already-present files with the right hash are not downloaded again.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(readFileSync(join(root, "engine", "exiftool.lock.json"), "utf8"));
const cacheDir = join(root, "engine", "cache");
mkdirSync(cacheDir, { recursive: true });

const which = process.argv[2] ?? "windows";
const entries = which === "all" ? ["windows", "source"] : [which];

const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");

for (const key of entries) {
  const entry = lock[key];
  if (!entry) {
    console.error(`Unknown engine entry "${key}". Use windows, source or all.`);
    process.exit(2);
  }
  const target = join(cacheDir, entry.file);
  if (existsSync(target) && sha256(readFileSync(target)) === entry.sha256) {
    console.log(`ExifTool ${lock.version} (${key}) already present and verified.`);
    continue;
  }
  console.log(`Downloading ${entry.file} ...`);
  const res = await fetch(entry.url, { redirect: "follow", headers: { "User-Agent": "Wget/1.21" } });
  if (!res.ok) {
    console.error(`Download failed: HTTP ${res.status} for ${entry.url}`);
    process.exit(1);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  const got = sha256(buf);
  if (got !== entry.sha256) {
    console.error(`SHA-256 mismatch for ${entry.file}\n  expected ${entry.sha256}\n  got      ${got}`);
    process.exit(1);
  }
  writeFileSync(target + ".part", buf);
  renameSync(target + ".part", target);
  console.log(`Verified ${entry.file} (${(buf.length / 1048576).toFixed(1)} MB).`);
}
