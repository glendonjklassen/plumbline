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
  "data/morphology.morphb",
  "data/concept-vectors.vecb",
  "data/concept-vectors.vec.freq",
  "data/concept-vectors.vec.meta",
]);

// The concept vectors ship PACKED (`.vecb`, built below), never as the 6.4 MB
// text: the browser cannot keep a parsed embedding between launches, so the text
// cost 742,600 atof calls on every single start. Both names are swept out here —
// the text because the packed form supersedes it, and any `.vecb` sitting in
// data/ because it is added explicitly below with its `rnd` flag (swept in, it
// would be treated as a core file and fetched on the boot path).
const VEC_TEXT = "concept-vectors.vec";
const VEC_PACKED = "concept-vectors.vecb";
// Same story for the morphology sidecar: 10.4 MB of JSONL, 31,091 serde calls
// building 355,603 entries, repeated on every launch. Packed it is both faster
// to read AND ~230 KB smaller over the wire, so there is no trade here at all.
const MORPH_TEXT = "morphology.jsonl";
const MORPH_PACKED = "morphology.morphb";

// (srcDir, homeDir, filter, stock) tuples for the home shipped to the browser.
const SOURCES = [
  [
    join(repo, "data"),
    "data",
    (n) =>
      !n.endsWith(".idxcache") &&
      n !== VEC_TEXT &&
      n !== VEC_PACKED &&
      n !== MORPH_TEXT &&
      n !== MORPH_PACKED,
    false,
  ],
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

// The concept vectors as packed f32 (`.vecb`) instead of word2vec text. The
// engine reads the rows with a copy rather than 742,600 atof calls — measured
// 22.15ms -> 7.08ms native, and it is the parse a phone repeats on EVERY launch
// because the parsed embedding lives in wasm memory and cannot outlive the tab
// (feedback 2026-07-27). Costs ~383 KB more over the wire than the text, which
// gzips better; that is paid once and cached, the parse was paid every time.
const vecbTmp = join(tmpdir(), `plumbline-vecb-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "-q", "-p", "plumbline-hydrate", "--", "vecb",
   "--from", join(repo, "data", VEC_TEXT), "--out", vecbTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const vecbRaw = readFileSync(vecbTmp);
rmSync(vecbTmp, { force: true });
const vecbGz = gzipSync(vecbRaw, { level: 9 });
writeFileSync(join(outRoot, "data", `${VEC_PACKED}.gz`), vecbGz);
hash.update("data").update(VEC_PACKED).update(vecbRaw);
files.push({
  path: `data/${VEC_PACKED}`,
  bytes: vecbRaw.length,
  gzBytes: vecbGz.length,
  rnd: true,
});

const morphTmp = join(tmpdir(), `plumbline-morphb-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "-q", "-p", "plumbline-hydrate", "--", "morphb",
   "--from", join(repo, "data", MORPH_TEXT), "--out", morphTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const morphRaw = readFileSync(morphTmp);
rmSync(morphTmp, { force: true });
const morphGz = gzipSync(morphRaw, { level: 9 });
writeFileSync(join(outRoot, "data", `${MORPH_PACKED}.gz`), morphGz);
hash.update("data").update(MORPH_PACKED).update(morphRaw);
files.push({
  path: `data/${MORPH_PACKED}`,
  bytes: morphRaw.length,
  gzBytes: morphGz.length,
  rnd: true,
});

const manifest = {
  version: hash.digest("hex").slice(0, 16),
  files,
};
writeFileSync(join(outRoot, "manifest.json"), JSON.stringify(manifest, null, 2));
const mb = (n) => (n / 1048576).toFixed(1);
const total = files.reduce((s, f) => s + f.bytes, 0);
const totalGz = files.reduce((s, f) => s + f.gzBytes, 0);
console.log(`pack ${manifest.version}: ${files.length} files, ${mb(total)}MB raw -> ${mb(totalGz)}MB gzipped`);
