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
//   3. A BINDING CALLED `t` in a component that calls `t("id")` — see the rule
//      below. Not a stray string at all, but the same failure (a screen with no
//      words on it) and the only one nothing else in the toolchain can see.
//   4. A LABEL PROPERTY in a script body — `{ key: "hymnal", label: "Hymnal" }`.
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
// ── Kotlin ───────────────────────────────────────────────────────────────────
//
// Compose has no markup/script split, so there is nothing as cheap as a text
// node to look at. What it has instead is a small set of places a sentence can
// legally sit: the argument to `Text(...)`, a `contentDescription`, a
// `placeholder`/`label` lambda, and a `Toast`. Those are checked; a bare string
// anywhere else in Kotlin is not, for the same reason it is not in a `<script>`
// — most strings there are keys, ids and paths.
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
const ANDROID = join(root, "apps/android/app/src/main/java/dev/plumbline");

/** Text that is not language: symbols, punctuation, numbers, single glyphs.
 *
 *  Interpolations are stripped before the test, so `"$name $chapter"` and
 *  `"${i + 1}"` are correctly seen as holding no words of their own. The
 *  variables' CONTENTS are somebody else's problem — and usually the engine's,
 *  which already hands back a localized book name. */
const words = (s) => s.replace(/\$\{[^}]*\}|\$\w+/g, "");
const NOT_WORDS = (s) => /^[^\p{L}]*$/u.test(words(s));

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
  //
  // This is the one failure mode in this whole system that NOTHING else can see.
  // `{#each threads as t (t.name)}` makes `t` the thread inside the block, so
  // `t("thread.delete")` calls an object — and because these values are `any`,
  // svelte-check is silent, the guard's other rules are silent, and the component
  // renders as a blank surface at runtime. Two pickers shipped like that until
  // the full Playwright suite caught them (2026-08-03).
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

// ── Kotlin: the places a sentence can legally sit in Compose ─────────────────
const kotlinFiles = [];
(function walkKt(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkKt(p);
    else if (name.endsWith(".kt")) kotlinFiles.push(p);
  }
})(ANDROID);

/** A literal in one of these reaches a reader. `Text(` first: it is the bulk. */
const KOTLIN_SITES = [
  /\bText\(\s*"((?:[^"\\]|\\.)*)"/g,
  /\bcontentDescription\s*=\s*"((?:[^"\\]|\\.)*)"/g,
  /\b(?:placeholder|label)\s*=\s*\{\s*Text\(\s*"((?:[^"\\]|\\.)*)"/g,
  /\bmakeText\([^,]+,\s*"((?:[^"\\]|\\.)*)"/g,
  // A DEFAULT PARAMETER or a NAMED ARGUMENT whose name is copy-shaped:
  // `backLabel: String = "Back to reading"`, `title = "Weaves"`. Found by
  // mutation-testing this checker — a literal there reaches a screen, and none
  // of the patterns above look at a signature or a call's arguments.
  //
  // By NAME rather than by `: String =`, which was the first draft and was too
  // broad: every wire-JSON default (`standing: String = "unread"`) tripped it.
  /\b(?:backLabel|title|label|desc|hint|caption|placeholder|message|body|verb)\s*(?::\s*String\s*)?=\s*"((?:[^"\\]|\\.)*)"/g,
];

/**
 * Files the copy-shaped-name rule skips.
 *
 * `Wire.kt` is the wire layer: every string in it is a protocol value with a
 * matching `#[serde]` field on the Rust side — `"unread"`, `"verseRef"`,
 * `"system"` — and `ShareState.title` defaults to the same English the core's
 * `church::title` does, for a field whose real value always arrives from the
 * engine. Nothing in it is ever painted as written.
 */
const KOTLIN_WIRE = /\/Wire\.kt$/;

for (const file of kotlinFiles) {
  const src = readFileSync(file, "utf8");
  const lines = src.split("\n");
  // Comments blanked, length-preserving: a `Text("…")` in a doc comment is
  // documentation, and this checker had already rewritten some by hand once.
  const code = src
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
    .replace(/\/\/[^\n]*/g, (m) => " ".repeat(m.length));

  for (const id of code.matchAll(/\bt\(\s*"([^"]+)"/g)) {
    if (!(id[1] in EN)) unknown.push({ file, id: id[1], line: code.slice(0, id.index).split("\n").length });
  }

  for (const re of KOTLIN_SITES) {
    if (KOTLIN_WIRE.test(file) && re === KOTLIN_SITES[KOTLIN_SITES.length - 1]) continue;
    re.lastIndex = 0;
    let hit;
    while ((hit = re.exec(code))) {
      const value = hit[1].trim();
      if (!value || NOT_WORDS(value) || NAMES.has(value)) continue;
      // The END of the match, not its start. `Text(` and its string can be lines
      // apart, and blanked comments in between count as whitespace to `\s*` — so
      // anchoring on the start reported the wrong line AND looked for the
      // exemption comment above the wrong one.
      const line = code.slice(0, hit.index + hit[0].length).split("\n").length - 1;
      if (lines.slice(Math.max(0, line - 2), line + 1).some((l) => /i18n-ignore/.test(l))) continue;
      findings.push({ file, line: line + 1, what: value.slice(0, 60) });
    }
  }
}

// Two site patterns can match the same literal — `label = { Text("x") }` is both
// a `Text(` and a `label =`. One finding per place.
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

console.log(
  `i18n: ${files.length} components + ${kotlinFiles.length} Kotlin files, no stray user-visible strings, every id defined.`,
);
