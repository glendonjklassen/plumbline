#!/usr/bin/env node
// Assemble the web shell's data pack: gzip the read-only home files (data/ +
// bridge/) plus the bundled stock study set (stock/ — threads / tags / weaves)
// into apps/web/public/pack/ with a manifest the loader fetches first. Stock
// entries are marked so the loader seeds them once (edits/deletions stick). The
// reader's other personal data (memory/) is never packed.
//
//   node scripts/build-web-pack.mjs
//
// The manifest is the load spec (formatVersion 2). Every entry carries:
//
//   path, bytes, gzBytes
//   hash      — sha256 of the RAW bytes, 16 hex chars. Per-file, so a release
//               that changes one weave invalidates one URL instead of all 44.
//   stage     — "text" | "study" | "analysis" | "corpus" | "optional": when the
//               loader fetches it.
//   seedOnce  — stock study set, seeded into the reader's own files once, after
//               which their copies rule.
//   role      — "corpusCache" for the one file the fast open depends on.
//
// The loader switches on these instead of re-deriving tiers from filenames;
// scripts/check-web-pack.mjs asserts the shape so producer and consumer cannot
// drift silently (the failure mode otherwise is a boot timeout with no
// diagnostic). `version` is a rolling content hash over the whole pack.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const outRoot = join(repo, "apps/web/public/pack");
const STOCK = join(repo, "stock");

// ── the stage table ──────────────────────────────────────────────────────────
//
//   text     — needed before the reader can see a word. Stage 1.
//   study    — Strong's, the margin notes, cross-references, the overlay, the
//              bridge witnesses. Fetched right after the reader hands over.
//   analysis — the machine tier. Background, and deferred behind an explicit
//              action on phones.
//   corpus   — every language's Bible except the one stage 1 opens. Background
//              download on every device, after there is text on screen. Not
//              `text` (a reader opens one Bible; 35 MB before first paint is
//              nonsense), not `study` (`fetchStage2Pack` inflates everything it
//              selects into the home — three corpora is ~91 MB), not `optional`
//              (nobody should have to ask for their own Bible).
//   optional — never fetched unless the reader asks: the suggested-weave bundle
//              and the machine-translated dictionaries.
//
// Anything under data/ or bridge/ not named here defaults to `study`: it loads,
// just not on the boot path.
const STAGE = {
  "data/kjv.jsonl.idxcache": "text",
  // The first-run path depends on it (36 KB): the new-believer welcome starts
  // the bundled booklet as it hands over, and `devotional_start` refuses an id
  // the catalogue does not carry — on `study` that write would race its
  // download. Never evicted either (the `bridge/*` rule in home.ts): the engine
  // parses it lazily on an arbitrary later tap.
  "data/devotional.json": "text",
  "data/morphology.morphb": "analysis",
  // No runtime reader in the tree — only witness.rs's own tests open it. Staged
  // off the boot path rather than dropped, because crates/hydrate lists it as an
  // intended R&D artifact.
  "data/text-witness.json": "analysis",
};

/** The stock study set seeds into the reader's own files once, then their copies
 *  rule. Kept in the pack even so: re-enabling the bundled set clears the seeded
 *  marker and the next boot re-seeds from these bytes. */
const SEED_ONCE_DIRS = new Set(["threads", "tags", "weaves"]);

// The concept embedding is not shipped — nothing reads it since "verses like
// this" and the concept map were removed. This prefix sweeps out
// `concept-vectors.vec`, its `.meta` / `.freq` sidecars and any packed `.vecb`
// in data/; the files stay as an offline-pipeline artifact (BIBLIOGRAPHY.md).
const VEC_PREFIX = "concept-vectors.";
// The morphology sidecar ships packed: 10.4 MB of JSONL, 355,603 entries parsed
// on every launch. Packed it is both faster to read and ~230 KB smaller over
// the wire.
const MORPH_TEXT = "morphology.jsonl";
const MORPH_PACKED = "morphology.morphb";
// The plain-English overlay (the AKJV delta) — a reading aid over the text
// itself, small enough to ship on the boot path.
const AKJV_TEXT = "akjv.jsonl";
const AKJV_PACKED = "akjv.akjvb";

// The raw corpus JSONL is NOT shipped: the idxcache below supersedes it (core
// opens straight from the cache when no source file is present), and shipping it
// is worse than dead weight — with the JSONL in the home `source_stamp`
// succeeds, so a cache the stamp rejects makes the engine parse 19 MB and write
// a fresh 37 MB idxcache back into data/: a boot that hangs for seconds and then
// blows the storage budget.
const KJV_TEXT = "kjv.jsonl";

// ── the language registry ────────────────────────────────────────────────────
//
// Asked for, not duplicated: which corpus belongs to which language, what its
// cache is called and which dictionary goes with it are facts the Rust core
// already holds (`crates/core/src/i18n.rs`), and `plumbline-hydrate languages`
// prints them.
const REGISTRY = JSON.parse(
  execFileSync("cargo", ["run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "languages"], {
    cwd: repo,
    encoding: "utf8",
  }),
).languages;
const BASE_LANG = REGISTRY.find((l) => l.code === "en");
/** The languages whose text is an optional download: everyone but English. */
const EXTRA_LANGS = REGISTRY.filter((l) => l.code !== BASE_LANG.code && l.corpus);
/** Every per-language file the generic `data/` walk must NOT pick up: a corpus
 *  JSONL is superseded by its idxcache (see above) and another language's
 *  dictionary is an optional download, so both would otherwise be fetched by
 *  every English reader. */
const LANG_FILES = new Set(EXTRA_LANGS.flatMap((l) => [l.corpus, l.lexicon].filter(Boolean)));

// (srcDir, homeDir, filter, seedOnce) tuples for the home shipped to the browser.
/** The only file types `data/` may publish — an allowlist, because a denylist
 *  ships whatever nobody thought to name. `data/` is also where the maintainer's
 *  own working files land (a source .docx was once gzipped into the pack);
 *  .gitignore stops those reaching git, not this walk, which reads the disk. */
const DATA_EXTS = new Set(["json", "jsonl", "tsv", "morphb", "akjvb"]);

const SOURCES = [
  [
    join(repo, "data"),
    "data",
    (n) =>
      DATA_EXTS.has(n.split(".").pop()) &&
      !n.endsWith(".idxcache") &&
      n !== KJV_TEXT &&
      !LANG_FILES.has(n) && // every other language's text + dictionary: emitted with their roles below
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

/** The per-file content hash, over the RAW bytes — never the gzip: some hosts
 *  serve a `.gz` with `Content-Encoding: gzip`, so the loader cannot know which
 *  form it received (pack.ts sniffs the gzip magic) and a hash over the
 *  compressed form would be unverifiable there. Raw also survives a zlib version
 *  change, which would make an unchanged file look changed. */
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
    // The stock set is tiny and must be present at open so it can seed, so it
    // rides stage 1 with the text.
    const stage = seedOnce || SEED_ONCE_DIRS.has(dir) ? "text" : undefined;
    emit(dir, name, readFileSync(join(src, name)), { stage, seedOnce });
  }
}
// The web-stamped corpus idxcache: the PWA's first boot takes the cache fast
// path instead of re-parsing ~19 MB of JSONL (8.4 s on a 2026 flagship phone).
// Stamped mtime 0 (what the browser WASI shim reports) via the hydrate CLI;
// RAW bincode — the wire gzip below covers transport, and the engine reads it
// with zero in-wasm inflation.
//
// `--locked` on all three generated artifacts: their bytes ARE bincode's
// encoding, so an unpinned serde/bincode bump would change the biggest file in
// the pack and every reader would re-download it with no data having changed.
// The pack version is a content hash, which can only mean something if the
// content is a function of the inputs alone.
const cacheTmp = join(tmpdir(), `plumbline-idxcache-${process.pid}`);
execFileSync(
  "cargo",
  ["run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "web-cache",
   "--data", join(repo, "data/kjv.jsonl"), "--out", cacheTmp],
  { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
);
const cacheRaw = readFileSync(cacheTmp);
rmSync(cacheTmp, { force: true });
// role, not a filename match: the loader has to be able to FIND the corpus
// cache — it is the one file whose presence decides whether the engine takes the
// fast open or parses the JSONL that is no longer shipped.
emit("data", "kjv.jsonl.idxcache", cacheRaw, { stage: "text", role: BASE_LANG.corpusRole });

// ── every other language's text: the same cache, for the other Bibles ────────
//
// Bundled, not optional: a phone set to Arabic must not open in Arabic and be
// shown the English KJV because the reader's own Bible is a download gated
// behind a Settings screen. Their own stage rather than `text`, though, because
// nothing here is on the path to first paint — three corpora cost 9 MB on the
// wire, once, in the background, after the reader is already reading.
//
// Only ONE is ever inflated into the home (`isOtherCorpus` in the loader skips
// the rest), so this costs bytes and disk but no memory and no boot time.
//
// Each takes its own role per language, never `corpusCache`: the stage-1 boot
// must keep taking the English one, and a second file claiming that role would
// make which text opens depend on manifest order.
//
// The dictionaries stay `optional`: machine-translated, and unlike a Bible
// nothing is broken without one (`strongs_for` serves the English definitions).
// Excluded from the generic data/ walk above so no English reader fetches one;
// conditional on existing, because a corpus is built by its own data-prep
// pipeline and a checkout that has not run it is not broken.
for (const lang of EXTRA_LANGS) {
  if (lang.lexicon) {
    const lexSrc = join(repo, "data", lang.lexicon);
    if (existsSync(lexSrc)) {
      emit("data", lang.lexicon, readFileSync(lexSrc), { stage: "optional", role: lang.lexiconRole });
    }
  }
  const src = join(repo, "data", lang.corpus);
  if (!existsSync(src)) continue;
  const tmp = join(tmpdir(), `plumbline-idxcache-${lang.code}-${process.pid}`);
  execFileSync(
    "cargo",
    ["run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "web-cache",
     "--data", src, "--out", tmp],
    { cwd: repo, stdio: ["ignore", "inherit", "inherit"] },
  );
  const raw = readFileSync(tmp);
  rmSync(tmp, { force: true });
  emit("data", lang.corpusCache, raw, { stage: "corpus", role: lang.corpusRole });
}

// ── the suggested weaves: one bundle, downloaded only if asked for ───────────
//
// Optional (the reader asks, in Settings): 422 KB of machine-suggested links
// does not belong on the boot path of a phone that may never open the weave
// library. The SOURCES walk above is non-recursive, so `weaves/suggested/` is
// not picked up there.
//
// One bundle rather than 194 entries: gzipped individually they are 784 KB
// (small files compress badly, each carrying its own dictionary) and as a single
// object 110 KB — one request instead of 194. The loader splits it back into
// `weaves/suggested/<name>` in the home, which is the shape the engine reads.
const suggestedDir = join(STOCK, "weaves", "suggested");
if (existsSync(suggestedDir)) {
  const bundle = {};
  for (const name of readdirSync(suggestedDir).filter((n) => n.endsWith(".json")).sort()) {
    // Stored as TEXT, not re-serialized JSON: a round-trip through JSON.parse
    // would restyle every file, which shows up as a whole-bundle hash change on
    // a release that touched nothing.
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

// Sorted so the manifest's own bytes do not depend on emission order.
files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
const manifest = {
  // Bumped on any non-additive change to the entry shape, so a loader that does
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
const stageLine = ["text", "study", "analysis", "corpus", "optional"]
  .map((st) => `${st} ${byStage(st).length}/${mb(byStage(st).reduce((s, f) => s + f.gzBytes, 0))}MB`)
  .join(", ");
console.log(`pack ${manifest.version}: ${files.length} files, ${mb(total)}MB raw -> ${mb(totalGz)}MB gzipped`);
console.log(`  stages: ${stageLine}`);
