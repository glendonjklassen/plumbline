#!/usr/bin/env node
// Assemble data/hymnal.json (format `hymnal-v1`) from the per-hymn source files
// in data-prep/hymnal/.
//
//   node scripts/build-hymnal.mjs [--check]
//
// The sources carry sourcing URLs and maintainer notes; the shipped file carries
// neither, because nothing in either shell reads them and they are a third of
// the bytes. Everything else is copied through untouched — this script does not
// edit anyone's words.
//
// IT ALSO VALIDATES, and refuses to write a hymnal it cannot vouch for. The
// checks are the ones a shell cannot make at runtime without failing in front of
// a reader: ids and numbers unique, every shipped language non-empty, and EVERY
// [bracket] a chord by the same grammar `crates/core/src/hymnal.rs` parses with.
// A bracket that is not a chord renders as literal text there — deliberately, so
// a typo is visible rather than swallowed — which means the only place that can
// catch one is here.
//
// `--check` validates and reports without writing, for CI.
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const src = join(repo, "data-prep/hymnal");
const out = join(repo, "data/hymnal.json");
const checkOnly = process.argv.includes("--check");

// ── the chord grammar, mirroring core::hymnal ────────────────────────────────
//
// Two copies of one grammar is a drift risk taken knowingly: this runs in Node
// at build time and that one runs in wasm at read time, and there is no shared
// artifact between them. The mirror is kept honest by the direction of failure —
// this side is STRICTER in effect, because anything it rejects never reaches the
// other. A chord the core would refuse and this accepts is the drift that
// matters, and `chord_grammar` in hymnal.rs pins the same token list.
const QUALITIES = [
  "mmaj7", "7sus4", "add11", "add13", "m7b5", "maj7", "dim7", "aug7", "sus2", "sus4",
  "add9", "add2", "maj", "min", "dim", "aug", "m7", "11", "13", "m", "6", "7", "9",
];

function parseRoot(s) {
  if (!/^[A-G]/.test(s)) return null;
  let rest = s.slice(1);
  if (rest[0] === "#" || rest[0] === "b") rest = rest.slice(1);
  return rest;
}

function validQuality(q) {
  outer: while (q.length) {
    for (const t of QUALITIES) {
      if (q.startsWith(t)) {
        q = q.slice(t.length);
        continue outer;
      }
    }
    return false;
  }
  return true;
}

function isChord(s) {
  const rest = parseRoot(s);
  if (rest === null) return false;
  const slash = rest.indexOf("/");
  const quality = slash < 0 ? rest : rest.slice(0, slash);
  const bass = slash < 0 ? null : rest.slice(slash + 1);
  if (!validQuality(quality)) return false;
  return bass === null ? true : parseRoot(bass) === "";
}

// ── load ─────────────────────────────────────────────────────────────────────

const problems = [];
const warnings = [];
const hymns = [];
const seenIds = new Set();
const seenNumbers = new Map();

const names = readdirSync(src)
  .filter((n) => n.endsWith(".json") && n !== "WORKLIST.json")
  .sort();

for (const name of names) {
  const where = `hymnal/${name}`;
  let doc;
  try {
    doc = JSON.parse(readFileSync(join(src, name), "utf8"));
  } catch (e) {
    problems.push(`${where}: not valid JSON — ${e.message}`);
    continue;
  }
  for (const field of ["id", "number", "tune", "meter", "key", "texts"]) {
    if (doc[field] === undefined || doc[field] === null) problems.push(`${where}: missing "${field}"`);
  }
  if (problems.length && doc.id === undefined) continue;

  if (seenIds.has(doc.id)) problems.push(`${where}: duplicate id "${doc.id}"`);
  seenIds.add(doc.id);
  if (seenNumbers.has(doc.number))
    problems.push(`${where}: number ${doc.number} already used by ${seenNumbers.get(doc.number)}`);
  seenNumbers.set(doc.number, doc.id);
  if (!isChord(doc.key) && !/^[A-G](#|b)?m?$/.test(doc.key ?? ""))
    problems.push(`${where}: key "${doc.key}" is not a key`);

  const texts = {};
  for (const [lang, t] of Object.entries(doc.texts ?? {})) {
    if (t === null) continue;
    if (!t.title) problems.push(`${where}: ${lang} has no title`);
    if (!t.author) problems.push(`${where}: ${lang} has no author`);
    if (!Array.isArray(t.stanzas) || t.stanzas.length === 0) {
      problems.push(`${where}: ${lang} has no stanzas`);
      continue;
    }
    if (!Array.isArray(t.sources) || t.sources.length === 0)
      warnings.push(`${where}: ${lang} cites no source`);

    // Every bracket, in every stanza and the chorus.
    const bodies = [...t.stanzas, ...(t.chorus ? [t.chorus] : [])];
    for (const body of bodies) {
      if (typeof body !== "string") {
        problems.push(`${where}: ${lang} has a non-string stanza`);
        continue;
      }
      for (const m of body.matchAll(/\[([^\]]*)\]/g)) {
        if (!isChord(m[1])) problems.push(`${where}: ${lang} has "[${m[1]}]", which is not a chord`);
      }
      // STRAIGHT APOSTROPHES ONLY (FORMAT.md). Hymn texts are full of them —
      // "'Tis", "pow'r", "e'er" — and the pages they are copied from disagree
      // about which character to print, so sourcing them faithfully produces a
      // book that is inconsistent line to line. Corrected across every file by
      // the maintainer on 2026-08-02; this is what keeps it corrected.
      if (body.includes("’")) {
        problems.push(`${where}: ${lang} uses a curly apostrophe (’) — FORMAT.md requires '`);
      }
    }
    // Stanza 1 carries the chart (FORMAT.md). A hymn with chords SOMEWHERE but
    // none on its first stanza paints an unchorded opening verse and a chorded
    // rest, which reads as a bug to a player.
    const anyChords = bodies.some((b) => typeof b === "string" && /\[/.test(b));
    if (anyChords && !/\[/.test(t.stanzas[0])) warnings.push(`${where}: ${lang} stanza 1 has no chords`);
    if (!anyChords) warnings.push(`${where}: ${lang} has no chords at all`);

    texts[lang] = {
      title: t.title,
      author: t.author,
      translator: t.translator ?? null,
      year: t.year ?? null,
      stanzas: t.stanzas,
      chorus: t.chorus ?? null,
    };
  }
  if (Object.keys(texts).length === 0) problems.push(`${where}: no language ships`);

  hymns.push({
    id: doc.id,
    number: doc.number,
    tune: doc.tune,
    meter: doc.meter,
    key: doc.key,
    texts,
  });
}

hymns.sort((a, b) => a.number - b.number);

// ── report ───────────────────────────────────────────────────────────────────

for (const w of warnings) console.warn(`  warn: ${w}`);
if (problems.length) {
  for (const p of problems) console.error(`  ERROR: ${p}`);
  console.error(`\nhymnal: ${problems.length} problem(s); refusing to write.`);
  process.exit(1);
}

// The work list is the table of contents; a hymn that never got written is a
// silent gap in the book, not an error in any one file.
const worklistPath = join(src, "WORKLIST.json");
if (existsSync(worklistPath)) {
  const wanted = JSON.parse(readFileSync(worklistPath, "utf8")).hymns ?? [];
  const missing = wanted.filter((w) => !seenIds.has(w.id));
  if (missing.length)
    console.warn(`  warn: ${missing.length} hymn(s) in the work list have no file: ${missing.map((m) => m.id).join(", ")}`);
}

const langs = {};
for (const h of hymns) for (const l of Object.keys(h.texts)) langs[l] = (langs[l] ?? 0) + 1;
const body = JSON.stringify({ format: "hymnal-v1", hymns });
const summary = `${hymns.length} hymns (${Object.entries(langs).map(([l, n]) => `${l} ${n}`).join(", ")}), ${(body.length / 1024).toFixed(0)} KB`;

if (checkOnly) {
  console.log(`hymnal ok: ${summary} (not written)`);
} else {
  writeFileSync(out, body);
  console.log(`hymnal ok: ${summary} → data/hymnal.json`);
}
