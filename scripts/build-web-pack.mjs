#!/usr/bin/env node
// Assemble the web shell's data pack: gzip the read-only home files (data/ +
// bridge/) into apps/web/public/pack/ with a manifest the loader fetches
// first. Personal study data (tags/, threads/, memory/) is deliberately NOT
// packed — web visitors author their own into browser storage.
//
//   node scripts/build-web-pack.mjs
//
// The pack version is a content hash, so the service worker / Cache API can
// invalidate exactly when the data actually changes.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const outRoot = join(repo, "apps/web/public/pack");

// (dir, filter) pairs that make up the read-only home shipped to the browser.
const SOURCES = [
  ["data", (n) => !n.endsWith(".idxcache")], // cache is rebuilt + persisted client-side
  ["bridge", () => true],
];

rmSync(outRoot, { recursive: true, force: true });
const files = [];
const hash = createHash("sha256");
for (const [dir, keep] of SOURCES) {
  for (const name of readdirSync(join(repo, dir)).filter(keep).sort()) {
    const raw = readFileSync(join(repo, dir, name));
    const gz = gzipSync(raw, { level: 9 });
    mkdirSync(join(outRoot, dir), { recursive: true });
    writeFileSync(join(outRoot, dir, `${name}.gz`), gz);
    hash.update(dir).update(name).update(raw);
    files.push({ path: `${dir}/${name}`, bytes: raw.length, gzBytes: gz.length });
  }
}
const manifest = {
  version: hash.digest("hex").slice(0, 16),
  files,
};
writeFileSync(join(outRoot, "manifest.json"), JSON.stringify(manifest, null, 2));
const mb = (n) => (n / 1048576).toFixed(1);
const total = files.reduce((s, f) => s + f.bytes, 0);
const totalGz = files.reduce((s, f) => s + f.gzBytes, 0);
console.log(`pack ${manifest.version}: ${files.length} files, ${mb(total)}MB raw -> ${mb(totalGz)}MB gzipped`);
