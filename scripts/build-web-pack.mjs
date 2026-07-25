#!/usr/bin/env node
// Assemble the web shell's data pack: gzip the read-only home files (data/ +
// bridge/) plus the bundled stock study set (the same assets/stock tree the
// Android app seeds — threads / tags / weaves) into apps/web/public/pack/
// with a manifest the loader fetches first. Stock entries are marked so the
// loader can seed them ONCE (edits/deletions stick, Android parity). The
// reader's other personal data (memory/) is never packed.
//
//   node scripts/build-web-pack.mjs
//
// The pack version is a content hash, so the service worker / Cache API can
// invalidate exactly when the data actually changes.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const outRoot = join(repo, "apps/web/public/pack");
const STOCK = join(repo, "apps/android/app/src/main/assets/stock");

// (srcDir, homeDir, filter, stock) tuples for the home shipped to the browser.
const SOURCES = [
  [join(repo, "data"), "data", (n) => !n.endsWith(".idxcache"), false],
  [join(repo, "bridge"), "bridge", () => true, false],
  [join(STOCK, "threads"), "threads", () => true, true],
  [join(STOCK, "tags"), "tags", () => true, true],
  [join(STOCK, "weaves"), "weaves", () => true, true],
];

rmSync(outRoot, { recursive: true, force: true });
const files = [];
const hash = createHash("sha256");
for (const [src, dir, keep, stock] of SOURCES) {
  if (!existsSync(src)) continue;
  const names = readdirSync(src, { withFileTypes: true })
    .filter((d) => d.isFile() && keep(d.name))
    .map((d) => d.name)
    .sort();
  for (const name of names) {
    const raw = readFileSync(join(src, name));
    const gz = gzipSync(raw, { level: 9 });
    mkdirSync(join(outRoot, dir), { recursive: true });
    writeFileSync(join(outRoot, dir, `${name}.gz`), gz);
    hash.update(dir).update(name).update(raw);
    const entry = { path: `${dir}/${name}`, bytes: raw.length, gzBytes: gz.length };
    if (stock) entry.stock = true;
    files.push(entry);
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
