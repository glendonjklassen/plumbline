#!/usr/bin/env node
// Validate apps/web/public/pack against what the web loader expects.
//
//   node scripts/check-web-pack.mjs
//
// Why this exists: the producer (scripts/build-web-pack.mjs) is plain JS outside
// apps/web and outside its tsconfig, so `npm run check` type-checks the loader's
// PackManifest interface without ever seeing the code that writes the JSON. A
// mismatch — a stage the loader doesn't switch on, an entry with no hash, a file
// no stage claims — shows up only as an e2e boot timeout with no diagnostic,
// which is the slowest possible feedback loop for the change most likely to have
// one. This makes it a fast, loud, local failure instead.
//
// It also verifies INTEGRITY: every entry's hash is re-derived from the bytes on
// disk. That is what makes the hash trustworthy enough to key URLs on and to
// verify downloads against.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const packRoot = join(repo, "apps/web/public/pack");
const manifestPath = join(packRoot, "manifest.json");

/** The roles the loader understands: two fixed ones, plus a per-language corpus
 *  and dictionary keyed by code (`crates/core/src/i18n.rs` composes them). */
const ROLE_OK = /^(corpusCache|suggestedWeaves|(corpus|lexicon):[a-z]{2,3})$/;
const STAGES = new Set(["text", "study", "analysis", "optional"]);
const SEED_DIRS = new Set(["threads", "tags", "weaves"]);
const problems = [];
const fail = (msg) => problems.push(msg);

if (!existsSync(manifestPath)) {
  console.error(`no pack at ${relative(repo, manifestPath)} — run \`npm run pack:data\` first`);
  process.exit(2);
}
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

// ── shape ────────────────────────────────────────────────────────────────────

if (manifest.formatVersion !== 2) {
  fail(`formatVersion is ${JSON.stringify(manifest.formatVersion)}, expected 2`);
}
if (!/^[0-9a-f]{16}$/.test(manifest.version ?? "")) {
  fail(`version ${JSON.stringify(manifest.version)} is not 16 hex chars`);
}
if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
  console.error("manifest.files is missing or empty");
  process.exit(2);
}

const seen = new Set();
for (const f of manifest.files) {
  const at = `${f.path ?? "<no path>"}`;
  if (typeof f.path !== "string" || !f.path.includes("/")) fail(`${at}: path is not a dir/name string`);
  if (seen.has(f.path)) fail(`${at}: duplicate entry`);
  seen.add(f.path);
  if (!Number.isInteger(f.bytes) || f.bytes < 0) fail(`${at}: bytes is not a non-negative integer`);
  if (!Number.isInteger(f.gzBytes) || f.gzBytes < 0) fail(`${at}: gzBytes is not a non-negative integer`);
  if (!/^[0-9a-f]{16}$/.test(f.hash ?? "")) fail(`${at}: hash ${JSON.stringify(f.hash)} is not 16 hex chars`);
  if (!STAGES.has(f.stage)) fail(`${at}: stage ${JSON.stringify(f.stage)} not in {${[...STAGES]}}`);
  if (f.seedOnce !== undefined && f.seedOnce !== true) fail(`${at}: seedOnce must be true or absent`);
  if (f.seedOnce && !SEED_DIRS.has(f.path.split("/")[0])) {
    fail(`${at}: seedOnce outside the stock dirs {${[...SEED_DIRS]}} — it would be treated as user-authored`);
  }
  if (f.role !== undefined && !ROLE_OK.test(f.role)) {
    fail(`${at}: unknown role ${JSON.stringify(f.role)}`);
  }
  // The retired v1 tier flags. Loud, because a half-migrated producer is worse
  // than an old one: the loader would silently mis-tier the file.
  for (const dead of ["stock", "rnd", "cache"]) {
    if (dead in f) fail(`${at}: carries retired v1 flag \`${dead}\` — use stage/seedOnce/role`);
  }
}

// ── the invariants the loader depends on ─────────────────────────────────────

/** Every per-language role in this manifest, grouped. `corpus:<code>` and
 *  `lexicon:<code>` are composed from the language registry in
 *  `crates/core/src/i18n.rs`; this checker does not need to know which codes
 *  exist, only that each behaves. */
const byRole = new Map();
for (const f of manifest.files) {
  if (f.role && (f.role.startsWith("corpus:") || f.role.startsWith("lexicon:"))) {
    byRole.set(f.role, [...(byRole.get(f.role) ?? []), f]);
  }
}

const roles = manifest.files.filter((f) => f.role === "corpusCache");
if (roles.length !== 1) {
  fail(`expected exactly one role:"corpusCache" entry, found ${roles.length} — the fast open keys off it`);
} else if (roles[0].stage !== "text") {
  fail(`the corpusCache is stage ${roles[0].stage}; it must be "text" or the reader waits for it`);
}

// EVERY OTHER LANGUAGE'S CORPUS, same shape and the same two things to get
// wrong — checked per code rather than for German by name, so a language added
// to the registry is checked by having been added.
//
// One must NOT claim `corpusCache`: that role is how the loader finds the text
// to open at boot, and a second file claiming it would make which language
// opens depend on manifest order. And each must be `optional`, or every English
// reader downloads a Bible in a language they do not read before they can read
// Genesis.
//
// Their dictionaries ride the same install, so they carry the same constraint.
for (const [role, entries] of byRole) {
  const code = role.slice(role.indexOf(":") + 1);
  const kind = role.startsWith("corpus:") ? "corpus" : "lexicon";
  if (entries.length > 1) {
    fail(`expected at most one role:"${role}" entry, found ${entries.length}`);
    continue;
  }
  if (entries[0].stage !== "optional") {
    fail(
      `the ${code} ${kind} is stage ${entries[0].stage}; it must be "optional" — nobody reading another language should download it`,
    );
  }
}

// The suggested-weave bundle is found by role, like the corpus cache, and it
// must stay OFF the automatic path: it is the one thing in the pack a reader
// has to ask for, and a stage slip would silently put 110 KB back on boot.
const sugg = manifest.files.filter((f) => f.role === "suggestedWeaves");
if (sugg.length > 1) {
  fail(`expected at most one role:"suggestedWeaves" entry, found ${sugg.length}`);
} else if (sugg.length === 1) {
  if (sugg[0].stage !== "optional") {
    fail(`the suggested-weave bundle is stage ${sugg[0].stage}; it must be "optional" (the reader asks for it)`);
  }
  if (sugg[0].seedOnce) {
    fail(`the suggested-weave bundle carries seedOnce — it seeds when downloaded, not at open`);
  }
}

// The raw JSONL must NOT ship: with a corpus cache present nothing fetches it,
// and if it ever reached the home the engine would parse 19 MB and write a
// 37 MB cache back (see the note in build-web-pack.mjs).
for (const raw of ["data/kjv.jsonl", "data/luther1912.jsonl"]) {
  if (seen.has(raw)) {
    fail(`${raw} is in the pack — it is superseded by its corpus cache and is unsafe in the home`);
  }
}

// The concept embedding must NOT ship either. Removed 2026-07-30 with the two
// features that read it ("verses like this" and the concept map), so any entry
// here is 3.08 MB of download that nothing in the engine will ever open.
for (const p of seen) {
  if (p.startsWith("data/concept-vectors.")) {
    fail(`${p} is in the pack — the concept embedding was removed 2026-07-30 and has no reader`);
  }
}

// Every stage must be non-empty: an empty "text" stage means nothing to boot on.
for (const st of STAGES) {
  if (!manifest.files.some((f) => f.stage === st)) fail(`no files in stage "${st}"`);
}

// ── every file on disk is described, and every description is true ───────────

/** Every `*.gz` actually written under pack/, as dir/name (no .gz). */
function onDisk(dir = "") {
  const here = join(packRoot, dir);
  const out = [];
  for (const d of readdirSync(here, { withFileTypes: true })) {
    if (d.isDirectory()) out.push(...onDisk(join(dir, d.name)));
    else if (d.name.endsWith(".gz")) out.push(join(dir, d.name).replace(/\.gz$/, ""));
  }
  return out;
}
const disk = new Set(onDisk());
for (const p of disk) if (!seen.has(p)) fail(`${p}: on disk but not in the manifest — nothing will fetch it`);
for (const p of seen) if (!disk.has(p)) fail(`${p}: in the manifest but not on disk — the loader will 404`);

// Integrity: the hash and the sizes must describe the bytes that shipped.
for (const f of manifest.files) {
  const gzPath = join(packRoot, `${f.path}.gz`);
  if (!existsSync(gzPath)) continue; // already reported above
  const gz = readFileSync(gzPath);
  if (gz.length !== f.gzBytes) fail(`${f.path}: gzBytes says ${f.gzBytes}, file is ${gz.length}`);
  const raw = gunzipSync(gz);
  if (raw.length !== f.bytes) fail(`${f.path}: bytes says ${f.bytes}, decompresses to ${raw.length}`);
  const got = createHash("sha256").update(raw).digest("hex").slice(0, 16);
  if (got !== f.hash) fail(`${f.path}: hash says ${f.hash}, content is ${got}`);
}

// ── report ───────────────────────────────────────────────────────────────────

if (problems.length) {
  console.error(`pack ${manifest.version} FAILED ${problems.length} check(s):`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}
const mb = (n) => (n / 1048576).toFixed(1);
const gzTotal = manifest.files.reduce((s, f) => s + f.gzBytes, 0);
const stages = [...STAGES]
  .map((st) => {
    const fs = manifest.files.filter((f) => f.stage === st);
    return `${st} ${fs.length}/${mb(fs.reduce((s, f) => s + f.gzBytes, 0))}MB`;
  })
  .join(", ");
console.log(
  `pack ${manifest.version} ok: ${manifest.files.length} files, ${mb(gzTotal)}MB gzipped, hashes verified`,
);
console.log(`  stages: ${stages}`);
