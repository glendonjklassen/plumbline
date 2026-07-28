#!/usr/bin/env node
// Subset EB Garamond to what Plumbline can actually render, convert to woff2,
// and content-hash the filenames.
//
//   node scripts/subset-fonts.mjs        (run by `npm run pack:fonts`)
//
// 1,605 KB of TTF becomes ~224 KB of woff2 — 86% off the cold boot, and the
// fonts are render-blocking, so it is 86% off the slowest part of a first visit.
//
// THE CORRECTNESS CONSTRAINT, which is why the charset below is generous:
// chapter layout is measured in the ENGINE WORKER over an OffscreenCanvas, and
// the shell PAINTS the resulting display list on the main thread. Both contexts
// load the same file, so they agree — but only if the file has every glyph the
// text can contain. A missing glyph means one context measures a fallback font's
// advance width and the other paints Garamond's, and the line wraps somewhere it
// isn't drawn. A few KB of unused glyphs is nothing against that, so the ranges
// are whole Unicode blocks rather than a tight codepoint list.
//
// The ranges were DERIVED, not guessed (2026-07-28). Every non-ASCII codepoint
// in data/kjv.jsonl, data/kjv-notes.jsonl, data/akjv.jsonl, data/strongs.json and
// apps/web/src was collected and checked against the subset:
//
//   - reader text needs 104 codepoints total: ASCII, æ Æ, U+2019, U+2026, U+2014,
//     and 22 Hebrew letters (Psalm 119's acrostic stanza headings);
//   - Strong's is the demanding one — 69k Greek, 5.5k polytonic Greek Extended,
//     114k Hebrew, plus modifier letters U+02BB/U+02BC in the transliterations —
//     and it renders in EB Garamond, because app.css sets it on `body` and the
//     study panel does not override it;
//   - EB Garamond contains NO Hebrew at all (0 of 27 letters, 0 of the points),
//     so every Hebrew glyph already came from a system fallback before this and
//     still does. Subsetting cannot regress what the font never had.
//
// Verified after subsetting: zero advance-width differences across every kept
// glyph, GPOS/GSUB retained (kerning and ligatures affect measured widths too),
// the `wght` variable axis retained — the faces are VARIABLE and the CSS
// declares `font-weight: 400 700`, so instancing them would silently kill bold —
// and no codepoint that we use and the font has was dropped.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const web = join(here, "..");
const outDir = join(web, "public/fonts");
const srcDir = join(web, "fonts-src");

/** Whole blocks, deliberately — see the header. */
const UNICODES = [
  "U+0020-007E", // Basic Latin
  "U+00A0-00FF", // Latin-1 Supplement: æ Æ × ± § ·
  "U+0100-017F", // Latin Extended-A
  "U+02B0-02FF", // Spacing Modifiers: U+02BB, U+02BC in Strong's xlit
  "U+0300-036F", // Combining Diacriticals
  "U+0370-03FF", // Greek and Coptic — Strong's lemmas
  "U+1D00-1D7F", // Phonetic Extensions: U+1D49 in Strong's pron
  "U+1E00-1EFF", // Latin Extended Additional — transliteration
  "U+1F00-1FFF", // Greek Extended — polytonic Strong's lemmas
  "U+2000-206F", // General Punctuation: – — ' ' " " … ‹ › •
  "U+2070-209F", // Super/subscripts
  "U+20A0-20BF", // Currency
  "U+2190-21FF", // Arrows: →
  "U+2200-22FF", // Math: ≈ ≡ −
  "U+2500-257F", // Box drawing: ─
  "U+25A0-25FF", // Geometric: ▸
  "U+2700-27BF", // Dingbats: ✦ ✕
  "U+FFFD", // Replacement char (one occurrence in Strong's — a data artefact)
].join(",");

const FACES = [
  { src: "EBGaramond.ttf", base: "EBGaramond", style: "normal" },
  { src: "EBGaramond-Italic.ttf", base: "EBGaramond-Italic", style: "italic" },
];

if (!existsSync(srcDir)) {
  console.error(
    `no font sources at ${srcDir}\n` +
      `The full TTFs live there (they are the build input, not a shipped asset).`,
  );
  process.exit(2);
}

// Clear previously generated faces so an old hash cannot linger and be served.
for (const n of readdirSync(outDir)) {
  if (/^EBGaramond.*\.woff2$/.test(n)) rmSync(join(outDir, n));
}
mkdirSync(outDir, { recursive: true });

const built = [];
for (const face of FACES) {
  const src = join(srcDir, face.src);
  if (!existsSync(src)) {
    console.error(`missing ${src}`);
    process.exit(2);
  }
  const subTtf = join(outDir, `${face.base}.subset.tmp.ttf`);
  execFileSync("pyftsubset", [src, `--unicodes=${UNICODES}`, `--output-file=${subTtf}`], {
    stdio: ["ignore", "inherit", "inherit"],
  });
  // pyftsubset --flavor=woff2 needs python brotli, which is not reliably present;
  // woff2_compress is, and produces the same thing. It writes <name>.woff2
  // beside its input.
  execFileSync("woff2_compress", [subTtf], { stdio: ["ignore", "inherit", "inherit"] });
  const woff2Tmp = subTtf.replace(/\.ttf$/, ".woff2");
  const bytes = readFileSync(woff2Tmp);
  // Content-hashed, for two concrete reasons: sw.js treats `/fonts/` as immutable
  // BY PATH, so a font replaced under the same name would be served from cache
  // forever; and the cache sweep exempts un-versioned entries, so an old face
  // could never be reclaimed.
  const hash = createHash("sha256").update(bytes).digest("hex").slice(0, 8);
  const name = `${face.base}-${hash}.woff2`;
  writeFileSync(join(outDir, name), bytes);
  rmSync(subTtf, { force: true });
  rmSync(woff2Tmp, { force: true });
  built.push({ ...face, name, bytes: bytes.length, from: readFileSync(src).length });
}

// ── the two generated consumers ──────────────────────────────────────────────
//
// ONE source of truth for the URLs. The worker builds FontFaces from them for
// MEASUREMENT and the document declares @font-face from them for PAINTING; if
// those two ever named different files, layout and paint would disagree.

const cssPath = join(web, "public/fonts.css");
writeFileSync(
  cssPath,
  `/* GENERATED by scripts/subset-fonts.mjs — do not edit.
   Lives in public/ and uses RELATIVE urls: Vite does not rebase absolute
   url("/fonts/…") in bundled CSS, so under a subpath host (GitHub Pages) those
   would 404. Relative resolves against this file, so any base works.

   font-display: swap, not block. These faces are render-blocking, and with
   "block" the boot splash — whose whole job is to say "we are working on it" —
   painted nothing until 1.6 MB of font arrived. The splash now asks for a system
   serif outright, so nothing swaps under the reader there; UI chrome swaps once,
   early. The reader itself is a canvas painted after document.fonts.load
   resolves, so it never renders in a fallback face. */
${built
  .map(
    (f) => `@font-face {
  font-family: "EB Garamond";
  src: url("./fonts/${f.name}") format("woff2");
  font-weight: 400 700;
  font-style: ${f.style};
  font-display: swap;
}`,
  )
  .join("\n")}
`,
);

const tsPath = join(web, "src/engine/fonts.generated.ts");
writeFileSync(
  tsPath,
  `// GENERATED by scripts/subset-fonts.mjs — do not edit.
//
// The reader face, as paths relative to the app base. The engine worker loads
// these into its own FontFaceSet to MEASURE layout; public/fonts.css declares
// the same files for the document to PAINT with. Same bytes in both, which is
// what keeps measured line breaks and painted line breaks identical.
export const READER_FONT_FILES = {
${built.map((f) => `  ${f.style}: "fonts/${f.name}",`).join("\n")}
} as const;

/** Both faces, for preloading and for the offline shell list. */
export const READER_FONT_PATHS: readonly string[] = Object.values(READER_FONT_FILES);
`,
);

const kb = (n) => (n / 1024).toFixed(0);
const totalFrom = built.reduce((s, f) => s + f.from, 0);
const totalTo = built.reduce((s, f) => s + f.bytes, 0);
for (const f of built) console.log(`  ${f.name}  ${kb(f.from)} KB TTF -> ${kb(f.bytes)} KB woff2`);
console.log(
  `fonts: ${kb(totalFrom)} KB -> ${kb(totalTo)} KB woff2 ` +
    `(${(100 - (totalTo / totalFrom) * 100).toFixed(0)}% off, both faces)`,
);
