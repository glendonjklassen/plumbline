#!/usr/bin/env node
// Align the American King James Version onto the KJV's frozen tokens and emit
// only what DIFFERS — `data/akjv.jsonl`.
//
//   node scripts/build-akjv-delta.mjs --akjv <AKJV.json> [--out data/akjv.jsonl]
//
// Why a delta and not a second corpus: the AKJV is a modernisation of the same
// text (thou→you, saith→says), verse for verse — 31,102 verses either way. So
// the overlay is expressible as "for this run of KJV tokens, the AKJV says
// this instead", which keeps `kjv.jsonl` and the frozen `kjv1769-tok2` stamp
// untouched, lets the reader swap words at layout time, leaves every Strong's
// code attached to the KJV token that owns it, and makes "show me the word
// this replaced" free — the original is still right there in the corpus.
//
// Spans, not single tokens, because a modernisation is not always 1:1
// ("thou shalt" → "you shall" is, "peradventure" → "perhaps" is, but some
// expand or contract). The codebase already thinks in token spans (renderings,
// weave spanA/spanB), so this fits the grain.
//
// Punctuation is deliberately NOT overlaid at the EDGES: a KJV token carries
// its own `pre`/`post` punctuation, the overlay replaces the WORD only, and a
// comma the AKJV moved is not a "re-rendering". But punctuation INSIDE a
// multi-token span is the replacement's own business — KJV "Verily, verily"
// becomes AKJV "Truly, truly", and dropping that comma would read as a typo.
//
// So a consumer renders a span [a,b] as:  pre(a) + replacement + post(b).
// The interior punctuation of the tokens a..b is dropped, because the
// replacement text carries whatever punctuation the AKJV put between its own
// words.
//
// The AKJV (Michael Peter Engelbrite, 1999) is public domain — see
// BIBLIOGRAPHY.md.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const arg = (name, fallback) => {
  const i = process.argv.indexOf(name);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
};
const akjvPath = arg("--akjv", null);
const outPath = arg("--out", join(repo, "data/akjv.jsonl"));
if (!akjvPath) {
  console.error("need --akjv <AKJV.json> (scrollmapper/bible_databases formats/json/AKJV.json)");
  process.exit(2);
}

/** The AKJV text carries paragraph pilcrows as standalone "words"; the KJV
 *  corpus carries paragraphs as a token FLAG, not as text. Left in, a trailing
 *  ¶ reads as an inserted word and lands a no-op delta on the last token. */
const clean = (t) => t.replace(/¶/g, " ");

/** Compare on letters and digits only: case and punctuation are not renderings. */
const norm = (w) => w.toLowerCase().replace(/[^a-z0-9]/g, "");
/** The bare word as it should appear in place of a KJV token's word. */
const surface = (w) => w.replace(/^[^\p{L}\p{N}]+/u, "").replace(/[^\p{L}\p{N}]+$/u, "");
/** A replacement phrase: interior punctuation kept, edges left to the KJV token
 *  whose `pre`/`post` still frames the span. */
const phrase = (ws) => {
  const kept = ws.filter((w) => surface(w));
  if (!kept.length) return "";
  const joined = kept.join(" ");
  return joined.replace(/^[^\p{L}\p{N}]+/u, "").replace(/[^\p{L}\p{N}]+$/u, "");
};

// ── the KJV side: the frozen tokens ───────────────────────────────────────────
const kjvLines = readFileSync(join(repo, "data/kjv.jsonl"), "utf8").split("\n").filter(Boolean);
const kjvHeader = JSON.parse(kjvLines[0]);
const kjvVerses = kjvLines.slice(1).map((l) => JSON.parse(l));

// ── the AKJV side, mapped onto the KJV's book ids by canonical position ───────
// Both are the 66 books in order, so the ordinal IS the mapping; matching on
// English names would only invent a table to get wrong ("Revelation of John").
const akjv = JSON.parse(readFileSync(akjvPath, "utf8"));
const kjvBookOrder = [];
for (const v of kjvVerses) if (kjvBookOrder.at(-1) !== v.b) kjvBookOrder.push(v.b);
if (kjvBookOrder.length !== akjv.books.length) {
  console.error(`book count differs: KJV ${kjvBookOrder.length}, AKJV ${akjv.books.length}`);
  process.exit(1);
}
/** "Gen 1:1" → the AKJV verse text. */
const akjvText = new Map();
akjv.books.forEach((b, i) => {
  const id = kjvBookOrder[i];
  for (const c of b.chapters) {
    for (const v of c.verses) akjvText.set(`${id} ${c.chapter}:${v.verse}`, v.text);
  }
});

/** Longest common subsequence over normalized words → matched index pairs. */
function lcsPairs(a, b) {
  const n = a.length;
  const m = b.length;
  const dp = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const pairs = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      pairs.push([i, j]);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) i++;
    else j++;
  }
  return pairs;
}

/** KJV token words + AKJV words → [startTok, endTok, replacement] spans. */
function deltaFor(kjvWords, akjvWords) {
  const pairs = lcsPairs(kjvWords.map(norm), akjvWords.map(norm));
  const out = [];
  let i = 0;
  let j = 0;
  const emit = (i0, i1, j0, j1) => {
    if (i0 === i1 && j0 === j1) return;
    const text = phrase(akjvWords.slice(j0, j1));
    if (i0 === i1) {
      if (!text) return; // nothing was really inserted (punctuation, a mark)
      // A pure insertion has no KJV token to hang on. Fold it into the
      // preceding token (or the following one at the start of a verse) so every
      // entry still addresses a real span — the reader tapping it sees the KJV
      // word that the inserted words now accompany.
      const at = i0 > 0 ? i0 - 1 : 0;
      const host = surface(kjvWords[at] ?? "");
      const merged = i0 > 0 ? `${host} ${text}` : `${text} ${host}`;
      const prev = out.at(-1);
      if (prev && prev[1] === at) prev[2] = `${prev[2]} ${text}`.trim();
      else out.push([at, at, merged.trim()]);
      return;
    }
    // A repeated word lets the LCS anchor on the OTHER occurrence, leaving two
    // identical words paired as a "replacement" ("day" -> "day"). Marking those
    // would put a dotted underline under a word that never changed, so compare
    // the span's own text and drop it when nothing actually differs.
    const wasNorm = kjvWords.slice(i0, i1).map(norm).join(" ");
    if (wasNorm === akjvWords.slice(j0, j1).map(norm).join(" ")) return;
    out.push([i0, i1 - 1, text]); // text "" is a deletion
  };
  for (const [pi, pj] of pairs) {
    emit(i, pi, j, pj);
    i = pi + 1;
    j = pj + 1;
  }
  emit(i, kjvWords.length, j, akjvWords.length);
  return out;
}

// ── walk the corpus ───────────────────────────────────────────────────────────
const lines = [];
let missing = 0;
let totalTokens = 0;
let changedTokens = 0;
let versesWithDelta = 0;
const sample = [];

for (const v of kjvVerses) {
  const key = `${v.b} ${v.c}:${v.v}`;
  const text = akjvText.get(key);
  const kjvWords = v.t.map((t) => t[1]);
  totalTokens += kjvWords.length;
  if (text === undefined) {
    missing++;
    continue;
  }
  const akjvWords = clean(text).split(/\s+/).filter(Boolean);
  const d = deltaFor(kjvWords, akjvWords);
  if (!d.length) continue;
  versesWithDelta++;
  for (const [s, e] of d) changedTokens += e - s + 1;
  lines.push(JSON.stringify({ b: v.b, c: v.c, v: v.v, d }));
  if (sample.length < 6 && d.length) sample.push({ key, d, kjv: kjvWords });
}

const header = {
  format: "overlay-akjv-v1",
  source: "American King James Version (Michael Peter Engelbrite, 1999) — public domain",
  tokenization: kjvHeader.tokenization,
  verses: versesWithDelta,
};
writeFileSync(outPath, [JSON.stringify(header), ...lines].join("\n") + "\n");

const pct = (a, b) => ((a / b) * 100).toFixed(1);
console.log(`wrote ${outPath}`);
console.log(`  verses with a re-rendering: ${versesWithDelta} / ${kjvVerses.length} (${pct(versesWithDelta, kjvVerses.length)}%)`);
console.log(`  KJV tokens re-rendered:     ${changedTokens} / ${totalTokens} (${pct(changedTokens, totalTokens)}%)`);
if (missing) console.log(`  verses with no AKJV text:   ${missing}`);
console.log("\n  samples:");
for (const s of sample) {
  for (const [a, b, t] of s.d) {
    const was = s.kjv.slice(a, b + 1).join(" ");
    console.log(`    ${s.key.padEnd(12)} "${was}" -> "${t}"`);
  }
}
