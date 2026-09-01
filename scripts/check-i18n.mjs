// Fails the build (exits non-zero) on a user-visible string literal outside the
// catalogue. Run by CI and by `npm run check`.
//
// What it looks for:
//
//   1. A text node in Svelte markup — `<button>Cancel</button>`.
//   2. A literal in a user-facing attribute — aria-label, title, placeholder,
//      alt. These are the ones missed by eye: they do not look like copy.
//   3. A binding called `t` in a component that calls `t("id")` — see the rule
//      below.
//   4. A label property in a script body — `{ key: "hymnal", label: "Hymnal" }`.
//
// It deliberately does NOT read script bodies for bare strings in general:
// almost every string in a script is a key, a class, a CSS value, a method name
// or a URL, so the allowlist would become the file. The property names below are
// the narrow exception — what a UI table calls its human-readable column.
//
// Escape hatch: `<!-- i18n-ignore: why -->` on the line before, or
// `// i18n-ignore: why` for a script line; `i18n-ignore-start: why` …
// `i18n-ignore-end` for a run of lines. The reason is required and is not
// parsed — it is there for the next person, who would otherwise assume the
// exemption was a mistake.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const WEB = join(root, "apps/web/src");

/** Text that is not language: symbols, punctuation, numbers, single glyphs.
 *  Interpolations are stripped first, so `"$name $chapter"` holds no words of
 *  its own; what they contain is the engine's problem, and it already hands back
 *  localized book names. */
const words = (s) => s.replace(/\$\{[^}]*\}|\$\w+/g, "");
const NOT_WORDS = (s) => /^[^\p{L}]*$/u.test(words(s));

/** Names, not copy: the same in every language we ship, and kept out of the
 *  catalogue so it stays honest about what a translator has to do. */
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
// `t()` answers an unknown id with the id itself — right at runtime (visible,
// greppable, never a crash), wrong at build time, because a typo then ships as a
// screen reading "settings.cpoyFormat". Template ids (`explore.${c.id}`) are
// skipped: they are assembled at runtime, and the core's own catalogue tests
// cover their completeness.
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
  // Found by index rather than parsed, which is enough for reporting positions.
  const styleAt = src.lastIndexOf("\n<style");
  // LAST, not first: a component may open with `<script module>` and then its
  // instance script, and taking the first close treats the second script as
  // markup — which reports arithmetic as copy.
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
      if (!value || NOT_WORDS(value) || NAMES.has(value)) continue;
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
      if (!value || NOT_WORDS(value) || NAMES.has(value)) continue;
      const line = script.slice(0, hit.index).split("\n").length - 1;
      if (exempt(line)) continue;
      findings.push({ file, line: line + 1, what: `${prop}: "${value}"` });
    }
  }

  // ── `t` shadowed ───────────────────────────────────────────────────────────
  // A binding called `t` in a component that also calls `t("id")`.
  // `{#each threads as t (t.name)}` makes `t` the thread inside the block, so
  // `t("thread.delete")` calls an object — and because these values are `any`,
  // svelte-check is silent and the component renders blank at runtime. Nothing
  // else in the toolchain can see it.
  //
  // Reported wherever the binding appears, not only where the collision bites:
  // the fix is to rename the binding, and a component that binds `t` at all is
  // one edit away from the bug even if it does not call the lookup today.
  if (/\bt\(\s*"/.test(src)) {
    const shadows = [
      /\{#each\s+[^}]*\bas\s+t\b/g,
      /\bconst\s+t\s*=/g,
      /\blet\s+t\s*=/g,
      /\{#snippet\s+\w+\(\s*t\b/g,
    ];
    for (const re of shadows) {
      re.lastIndex = 0;
      let hit;
      while ((hit = re.exec(src))) {
        const line = src.slice(0, hit.index).split("\n").length - 1;
        if (exempt(line)) continue;
        findings.push({
          file,
          line: line + 1,
          what: `\`${hit[0].trim()}\` shadows the catalogue lookup t() — rename the binding`,
        });
      }
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
    if (!text || NOT_WORDS(text) || NAMES.has(text)) continue;
    const line = src.slice(0, markupFrom + m.index).split("\n").length - 1;
    if (exempt(line)) continue;
    findings.push({ file, line: line + 2, what: text.slice(0, 60) });
  }
}

// Two rules can match the same literal in one place. One finding per place.
const seen = new Set();
const unique = findings.filter((f) => {
  const k = `${f.file}:${f.line}:${f.what}`;
  if (seen.has(k)) return false;
  seen.add(k);
  return true;
});
findings.length = 0;
findings.push(...unique);

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
