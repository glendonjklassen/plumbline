#!/usr/bin/env node
// Build HYMNAL-REVIEW.md: every hymn as it would be sung, for the maintainer to
// correct before any of it ships.
//
//   node scripts/hymnal-review.mjs
//
// The texts are historical and public domain, but "public domain" is not the
// same as "right": stanza wording and stanza count vary between hymnals, German
// orthography varies by era, and the chord charts are an editorial reading of a
// tune rather than a transcription of anything. All of that is the maintainer's
// to settle, so this document exists to be marked up.
//
// What it surfaces, in the order it matters:
//   * anything the sourcing agents flagged, and anything structurally suspect,
//     collected at the TOP — a 92-hymn document nobody reads end to end still
//     has to put its problems where they will be seen;
//   * each hymn's full text per language, lyrics and chords on separate lines
//     the way a chart is actually read;
//   * the sources each text came from, so a disputed line can be checked
//     against what was actually consulted rather than re-litigated from memory.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const DIR = join(repo, "data-prep/hymnal");
const OUT = join(repo, "HYMNAL-REVIEW.md");

const worklist = JSON.parse(readFileSync(join(DIR, "WORKLIST.json"), "utf8")).hymns;
const files = readdirSync(DIR).filter((f) => f.endsWith(".json") && f !== "WORKLIST.json");
const byId = new Map();
for (const f of files) {
  try {
    const h = JSON.parse(readFileSync(join(DIR, f), "utf8"));
    byId.set(h.id ?? f.replace(/\.json$/, ""), h);
  } catch (e) {
    byId.set(f.replace(/\.json$/, ""), { __broken: String(e) });
  }
}

const LANG = { en: "English", de: "German" };

// ── the same chord grammar core::hymnal parses with ──────────────────────────
const QUALITIES = [
  "mmaj7", "7sus4", "add11", "add13", "m7b5", "maj7", "dim7", "aug7", "sus2", "sus4",
  "add9", "add2", "maj", "min", "dim", "aug", "m7", "11", "13", "m", "6", "7", "9",
];
const root = (s) => {
  const m = /^[A-G]/.exec(s);
  if (!m) return null;
  let rest = s.slice(1);
  if (rest[0] === "#" || rest[0] === "b") rest = rest.slice(1);
  return rest;
};
function isChord(s) {
  let rest = root(s);
  if (rest === null) return false;
  const slash = rest.indexOf("/");
  let bass = null;
  if (slash >= 0) {
    bass = rest.slice(slash + 1);
    rest = rest.slice(0, slash);
  }
  outer: while (rest.length) {
    for (const q of QUALITIES) {
      if (rest.startsWith(q)) {
        rest = rest.slice(q.length);
        continue outer;
      }
    }
    return false;
  }
  return bass === null ? true : root(bass) === "";
}

/** Split a line into (chord?, text) segments — core::hymnal::parse_line. */
function parseLine(line) {
  const segs = [];
  let text = "";
  let chord = null;
  let rest = line;
  for (;;) {
    const at = rest.indexOf("[");
    if (at < 0) {
      text += rest;
      break;
    }
    text += rest.slice(0, at);
    const after = rest.slice(at + 1);
    const end = after.indexOf("]");
    if (end >= 0 && isChord(after.slice(0, end))) {
      if (chord !== null || text) segs.push({ chord, text });
      chord = after.slice(0, end);
      text = "";
      rest = after.slice(end + 1);
    } else if (end >= 0) {
      text += `[${after.slice(0, end)}]`;
      rest = after.slice(end + 1);
    } else {
      text += `[${after}`;
      break;
    }
  }
  if (chord !== null || text || !segs.length) segs.push({ chord, text });
  return segs;
}

/** One line as a chart: chords on their own row, over the syllable they strike.
 *  This is how a chart is read, and it is the only rendering in which a chord
 *  landing on the wrong syllable is visible at a glance. */
function chartLine(line) {
  const segs = parseLine(line);
  if (!segs.some((s) => s.chord)) return [null, segs.map((s) => s.text).join("")];
  let top = "";
  let bottom = "";
  // Where the last chord NAME ended. Not `top.length`, which also counts the
  // padding that trails it — testing against that inserted a space at every
  // chord and split "Amazing" into "A mazing".
  let lastEnd = 0;
  for (const s of segs) {
    if (s.chord) {
      // The chord wants the column its syllable starts at. Where chords crowd —
      // short syllables under long names like G/D — the LYRIC stretches instead,
      // and the chord is never clipped: clipping turned two adjacent chords into
      // "D7G", a chord that does not exist, while a word with a space in it is
      // merely ugly and is what every monospace chart does.
      const col = lastEnd > 0 ? Math.max(bottom.length, lastEnd + 1) : bottom.length;
      while (bottom.length < col) bottom += " ";
      while (top.length < col) top += " ";
      top += s.chord;
      lastEnd = top.length;
    }
    bottom += s.text;
    while (top.length < bottom.length) top += " ";
  }
  return [top.replace(/\s+$/, ""), bottom.replace(/\s+$/, "")];
}

const problems = [];
function flag(id, what) {
  problems.push({ id, what });
}

/** GitHub's heading anchor, so the flag list can link into the document. */
function anchor(number, title) {
  return `${number}-${title}`
    .toLowerCase()
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

// ── walk the work list, in book order ────────────────────────────────────────
const sections = [];
let shipped = 0;
let withChords = 0;
const langCount = { en: 0, de: 0 };

for (const w of worklist) {
  const h = byId.get(w.id);
  if (!h) {
    flag(w.id, `**${w.number}. ${w.title}** — NO FILE. Not in the book.`);
    continue;
  }
  if (h.__broken) {
    flag(w.id, `**${w.number}. ${w.title}** — file does not parse: ${h.__broken}`);
    continue;
  }
  shipped++;
  if (h.number !== w.number) flag(w.id, `number is ${h.number}, the work list says ${w.number}`);

  const langs = Object.entries(h.texts ?? {}).filter(([, t]) => t);
  if (!langs.length) {
    flag(w.id, `**${w.number}. ${w.title}** — no text in any language.`);
    continue;
  }
  for (const want of w.langs ?? []) {
    if (!h.texts?.[want]) {
      flag(w.id, `${w.number}. ${w.title} — work list wants **${LANG[want] ?? want}**, file has none`);
    }
  }
  if (w.optEn && !h.texts?.en) {
    flag(w.id, `${w.number}. ${w.title} — German only; the hoped-for translation (${w.optEn}) was not sourced`);
  }

  // The hymn is named by the language it was WRITTEN in — the work list's first
  // language, not whichever key the JSON happens to put first. Otherwise "Nun
  // danket alle Gott" is filed under Winkworth's English title.
  const primary =
    langs.find(([l]) => l === (w.langs ?? [])[0]) ?? langs[0];
  const title = primary[1].title;

  const lines = [];
  lines.push(`## ${h.number}. ${title}`);
  lines.push("");
  const meta = [`Tune **${h.tune || "?"}**`, h.meter || "?", `key **${h.key || "?"}**`];
  lines.push(meta.join(" · "));
  if (!h.tune || !h.meter || !h.key) flag(w.id, `${h.number}. ${w.title} — missing tune, meter or key`);
  lines.push("");

  let anyChords = false;
  const ordered = [primary, ...langs.filter((e) => e !== primary)];
  for (const [lang, t] of ordered) {
    langCount[lang] = (langCount[lang] ?? 0) + 1;
    if (langs.length > 1) {
      lines.push(`### ${LANG[lang] ?? lang} — ${t.title}`);
      lines.push("");
    }
    const credit = [t.author, t.translator ? `tr. ${t.translator}` : null, t.year].filter(Boolean).join(", ");
    lines.push(`*${credit}*`);
    lines.push("");

    const stanzas = t.stanzas ?? [];
    if (!stanzas.length) flag(w.id, `${h.number}. ${w.title} (${lang}) — no stanzas`);
    const first = stanzas[0] ?? "";
    if (!/\[[^\]]+\]/.test(first)) {
      flag(w.id, `${h.number}. ${w.title} (${lang}) — stanza 1 carries no chords`);
    } else {
      anyChords = true;
    }
    // Every bracket, against the shipped grammar. build-hymnal.mjs refuses these
    // too — this is so a bad one is named next to the words it belongs to.
    for (const [i, st] of stanzas.entries()) {
      for (const m of st.matchAll(/\[([^\]]*)\]/g)) {
        if (!isChord(m[1])) flag(w.id, `${h.number}. ${w.title} (${lang}) stanza ${i + 1} — \`[${m[1]}]\` is not a chord`);
      }
    }

    lines.push("```");
    for (const [i, st] of stanzas.entries()) {
      lines.push(`${i + 1}.`);
      for (const line of st.split("\n")) {
        const [top, bottom] = chartLine(line);
        if (top) lines.push(top);
        lines.push(bottom);
      }
      lines.push("");
    }
    if (t.chorus) {
      lines.push("Refrain:");
      for (const line of t.chorus.split("\n")) {
        const [top, bottom] = chartLine(line);
        if (top) lines.push(top);
        lines.push(bottom);
      }
      lines.push("");
    }
    lines.push("```");
    lines.push("");

    const src = t.sources ?? [];
    if (!src.length) flag(w.id, `${h.number}. ${w.title} (${lang}) — no source recorded`);
    else lines.push(`Sources: ${src.map((u) => `<${u}>`).join(" · ")}`);
    lines.push("");
  }
  if (anyChords) withChords++;
  if (h.notes) {
    lines.push(`> **Sourcing note:** ${h.notes}`);
    lines.push("");
    // Nearly every hymn has a note, and most of them are just provenance —
    // which edition, which stanza order. Flagging all of them at the top would
    // bury the handful that are actually asking a question, so only the notes
    // that ADMIT to uncertainty come up, and only as a pointer to the hymn.
    const doubt = /best.effort|could not|weakest|not confirmed|expect the question|no PD source|uncertain|unable to/i;
    if (doubt.test(h.notes)) {
      flag(w.id, `[${h.number}. ${title}](#${anchor(h.number, title)}) — the sourcing note admits doubt (chart or wording)`);
    }
  }
  sections.push(lines.join("\n"));
}

const head = [
  "# Hymnal review",
  "",
  `${shipped} of ${worklist.length} hymns, ${langCount.en ?? 0} English and ${langCount.de ?? 0} German texts, ${withChords} charted.`,
  "",
  "Everything here is public domain, but that is a copyright fact, not an",
  "editorial one. Stanza wording and stanza count differ between hymnals, German",
  "orthography differs by era, and the chords are one playable reading of a tune",
  "rather than a transcription of any particular setting. Mark up anything that",
  "is wrong and it gets fixed at the source files in `data-prep/hymnal/`, which",
  "is what `data/hymnal.json` is built from.",
  "",
  "Chords are shown above the syllable they strike, which is where a wrong one",
  "shows up. Sources are the pages actually consulted.",
  "",
];

if (problems.length) {
  head.push("## Needs your eye");
  head.push("");
  for (const p of problems) head.push(`- ${p.what}`);
  head.push("");
} else {
  head.push("Nothing was flagged as structurally suspect.");
  head.push("");
}
head.push("---");
head.push("");

writeFileSync(OUT, head.join("\n") + sections.join("\n") + "\n");
console.log(
  `HYMNAL-REVIEW.md: ${shipped}/${worklist.length} hymns, ${problems.length} flagged`,
);
