// Fails the build on a user-visible string literal outside the catalogue.
//
// This is the whole reason the catalogue holds: extraction is a one-day job and
// re-accumulation is a permanent one. Every shell in this repo drifted back to
// literals for a year because nothing ever said no, and a warning would have
// been read exactly as often as it was printed. So this EXITS NON-ZERO, and CI
// and `npm run check` both run it.
//
// ── What it looks for ────────────────────────────────────────────────────────
//
// Two shapes, because they are the two ways a sentence reaches a screen:
//
//   1. A TEXT NODE in Svelte markup — anything between tags that a reader would
//      read. `<button>Cancel</button>`.
//   2. A LITERAL in a user-facing ATTRIBUTE — aria-label, title, placeholder,
//      alt. `<input placeholder="Church name" />`. These are the ones that get
//      missed by eye, because they do not look like copy in a diff.
//
//   3. A LABEL PROPERTY in a script body — `{ key: "hymnal", label: "Hymnal" }`.
//      This one was added after it bit: the bottom bar's nav table survived the
//      whole extraction pass with one bare label in it, and neither review nor
//      the first two rules saw it, because a string in a script does not look
//      like copy. Only e2e/language.spec.ts caught it, and only because it
//      happened to assert that exact word.
//
// It deliberately does NOT read script bodies for bare strings in general.
// Almost every string in a script is a key, a class, a CSS value, an engine
// method name or a URL, so the false-positive rate would be high enough that the
// allowlist became the file. The property names below are the narrow exception:
// they are what a UI table calls its human-readable column, and nothing else.
//
// ── Escape hatch ─────────────────────────────────────────────────────────────
//
// `<!-- i18n-ignore: why -->` on the line before, or `// i18n-ignore: why` for a
// script line. For a run of lines, `i18n-ignore-start: why` … `i18n-ignore-end`.
// The reason is required and is not parsed — it is there for the next person,
// who will otherwise assume the exemption was a mistake.
//
// There is exactly one block exemption in the tree today, and it is the honest
// kind: the boot-diagnostics tables render only under the PERF build flag, so
// no reader in any language can reach them, and their contents are stage names
// out of the engine's own trace. Translating "worst single stall" would be
// translating a variable name.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const WEB = join(root, "apps/web/src");

/** Text that is not language: symbols, punctuation, numbers, single glyphs. */
const NOT_WORDS = /^[^\p{L}]*$/u;

/**
 * Words that are the same in every language we ship, and are not copy.
 *
 * A NAME is not a translation problem — "Plumbline" is the app, "KJV" is an
 * edition, "QR" is a format. Keeping them out of the catalogue keeps the
 * catalogue honest about what a translator actually has to do.
 */
const NAMES = new Set(["Plumbline", "KJV", "AKJV", "QR", "OK", "Aa", "plumblinebible.org", "OSIS"]);

/** Object properties that hold copy in a UI table — see rule 3 above. */
const LABEL_PROPS = ["label", "desc", "hint", "placeholder", "heading", "caption", "tooltip"];

/** Attributes a reader can hear or read. `title` included: it is a tooltip. */
const SPEAKING_ATTRS = ["aria-label", "placeholder", "title", "alt", "aria-valuetext", "aria-description"];

const files = [];
(function walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (name.endsWith(".svelte")) files.push(p);
  }
})(WEB);

const findings = [];

// ── every id the shell asks for exists ───────────────────────────────────────
// The other half of the contract. `t()` answers an unknown id with the id
// itself, which is the right runtime behaviour — visible, greppable, never a
// crash — and exactly the wrong build-time behaviour, because a typo then ships
// as a screen reading "settings.cpoyFormat". Template ids (`explore.${c.id}`)
// are skipped: they are assembled at runtime and their completeness is what the
// core's own catalogue tests are for.
const EN = JSON.parse(readFileSync(join(root, "crates/core/src/i18n/en.json"), "utf8"));
const unknown = [];

const sources = [...files];
(function walkTs(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkTs(p);
    else if (name.endsWith(".ts") && !name.endsWith(".generated.ts")) sources.push(p);
  }
})(WEB);

for (const file of sources) {
  const src = readFileSync(file, "utf8");
  const re = /\bt\(\s*"([^"]+)"/g;
  let m;
  while ((m = re.exec(src))) {
    if (!(m[1] in EN)) unknown.push({ file, id: m[1], line: src.slice(0, m.index).split("\n").length });
  }
}

for (const file of files) {
  const src = readFileSync(file, "utf8");
  const lines = src.split("\n");
  // Everything before the markup is script; everything inside <style> is CSS.
  // Both are found by index rather than parsed, which is enough: this checker
  // reports positions, it does not rewrite anything.
  const styleAt = src.lastIndexOf("\n<style");
  // LAST, not first: a component may open with `<script module>` and then its
  // instance script, and taking the first close treated the second script as
  // markup — which reported arithmetic as copy.
  const scriptEnd = src.lastIndexOf("</script>");

  // Lines inside an `i18n-ignore-start` … `i18n-ignore-end` run.
  const blocked = new Set();
  let inBlock = false;
  lines.forEach((l, i) => {
    if (/i18n-ignore-end/.test(l)) inBlock = false;
    else if (/i18n-ignore-start/.test(l)) inBlock = true;
    else if (inBlock) blocked.add(i);
  });

  const exempt = (lineIdx) => {
    if (blocked.has(lineIdx)) return true;
    for (let i = lineIdx; i >= 0 && i >= lineIdx - 2; i--) {
      if (/i18n-ignore(?!-)/.test(lines[i])) return true;
    }
    return false;
  };

  // The markup region: after the last script, before the style. Comments are
  // blanked (length-preserving, so reported line numbers stay true) — a
  // commented-out `aria-label` is not on anybody's screen.
  const markupFrom = scriptEnd < 0 ? 0 : scriptEnd + "</script>".length;
  const markupTo = styleAt < 0 ? src.length : styleAt;
  const stripped = src
    .slice(markupFrom, markupTo)
    .replace(/<!--[\s\S]*?-->/g, (s) => s.replace(/[^\n]/g, " "));

  // ── attributes ─────────────────────────────────────────────────────────────
  for (const attr of SPEAKING_ATTRS) {
    const re = new RegExp(`\\b${attr}="([^"{}]*)"`, "g");
    let m;
    while ((m = re.exec(stripped))) {
      const value = m[1].trim();
      if (!value || NOT_WORDS.test(value) || NAMES.has(value)) continue;
      const line = src.slice(0, markupFrom + m.index).split("\n").length - 1;
      if (exempt(line)) continue;
      findings.push({ file, line: line + 1, what: `${attr}="${value}"` });
    }
  }

  // ── label properties in the script ─────────────────────────────────────────
  const script = scriptEnd < 0 ? "" : src.slice(0, scriptEnd);
  for (const prop of LABEL_PROPS) {
    const re = new RegExp(`\\b${prop}\\s*:\\s*"([^"]*)"`, "g");
    let hit;
    while ((hit = re.exec(script))) {
      const value = hit[1].trim();
      if (!value || NOT_WORDS.test(value) || NAMES.has(value)) continue;
      const line = script.slice(0, hit.index).split("\n").length - 1;
      if (exempt(line)) continue;
      findings.push({ file, line: line + 1, what: `${prop}: "${value}"` });
    }
  }

  // ── text nodes ─────────────────────────────────────────────────────────────
  // Between a `>` and a `<`. Anything holding a `{` is an expression and is
  // somebody else's problem — `{t("x")} of {n}` is fine, and a mixed node like
  // `Page {n}` is still caught, because "Page" survives on its own.
  const nodes = /(^|>)([^<>{}]+)(?=<)/g;
  let m;
  while ((m = nodes.exec(stripped))) {
    const text = m[2].replace(/\s+/g, " ").trim();
    if (!text || NOT_WORDS.test(text) || NAMES.has(text)) continue;
    const line = src.slice(0, markupFrom + m.index).split("\n").length - 1;
    if (exempt(line)) continue;
    findings.push({ file, line: line + 2, what: text.slice(0, 60) });
  }
}

if (unknown.length) {
  console.error(`\ni18n: ${unknown.length} id${unknown.length === 1 ? "" : "s"} the catalogue does not define.\n`);
  for (const u of unknown) console.error(`  ${relative(root, u.file)}:${u.line}  t("${u.id}")`);
  console.error(`\nEach would render as its own id on screen. Define it in crates/core/src/i18n/en.json.\n`);
  process.exit(1);
}

if (findings.length) {
  console.error(`\ni18n: ${findings.length} user-visible string${findings.length === 1 ? "" : "s"} outside the catalogue.\n`);
  for (const f of findings) {
    console.error(`  ${relative(root, f.file)}:${f.line}  ${f.what}`);
  }
  console.error(
    `\nAdd the string to crates/core/src/i18n/en.json and render it with t("id").\n` +
      `If it genuinely is not copy — a name, a symbol, a developer-only diagnostic —\n` +
      `put "i18n-ignore: <why>" in a comment on the line above it.\n`,
  );
  process.exit(1);
}

console.log(`i18n: ${files.length} components, no stray user-visible strings, every id defined.`);
