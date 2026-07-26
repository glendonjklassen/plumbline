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
//
// Heavy machine-tier artifacts are marked `rnd` (TODO #28): the app boots on
// the core files (what the Android APK bundles) and fetches the rnd set in
// the background after first paint — see apps/web/src/engine/boot.ts.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const outRoot = join(repo, "apps/web/public/pack");
const STOCK = join(repo, "apps/android/app/src/main/assets/stock");

// The machine-tier artifacts deferred out of the boot path. Everything else
// under data/ matches the Android APK's bundled core set.
const RND = new Set([
  "data/morphology.jsonl",
  "data/concept-vectors.vec",
  "data/concept-vectors.vec.freq",
  "data/concept-vectors.vec.meta",
]);

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
    if (RND.has(entry.path)) entry.rnd = true;
    files.push(entry);
  }
}
// The web-stamped corpus idxcache: the PWA's FIRST boot takes the cache fast
// path instead of re-parsing ~19 MB of JSONL (8.4 s measured on a 2026
// flagship phone). Stamped mtime 0 (what the browser WASI shim reports) via
// the hydrate CLI; RAW bincode — the wire gzip below covers transport, and
// the engine reads it with zero in-wasm inflation. Marked `cache` so the
// loader fetches it only when IndexedDB doesn't already hold one.
const cacheTmp = join(tmpdir(), `plumbline-idxcache-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "-q", "-p", "plumbline-hydrate", "--", "web-cache",
   "--data", join(repo, "data/kjv.jsonl"), "--out", cacheTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const cacheRaw = readFileSync(cacheTmp);
rmSync(cacheTmp, { force: true });
const cacheGz = gzipSync(cacheRaw, { level: 9 });
writeFileSync(join(outRoot, "data", "kjv.jsonl.idxcache.gz"), cacheGz);
hash.update("data").update("kjv.jsonl.idxcache").update(cacheRaw);
files.push({ path: "data/kjv.jsonl.idxcache", bytes: cacheRaw.length, gzBytes: cacheGz.length, cache: true });

const manifest = {
  version: hash.digest("hex").slice(0, 16),
  files,
};
writeFileSync(join(outRoot, "manifest.json"), JSON.stringify(manifest, null, 2));
const mb = (n) => (n / 1048576).toFixed(1);
const total = files.reduce((s, f) => s + f.bytes, 0);
const totalGz = files.reduce((s, f) => s + f.gzBytes, 0);
console.log(`pack ${manifest.version}: ${files.length} files, ${mb(total)}MB raw -> ${mb(totalGz)}MB gzipped`);
