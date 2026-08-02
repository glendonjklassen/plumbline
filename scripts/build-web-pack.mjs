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
// THE MANIFEST IS THE SPEC (formatVersion 2). Every entry carries:
//
//   path, bytes, gzBytes — as before
//   hash      — sha256 of the RAW bytes, 16 hex chars. Per-file, so a release
//               that changes one weave invalidates one URL instead of all 44.
//   stage     — "text" | "study" | "analysis": when the loader fetches it.
//   seedOnce  — the bundled stock study set, seeded into the reader's own files
//               once, after which their copies rule.
//   role      — "corpusCache" for the one file the fast open depends on.
//
// The loader switches on these instead of re-deriving tiers from filenames, and
// scripts/check-web-pack.mjs asserts the shape so producer and consumer cannot
// drift apart silently — the failure mode otherwise is a boot timeout with no
// diagnostic.
//
// `version` stays a rolling content hash over the whole pack: it is the single
// "which pack is this" identity that About reports and that `?v=` stamps today.
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

// ── the stage table: THE spec ────────────────────────────────────────────────
//
// Which stage a file loads in was expressed in four places that could disagree
// (a `RND` set here, three filter predicates in the web loader, and hardcoded
// filenames in the Rust). This table is now the only one; the loader switches on
// the `stage` the manifest carries, and scripts/check-web-pack.mjs asserts the
// stages partition the pack so a new file cannot be silently unreachable.
//
//   text     — needed before the reader can see a word. Stage 1.
//   study    — Strong's, the margin notes, cross-references, the overlay, the
//              bridge witnesses. Fetched right after the reader hands over.
//   analysis — the machine tier. Background, and deferred behind an explicit
//              action on phones.
//   optional — never fetched unless the reader asks for it in Settings. Today
//              that is the suggested-weave bundle (see the bottom of this file).
//
// Anything under data/ or bridge/ not named here defaults to `study`, which is
// the safe default: it loads, just not on the boot path.
const STAGE = {
  "data/kjv.jsonl.idxcache": "text",
  "data/morphology.morphb": "analysis",
  // No runtime reader anywhere in the tree: the only code that opens it is
  // witness.rs's own tests, and the fused bridge does not consume it. Staged out
  // of the boot path rather than dropped, because crates/hydrate lists it as an
  // intended R&D artifact — that disagreement is the maintainer's to settle.
  "data/text-witness.json": "analysis",
};

/** The stock study set seeds into the reader's own files once, then their copies
 *  rule. Kept in the pack forever even so: re-enabling the bundled set clears
 *  the seeded marker and the NEXT boot re-seeds from these bytes. */
const SEED_ONCE_DIRS = new Set(["threads", "tags", "weaves"]);

// The concept embedding is NOT shipped, as of 2026-07-30. `concept-vectors.vec`,
// its `.meta` / `.freq` sidecars and any packed `.vecb` sitting in data/ are all
// swept out by this prefix. The two features that read it — "verses like this"
// and the concept map — were removed the same day, so the remaining 3.08 MB of
// the analysis tier was a download nothing would ever open. The files stay in
// data/ as an offline-pipeline artifact; see BIBLIOGRAPHY.md.
const VEC_PREFIX = "concept-vectors.";
// The morphology sidecar DOES ship, and ships packed: 10.4 MB of JSONL, 31,091 serde calls
// building 355,603 entries, repeated on every launch. Packed it is both faster
// to read AND ~230 KB smaller over the wire, so there is no trade here at all.
const MORPH_TEXT = "morphology.jsonl";
const MORPH_PACKED = "morphology.morphb";
// The plain-English overlay (the AKJV delta). Core, not rnd: it is a reading
// aid over the text itself, and it is small enough to ship on the boot path
// rather than behind a download the reader has to think about.
const AKJV_TEXT = "akjv.jsonl";
const AKJV_PACKED = "akjv.akjvb";

// The raw corpus JSONL is NOT shipped. The parsed idxcache below supersedes it
// (core opens straight from the cache when no source file is present), so 2.4 MB
// gzipped was downloaded by nobody — and it was worse than dead weight: with the
// JSONL in the home, `source_stamp` succeeds, and a cache the stamp rejects makes
// the engine parse 19 MB and write a fresh 37 MB idxcache back into data/. On the
// web that is a boot that hangs for seconds and then blows the storage budget.
// The cache is the text now; the JSONL stays a data-prep input.
const KJV_TEXT = "kjv.jsonl";

// (srcDir, homeDir, filter, seedOnce) tuples for the home shipped to the browser.
const SOURCES = [
  [
    join(repo, "data"),
    "data",
    (n) =>
      !n.endsWith(".idxcache") &&
      n !== KJV_TEXT &&
      !n.startsWith(VEC_PREFIX) &&
      n !== MORPH_TEXT &&
      n !== MORPH_PACKED &&
      n !== AKJV_TEXT &&
      n !== AKJV_PACKED,
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

/** Fold a file into the rolling pack hash. Length-prefixed: plain concatenation
 *  meant ("data","x.json") and ("datax",".json") fed the same byte stream. */
function fold(dir, name, raw) {
  hash.update(`${dir.length}:${dir}${name.length}:${name}${raw.length}:`).update(raw);
}

/** The per-file content hash, over the RAW bytes — never the gzip.
 *
 *  Raw, for a decisive reason: the loader cannot know which of the two it
 *  received. Some hosts see the `.gz` extension and serve it with
 *  `Content-Encoding: gzip`, so the browser hands the app already-decompressed
 *  bytes (pack.ts sniffs the gzip magic for exactly this). A hash over the
 *  compressed form would be unverifiable on those hosts. Raw also survives a
 *  zlib version change, which would otherwise make an unchanged file look
 *  changed on a CI rebuild. */
const contentHash = (raw) => createHash("sha256").update(raw).digest("hex").slice(0, 16);

/** Emit one pack file: write the gz, fold it, and return its manifest entry. */
function emit(dir, name, raw, { stage, seedOnce = false, role } = {}) {
  const gz = gzipSync(raw, { level: 9 });
  mkdirSync(join(outRoot, dir), { recursive: true });
  writeFileSync(join(outRoot, dir, `${name}.gz`), gz);
  fold(dir, name, raw);
  const path = `${dir}/${name}`;
  const entry = {
    path,
    bytes: raw.length,
    gzBytes: gz.length,
    hash: contentHash(raw),
    stage: stage ?? STAGE[path] ?? "study",
  };
  if (seedOnce) entry.seedOnce = true;
  if (role) entry.role = role;
  files.push(entry);
  return entry;
}

for (const [src, dir, keep, seedOnce] of SOURCES) {
  if (!existsSync(src)) continue;
  const names = readdirSync(src, { withFileTypes: true })
    .filter((d) => d.isFile() && keep(d.name))
    .map((d) => d.name)
    .sort();
  for (const name of names) {
    // The stock set is tiny and needed AT OPEN so it can seed, so it rides
    // stage 1 with the text.
    const stage = seedOnce || SEED_ONCE_DIRS.has(dir) ? "text" : undefined;
    emit(dir, name, readFileSync(join(src, name)), { stage, seedOnce });
  }
}
// The web-stamped corpus idxcache: the PWA's FIRST boot takes the cache fast
// path instead of re-parsing ~19 MB of JSONL (8.4 s measured on a 2026
// flagship phone). Stamped mtime 0 (what the browser WASI shim reports) via
// the hydrate CLI; RAW bincode — the wire gzip below covers transport, and
// the engine reads it with zero in-wasm inflation. Marked `cache` so the
// loader fetches it only when IndexedDB doesn't already hold one.
//
// `--locked` on all three generated artifacts, deliberately: their bytes ARE
// bincode's encoding, so an unpinned serde/bincode bump would silently change
// the biggest file in the pack — and every reader would re-download it with no
// data having changed. The pack version is a content hash; it can only mean
// something if the content is a function of the inputs alone.
const cacheTmp = join(tmpdir(), `plumbline-idxcache-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "web-cache",
   "--data", join(repo, "data/kjv.jsonl"), "--out", cacheTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const cacheRaw = readFileSync(cacheTmp);
rmSync(cacheTmp, { force: true });
// role: the loader has to be able to FIND the corpus cache, not merely know it
// is stage-1 — it is the one file whose presence decides whether the engine
// takes the fast open or parses the JSONL that is no longer shipped.
emit("data", "kjv.jsonl.idxcache", cacheRaw, { stage: "text", role: "corpusCache" });

// ── the suggested weaves: one bundle, downloaded only if asked for ───────────
//
// These 194 files ship inside the APK and, until now, did not reach the web at
// all: the SOURCES walk above is non-recursive, so `weaves/suggested/` was
// silently skipped and the two shells disagreed about what the stock set even
// contains. They are here now, as an OPTIONAL download — the reader asks, in
// Settings — because 422 KB of machine-suggested links is not something to put
// on the boot path of a phone that may never open the weave library.
//
// ONE bundle rather than 194 pack entries, which is not merely tidier: gzipped
// individually they are 784 KB (small files compress badly and each carries its
// own dictionary), and as a single object they are 110 KB — seven times smaller,
// and one request instead of 194. The loader splits it back into
// `weaves/suggested/<name>` in the home, which is the shape the engine reads and
// the same shape Android's asset copy produces.
const suggestedDir = join(STOCK, "weaves", "suggested");
if (existsSync(suggestedDir)) {
  const bundle = {};
  for (const name of readdirSync(suggestedDir).filter((n) => n.endsWith(".json")).sort()) {
    // Stored as TEXT, not re-serialized JSON: these are the maintainer's files,
    // and a round-trip through JSON.parse would quietly restyle every one of
    // them — which shows up as a whole-bundle hash change on a release that
    // touched nothing.
    bundle[name] = readFileSync(join(suggestedDir, name), "utf8");
  }
  emit("weaves", "suggested.bundle.json", Buffer.from(JSON.stringify(bundle)), {
    stage: "optional",
    role: "suggestedWeaves",
  });
}

const morphTmp = join(tmpdir(), `plumbline-morphb-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "morphb",
   "--from", join(repo, "data", MORPH_TEXT), "--out", morphTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const morphRaw = readFileSync(morphTmp);
rmSync(morphTmp, { force: true });
emit("data", MORPH_PACKED, morphRaw);

const akjvTmp = join(tmpdir(), `plumbline-akjvb-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "akjvb",
   "--from", join(repo, "data", AKJV_TEXT), "--out", akjvTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const akjvRaw = readFileSync(akjvTmp);
rmSync(akjvTmp, { force: true });
emit("data", AKJV_PACKED, akjvRaw);

// Sorted so the manifest's own bytes do not depend on emission order, and so a
// human diff between two packs is readable.
files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
const manifest = {
  // Bumped on any NON-additive change to the entry shape, so a loader that does
  // not understand a pack fails loudly instead of quietly mis-tiering files.
  formatVersion: 2,
  version: hash.digest("hex").slice(0, 16),
  files,
};
writeFileSync(join(outRoot, "manifest.json"), JSON.stringify(manifest, null, 2));
const mb = (n) => (n / 1048576).toFixed(1);
const total = files.reduce((s, f) => s + f.bytes, 0);
const totalGz = files.reduce((s, f) => s + f.gzBytes, 0);
const byStage = (st) => files.filter((f) => f.stage === st);
const stageLine = ["text", "study", "analysis", "optional"]
  .map((st) => `${st} ${byStage(st).length}/${mb(byStage(st).reduce((s, f) => s + f.gzBytes, 0))}MB`)
  .join(", ");
console.log(`pack ${manifest.version}: ${files.length} files, ${mb(total)}MB raw -> ${mb(totalGz)}MB gzipped`);
console.log(`  stages: ${stageLine}`);
