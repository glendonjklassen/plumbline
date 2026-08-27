#!/usr/bin/env node
// Assemble data/devotional.json (format `devotional-v1`) from the per-devotional
// source files in data-prep/devotional/.
//
//   node scripts/build-devotional.mjs [--check]
//
// The sources are produced by data-prep/devotional/import-docx.py from whatever
// the church hands over; this step is the repeatable one, so it is the one CI
// runs. Nobody's words are edited here — the script copies text through and
// spends its effort on the checks a shell cannot make at runtime without failing
// in front of a reader:
//
//   - ids unique, days 1..N without gaps, `days` matching the entries
//   - every entry has scripture, a non-empty reflection, and an activity
//   - every SECTION covers a real run of days, and the sections tile 1..N
//     exactly once — a gap would leave a day the study list cannot file
//   - every scripture range runs forwards and its book is a real OSIS id
//   - every shipped language complete for every entry, because a devotional
//     half-translated is worse than one not offered: it would strand a reader
//     mid-booklet in a language they did not choose
//
// `--check` validates and reports without writing, for CI.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const src = join(repo, "data-prep/devotional");
const out = join(repo, "data/devotional.json");
const checkOnly = process.argv.includes("--check");

const problems = [];
const fail = (where, msg) => problems.push(`${where}: ${msg}`);

/** The 66 OSIS ids, read from canon.rs so this cannot drift into accepting a
 *  book the engine will not recognise. The importer resolves names to ids; this
 *  is the second gate, on the file that actually ships. */
function canonIds() {
  const rs = readFileSync(join(repo, "crates/core/src/canon.rs"), "utf8");
  const ids = [...rs.matchAll(/Book \{ id: "([^"]+)"/g)].map((m) => m[1]);
  if (ids.length !== 66) throw new Error(`canon.rs: matched ${ids.length} books, expected 66`);
  return new Set(ids);
}

const BOOKS = canonIds();

function checkScripture(where, refs) {
  if (!Array.isArray(refs) || refs.length === 0) return fail(where, "no scripture");
  for (const r of refs) {
    if (!BOOKS.has(r.book)) fail(where, `unknown OSIS book id ${JSON.stringify(r.book)}`);
    if (!(r.chapter > 0) || !(r.verse > 0)) fail(where, "chapter and verse must be positive");
    if (r.end !== undefined && !(r.end > r.verse)) fail(where, `range ${r.verse}–${r.end} does not run forwards`);
  }
}

/** A language's text for one entry — all four fields or none. */
function checkEntryText(where, lang, tx) {
  if (typeof tx?.title !== "string" || !tx.title.trim()) fail(where, `${lang}: no title`);
  if (!Array.isArray(tx?.reflection) || tx.reflection.length === 0) fail(where, `${lang}: no reflection`);
  else if (tx.reflection.some((p) => typeof p !== "string" || !p.trim()))
    fail(where, `${lang}: an empty reflection paragraph`);
  if (typeof tx?.activity !== "string" || !tx.activity.trim()) fail(where, `${lang}: no activity`);
}

const devotionals = [];
const seenIds = new Set();

for (const file of readdirSync(src).filter((f) => f.endsWith(".json")).sort()) {
  const where = `data-prep/devotional/${file}`;
  const d = JSON.parse(readFileSync(join(src, file), "utf8"));

  if (!d.id) fail(where, "no id");
  else if (seenIds.has(d.id)) fail(where, `duplicate id ${d.id}`);
  seenIds.add(d.id);

  const entries = d.entries ?? [];
  const days = entries.map((e) => e.day);
  const expected = entries.map((_, i) => i + 1);
  if (String(days) !== String(expected)) fail(where, `days are not 1..N without gaps: [${days}]`);
  if (d.days !== entries.length) fail(where, `declared days ${d.days} but carries ${entries.length} entries`);

  // The languages this devotional claims are the ones the FIRST entry offers;
  // every other entry must offer exactly the same set. (Falling back per-entry
  // would mean a booklet that changes language halfway through.)
  const langs = Object.keys(entries[0]?.texts ?? {}).sort();
  if (langs.length === 0) fail(where, "no languages");
  for (const e of entries) {
    const at = `${where} day ${e.day}`;
    checkScripture(at, e.scripture);
    const has = Object.keys(e.texts ?? {}).sort();
    if (String(has) !== String(langs)) fail(at, `languages [${has}] differ from the booklet's [${langs}]`);
    for (const lang of langs) checkEntryText(at, lang, e.texts[lang]);
  }

  // Sections must tile the days exactly once — the study list files every day
  // under exactly one heading, so a gap or an overlap has no rendering.
  const covered = new Map();
  for (const s of d.sections ?? []) {
    if (!(s.from >= 1) || !(s.to >= s.from) || s.to > entries.length)
      fail(where, `section ${JSON.stringify(s.title)} covers days ${s.from}–${s.to}, outside 1–${entries.length}`);
    if (typeof s.title !== "string" || !s.title.trim()) fail(where, "a section with no title");
    for (let day = s.from; day <= s.to; day++) covered.set(day, (covered.get(day) ?? 0) + 1);
  }
  if (d.sections?.length) {
    const uncovered = expected.filter((day) => !covered.has(day));
    const doubled = [...covered].filter(([, n]) => n > 1).map(([day]) => day);
    if (uncovered.length) fail(where, `days in no section: [${uncovered}]`);
    if (doubled.length) fail(where, `days in more than one section: [${doubled}]`);
  }

  for (const lang of langs) {
    const tx = d.texts?.[lang];
    if (typeof tx?.name !== "string" || !tx.name.trim())
      fail(where, `${lang}: the devotional has no name — it is the label every list shows it under`);
    if (!Array.isArray(tx?.closing) || tx.closing.length === 0) fail(where, `${lang}: no closing note`);
  }

  devotionals.push(d);
}

if (devotionals.length === 0) fail("data-prep/devotional", "no source files");

// AT MOST ONE new-believer booklet. The welcome starts "the one flagged", so two
// flagged makes which booklet a new believer is handed depend on file order —
// exactly the accident the flag exists to prevent.
const flagged = devotionals.filter((d) => d.newBeliever === true).map((d) => d.id);
if (flagged.length > 1) fail("data-prep/devotional", `more than one newBeliever booklet: [${flagged}]`);

if (problems.length > 0) {
  console.error("devotional: refusing to build —");
  for (const p of problems) console.error(`  ${p}`);
  process.exit(1);
}

const total = devotionals.reduce((n, d) => n + d.entries.length, 0);
const langs = [...new Set(devotionals.flatMap((d) => Object.keys(d.texts ?? {})))].sort();
if (checkOnly) {
  console.log(`devotional: ${devotionals.length} devotional(s), ${total} days, languages [${langs}] — ok`);
} else {
  writeFileSync(out, JSON.stringify({ format: "devotional-v1", devotionals }) + "\n");
  console.log(`data/devotional.json: ${devotionals.length} devotional(s), ${total} days, languages [${langs}]`);
}
