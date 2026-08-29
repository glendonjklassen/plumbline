#!/usr/bin/env node
// Subset every bundled face to what Plumbline can actually render, convert to
// woff2, and content-hash the filenames.
//
//   node scripts/subset-fonts.mjs        (run by `npm run pack:fonts`)
//
// The reader picks TWO faces independently — one for scripture, one for the
// chrome (`core::font::Font`, config `textFont` / `chromeFont`) — so this script
// builds a FAMILY TABLE rather than one face. Each family is emitted whole; the
// browser downloads only the families a reader actually selects, because
// @font-face is lazy, so bundling four costs a reader who keeps the default
// exactly nothing over bundling one.
//
// THE CORRECTNESS CONSTRAINT, which is why the charset below is generous:
// chapter layout is measured in the ENGINE WORKER over an OffscreenCanvas, and
// the shell PAINTS the resulting display list on the main thread. Both contexts
// load the same file, so they agree — but only if the file has every glyph the
// text can contain. A missing glyph means one context measures a fallback font's
// advance width and the other paints the real face's, and the line wraps
// somewhere it isn't drawn. A few KB of unused glyphs is nothing against that,
// so the ranges are whole Unicode blocks rather than a tight codepoint list.
//
// The ranges were DERIVED, not guessed (2026-07-28). Every non-ASCII codepoint
// in data/kjv.jsonl, data/kjv-notes.jsonl, data/akjv.jsonl, data/strongs.json and
// apps/web/src was collected and checked against the subset:
//
//   - reader text needs 104 codepoints total: ASCII, æ Æ, U+2019, U+2026, U+2014,
//     and 22 Hebrew letters (Psalm 119's acrostic stanza headings);
//   - Strong's is the demanding one — 69k Greek, 5.5k polytonic Greek Extended,
//     114k Hebrew, plus modifier letters U+02BB/U+02BC in the transliterations;
//   - EB Garamond contains NO Hebrew at all (0 of 27 letters, 0 of the points),
//     so every Hebrew glyph already came from a system fallback before this and
//     still does. Subsetting cannot regress what a font never had, and the same
//     is true of the three faces added beside it: a family that has no polytonic
//     Greek simply keeps falling back for it, exactly as Garamond does for Hebrew.
//
// Verified after subsetting: zero advance-width differences across every kept
// glyph, GPOS/GSUB retained (kerning and ligatures affect measured widths too),
// and the `wght` variable axis retained — every family here is VARIABLE and the
// CSS declares `font-weight: 400 700`, so instancing them would silently kill
// bold. Fira Code's axis runs 300–700 with its DEFAULT at 300, which is why the
// declaration matters more than it looks: without an explicit 400 the browser
// would render the Light instance as regular text.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
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

// The bundled families, keyed by their `core::font::Font` token — the SAME
// tokens the config stores and Android's registry uses, so a face cannot be
// called one thing here and another there.
//
// A family with no italic entry does not get one. Fira Code ships no italic at
// all, and a sheared upright looks exactly like a sheared upright; the reader
// tells translator-supplied words apart by the palette's `added` tone instead
// (see `core::font`, and `Font::has_italic`, which is the same fact in Rust).
// `scale` is the face's optical size multiplier — the numbers are
// `core::font::Font::scale()`'s (crates/core/src/font.rs, the source of truth,
// where the x-height measurements and the half-correction rationale live) and
// must stay identical to them, like the tokens.
const FAMILIES = [
  {
    token: "eb-garamond",
    script: "latin",
    css: "EB Garamond",
    fallback: "Georgia, serif",
    scale: 1.0,
    faces: [
      { src: "EBGaramond.ttf", style: "normal" },
      { src: "EBGaramond-Italic.ttf", style: "italic" },
    ],
  },
  {
    token: "literata",
    script: "latin",
    css: "Literata",
    fallback: "Georgia, serif",
    scale: 0.89,
    faces: [
      { src: "Literata.ttf", style: "normal" },
      { src: "Literata-Italic.ttf", style: "italic" },
    ],
  },
  {
    token: "inter",
    script: "latin",
    css: "Inter",
    fallback: "system-ui, sans-serif",
    scale: 0.87,
    faces: [
      { src: "Inter.ttf", style: "normal" },
      { src: "Inter-Italic.ttf", style: "italic" },
    ],
  },
  {
    token: "fira-code",
    script: "latin",
    css: "Fira Code",
    fallback: "ui-monospace, monospace",
    scale: 0.88,
    faces: [{ src: "FiraCode.ttf", style: "normal" }],
  },
  {
    // The one STATIC family (no `wght` axis): four files where the others are
    // one or two. Each face declares its own single weight — a static 400
    // declared as `400 700` would make the browser paint bold text regular —
    // and the 700s are `chromeOnly`: the engine worker measures scripture,
    // which is never bold, so FONT_FILES (its load list) skips them while
    // fonts.css (the document's paint list) declares all four.
    token: "atkinson-hyperlegible",
    script: "latin",
    css: "Atkinson Hyperlegible",
    fallback: "system-ui, sans-serif",
    scale: 0.9,
    faces: [
      { src: "AtkinsonHyperlegible-Regular.ttf", style: "normal", weight: "400" },
      { src: "AtkinsonHyperlegible-Italic.ttf", style: "italic", weight: "400" },
      { src: "AtkinsonHyperlegible-Bold.ttf", style: "normal", weight: "700", chromeOnly: true },
      { src: "AtkinsonHyperlegible-BoldItalic.ttf", style: "italic", weight: "700", chromeOnly: true },
    ],
  },
  {
    // The naskh face that carries Arabic — see the note above the token
    // constants. Its ranges and layout features are its own: the Arabic block
    // instead of the Latin one, and EVERY layout feature rather than fontTools'
    // default set, because init/medi/fina/isol are what join cursive script and
    // subsetting them away would leave every letter in isolated form at a
    // different advance width than the shaped text.
    //
    // `font-weight: 400` and no italic. A static regular declared `400 700`
    // paints bold text regular — the lesson Atkinson's entry above records — so
    // this one declares what it is and lets the browser synthesize a bold for
    // chrome headings. Scripture is never bold, so nothing the engine measures
    // is affected.
    token: "amiri",
    script: "arabic",
    css: "Amiri",
    fallback: "serif",
    scale: 1.06,
    unicodes: ["U+0020-007E", "U+00A0-00FF", "U+0600-06FF", "U+200F", "U+2000-206F", "U+FB50-FDFF", "U+FE70-FEFF"].join(
      ",",
    ),
    layoutFeatures: "*",
    faces: [{ src: "Amiri-Regular.ttf", style: "normal", weight: "400" }],
  },
  {
    // Gurmukhi, for the Punjabi Bible. `layoutFeatures: "*"` for the same
    // reason Amiri needs it and more so: an Indic script is SHAPED, not just
    // laid out. `nukt` composes the base and its dot into one glyph, `half`
    // and `blwf` make the subjoined forms a conjunct is drawn with, `pres`
    // and `abvs` place the matras. Subsetting those away leaves every
    // conjunct as a row of separate letters at a different total advance than
    // the shaped text — the measure-and-paint disagreement in the header, at
    // the scale of most words in the language.
    //
    // The unicode range is the Gurmukhi block plus Latin: the app's chrome is
    // painted in the reader's face too, and the chrome carries version
    // numbers, "KJV", and the Latin font names in the picker.
    token: "noto-serif-gurmukhi",
    script: "gurmukhi",
    css: "Noto Serif Gurmukhi",
    fallback: "serif",
    scale: 0.82,
    unicodes: ["U+0020-007E", "U+00A0-00FF", "U+0964-0965", "U+0A00-0A7F", "U+2000-206F"].join(","),
    layoutFeatures: "*",
    faces: [{ src: "NotoSerifGurmukhi.ttf", style: "normal" }],
  },
  {
    // Devanagari, for the Hindi Bible — and the face Marathi or
    // Urdu-Devanagari would read too, which is why `core::font::Font::script`
    // is the column and the language is not.
    //
    // U+0964-0965 is the danda and double danda, which BOTH Indic corpora end
    // their sentences with and which live outside either script's own block.
    // A missing danda is a missing glyph on 27,354 Punjabi and 31,541 Hindi
    // verses, measured in one context and painted from a fallback in the other.
    token: "noto-serif-devanagari",
    script: "devanagari",
    css: "Noto Serif Devanagari",
    fallback: "serif",
    scale: 0.82,
    // The ONLY family here with a second variation axis, and nothing reads it:
    // the CSS never sets `font-stretch`, so every width but the default is
    // dead weight in a file the engine worker downloads before it can lay out
    // a line. Pinned rather than subsetted away, because `--unicodes` does not
    // touch `fvar` — 237 KB of woff2 against 130 KB, on a face every reader
    // fetches whether or not they read Hindi.
    pinAxes: { wdth: 100 },
    unicodes: ["U+0020-007E", "U+00A0-00FF", "U+0900-097F", "U+0964-0965", "U+200C-200D", "U+2000-206F"].join(","),
    layoutFeatures: "*",
    faces: [{ src: "NotoSerifDevanagari.ttf", style: "normal" }],
  },
];

// THE SCRIPT FALLBACK: a family that is bundled for everyone and offered to
// nobody who cannot read it.
//
// None of the five families above contains a single Arabic glyph, and for this
// app that is a CORRECTNESS bug rather than an ugly one — the same one the
// header warns about, at the scale of a whole script. Chapter layout is measured
// in the engine worker and painted on the main thread; with no Arabic in the
// selected face, the worker measures whatever system font its OffscreenCanvas
// falls back to and the main thread paints whatever the document falls back to.
// Those are not required to be the same font, and when they differ every line of
// the Van Dyck wraps somewhere it is not drawn.
//
// So Amiri is appended to EVERY family's fallback stack and loaded in both
// contexts unconditionally. Not a sixth token in the picker: the reader's choice
// is about the voice of the Latin text, and there is nothing to choose between
// here — it is the difference between rendering Arabic and not. Per-glyph
// fallback means a reader on EB Garamond gets Garamond for English and Amiri for
// Arabic out of one stack, which is exactly what a fallback list is for.
//
// Amiri because it is the naskh face that Arabic typography actually uses for
// scripture, and because it positions tashkeel properly — `svd1865.jsonl` is
// fully vocalized, and most faces either collide the marks or drop them.
// OFL 1.1, vendored at fonts-src/OFL-Amiri.txt.
/** The families that exist to carry a script none of the Latin faces has —
 *  bundled always, each offered only to readers of its own script. See
 *  `core::font::Font::offered_for`, which is the same fact in Rust and the one
 *  the pickers actually read.
 *
 *  A LIST, and it was a single token while Arabic was the only one. Two more
 *  scripts arrived at once and every consumer of the singular form was written
 *  as `token === SCRIPT_FALLBACK_TOKEN`, which is exactly the shape that
 *  answers "wrong" for the second member. */
const SCRIPT_FALLBACK_TOKENS = FAMILIES.filter((f) => f.script !== "latin").map((f) => f.token);
const SCRIPT_FALLBACK_CSS = FAMILIES.filter((f) => f.script !== "latin").map((f) => `"${f.css}"`).join(", ");

/** The face the app falls back to everywhere — must be a key of FAMILIES. */
const DEFAULT_TOKEN = "eb-garamond";

if (!existsSync(srcDir)) {
  console.error(
    `no font sources at ${srcDir}\n` +
      `The full TTFs live there (they are the build input, not a shipped asset).`,
  );
  process.exit(2);
}

// fontTools ships `pyftsubset` as a console script, but a distro install may
// expose only the module. Same code either way; take whichever is present so a
// machine with python3-fonttools and no scripts dir can still build the fonts.
const SUBSET = (() => {
  try {
    execFileSync("pyftsubset", ["--help"], { stdio: "ignore" });
    return { cmd: "pyftsubset", pre: [] };
  } catch {
    return { cmd: "python3", pre: ["-m", "fontTools.subset"] };
  }
})();

/** Pin a variable font's axes to fixed values, dropping the rest of the design
 *  space. Only used where an axis is present and unread — see `pinAxes`. */
const INSTANCER = ["-m", "fontTools.varLib.instancer"];

mkdirSync(outDir, { recursive: true });

// Clear previously generated faces so an old hash cannot linger and be served.
// Every woff2 in here is ours and is regenerated below, so the sweep is by
// extension rather than by a per-family name pattern — a family REMOVED from
// the table above must not leave its last build behind.
for (const n of readdirSync(outDir)) {
  if (n.endsWith(".woff2")) rmSync(join(outDir, n));
}

const built = [];
for (const family of FAMILIES) {
  for (const face of family.faces) {
    const src = join(srcDir, face.src);
    if (!existsSync(src)) {
      console.error(`missing ${src}`);
      process.exit(2);
    }
    const base = basename(face.src, ".ttf");
    const subTtf = join(outDir, `${base}.subset.tmp.ttf`);
    // Instancing runs BEFORE the subset: `--unicodes` does not touch `fvar`,
    // so pinning afterwards would leave the dropped axis's deltas in the
    // glyphs the subset kept.
    let input = src;
    let pinnedTmp = null;
    if (family.pinAxes) {
      pinnedTmp = join(outDir, `${base}.pinned.tmp.ttf`);
      execFileSync(
        "python3",
        [...INSTANCER, src, ...Object.entries(family.pinAxes).map(([a, v]) => `${a}=${v}`), "-o", pinnedTmp, "-q"],
        { stdio: ["ignore", "inherit", "inherit"] },
      );
      input = pinnedTmp;
    }
    const args = [
      ...SUBSET.pre,
      input,
      `--unicodes=${family.unicodes ?? UNICODES}`,
      `--output-file=${subTtf}`,
    ];
    if (family.layoutFeatures) args.push(`--layout-features=${family.layoutFeatures}`);
    execFileSync(SUBSET.cmd, args, { stdio: ["ignore", "inherit", "inherit"] });
    // pyftsubset --flavor=woff2 needs python brotli, which is not reliably
    // present; woff2_compress is, and produces the same thing. It writes
    // <name>.woff2 beside its input.
    execFileSync("woff2_compress", [subTtf], { stdio: ["ignore", "inherit", "inherit"] });
    const woff2Tmp = subTtf.replace(/\.ttf$/, ".woff2");
    const bytes = readFileSync(woff2Tmp);
    // Content-hashed, for two concrete reasons: sw.js treats `/fonts/` as
    // immutable BY PATH, so a font replaced under the same name would be served
    // from cache forever; and the cache sweep exempts un-versioned entries, so
    // an old face could never be reclaimed.
    const hash = createHash("sha256").update(bytes).digest("hex").slice(0, 8);
    const name = `${base}-${hash}.woff2`;
    writeFileSync(join(outDir, name), bytes);
    rmSync(subTtf, { force: true });
    rmSync(woff2Tmp, { force: true });
    if (pinnedTmp) rmSync(pinnedTmp, { force: true });
    built.push({ family, face, style: face.style, name, bytes: bytes.length, from: readFileSync(src).length });
  }
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
   painted nothing until the font arrived. The splash now asks for a system
   serif outright, so nothing swaps under the reader there; UI chrome swaps once,
   early. The reader itself is a canvas painted after document.fonts.load
   resolves, so it never renders in a fallback face.

   Every bundled family is declared; a browser downloads only the ones a
   selected face actually names, so the reader who keeps the default pays for
   one family. */
${built
  .map(
    (f) => `@font-face {
  font-family: "${f.family.css}";
  src: url("./fonts/${f.name}") format("woff2");
  font-weight: ${f.face.weight ?? "400 700"};
  font-style: ${f.style};
  font-display: swap;
}`,
  )
  .join("\n")}
`,
);

const byToken = new Map();
for (const f of built) {
  const e =
    byToken.get(f.family.token) ??
    {
      css: f.family.css,
      fallback: f.family.fallback,
      scale: f.family.scale ?? 1,
      script: f.family.script,
      files: {},
    };
  // FONT_FILES is the engine worker's MEASUREMENT load list; scripture is
  // never bold, so a chromeOnly face (Atkinson's static 700s) is declared in
  // fonts.css above but deliberately absent here.
  if (!f.face.chromeOnly) e.files[f.style] = `fonts/${f.name}`;
  byToken.set(f.family.token, e);
}

const tsPath = join(web, "src/engine/fonts.generated.ts");
writeFileSync(
  tsPath,
  `// GENERATED by scripts/subset-fonts.mjs — do not edit.
//
// Every bundled family, keyed by its \`core::font::Font\` token. The engine
// worker loads the SELECTED family's files into its own FontFaceSet to MEASURE
// layout; public/fonts.css declares the same files for the document to PAINT
// with. Same bytes in both, which is what keeps measured line breaks and
// painted line breaks identical.

/** A family's woff2 paths, relative to the app base. \`italic\` is absent for a
 *  face that ships none — see \`core::font::Font::has_italic\`. */
export type FontFiles = { readonly normal: string; readonly italic?: string };

export const FONT_FILES: Readonly<Record<string, FontFiles>> = {
${[...byToken]
  .map(
    ([token, e]) =>
      `  "${token}": { normal: "${e.files.normal}"${e.files.italic ? `, italic: "${e.files.italic}"` : ""} },`,
  )
  .join("\n")}
};

/** Token → the family name the @font-face rules declare (what a \`ctx.font\`
 *  string or a CSS \`font-family\` must name to get this face). */
export const FONT_CSS_FAMILY: Readonly<Record<string, string>> = {
${[...byToken].map(([token, e]) => `  "${token}": ${JSON.stringify(e.css)},`).join("\n")}
};

/** The face every axis falls back to — the shipped default, and the answer for
 *  a token this build does not know (a config written by a later build). */
export const DEFAULT_FONT = ${JSON.stringify(DEFAULT_TOKEN)};

/** Token → what to render in until the webfont lands, and for any codepoint the
 *  family lacks. A sans must not fall back to a serif: the substitution shows
 *  for one swap, and on a glyph the face is missing it is permanent.
 *
 *  Every Latin stack opens with ${SCRIPT_FALLBACK_CSS} — not a choice, and not
 *  in the picker: no selectable Latin face has a single Arabic, Gurmukhi or
 *  Devanagari glyph, and per-glyph fallback is what lets one stack serve Latin
 *  in the reader's chosen voice and each other script in a face that can
 *  actually shape it. They sit FIRST among the fallbacks and after the chosen
 *  family, so they only ever answer for codepoints the chosen family lacks. */
export const FONT_FALLBACK: Readonly<Record<string, string>> = {
${[...byToken]
  .map(
    ([token, e]) =>
      `  "${token}": ${JSON.stringify(
        e.script !== "latin" ? e.fallback : `${SCRIPT_FALLBACK_CSS}, ${e.fallback}`,
      )},`,
  )
  .join("\n")}
};

/** Token → the script that face can set, mirroring \`core::font::Font::script\`.
 *  The pickers show a face only where it matches the reader's language — see
 *  \`core::font::Font::offered_for\`, which is this rule in Rust. */
export const FONT_SCRIPT: Readonly<Record<string, string>> = {
${[...byToken].map(([token, e]) => `  "${token}": ${JSON.stringify(e.script)},`).join("\n")}
};

/** Script → the one face that sets it.
 *
 *  Latin maps to the shipped default and every other script to its single face
 *  — \`core::font\`'s \`each_non_latin_script_has_exactly_one_face\` is what
 *  makes that a table rather than a list, and Latin is the one script with more
 *  than one face, which is exactly why it is the one entry written by hand. */
export const SCRIPT_FACE: Readonly<Record<string, string>> = {
  latin: ${JSON.stringify(DEFAULT_TOKEN)},
${[...byToken]
  .filter(([, e]) => e.script !== "latin")
  .map(([token, e]) => `  ${JSON.stringify(e.script)}: ${JSON.stringify(token)},`)
  .join("\n")}
};

/** The script-fallback face, loaded by the engine worker ALONGSIDE whichever
 *  family is selected and declared in fonts.css for the document.
 *
 *  Unconditional on purpose. They could be loaded only when the open corpus
 *  needs them, but then the worker's font set would depend on which Bible is
 *  open, and the window where the two contexts disagree is exactly the window
 *  where a reader switches language. The same faces, always present in both. */
export const SCRIPT_FALLBACK_FILES: readonly string[] = ${JSON.stringify(
  built.filter((f) => f.family.script !== "latin" && !f.face.chromeOnly).map((f) => `fonts/${f.name}`),
)};

/** The same files, keyed by the token whose FontFace they build — the worker
 *  needs the family name to construct each one, and there is more than one now. */
export const SCRIPT_FALLBACK_BY_TOKEN: Readonly<Record<string, readonly string[]>> = {
${SCRIPT_FALLBACK_TOKENS.map(
  (t) =>
    `  ${JSON.stringify(t)}: ${JSON.stringify(
      built.filter((f) => f.family.token === t && !f.face.chromeOnly).map((f) => `fonts/${f.name}`),
    )},`,
).join("\n")}
};

/** Token → the face's optical size multiplier, mirroring
 *  \`core::font::Font::scale()\` (which holds the x-height measurements and the
 *  half-correction rationale). Applied at render time by reader/measure.ts's
 *  \`readerFontPx\` and composed into \`--uiScale\` for the chrome — NEVER
 *  written into \`config.bodySize\`. */
export const FONT_SCALE: Readonly<Record<string, number>> = {
${[...byToken].map(([token, e]) => `  "${token}": ${e.scale ?? 1},`).join("\n")}
};

/** EVERY built font file — FONT_FILES plus the chrome-only static bolds that
 *  the engine worker never measures with. This is the offline PRECACHE list
 *  (vite.config's shell manifest): a face the document can be asked to paint
 *  must be on the device, or "can I read offline" depends on whether bold
 *  chrome text ever rendered while the network was up. */
export const FONT_ALL_FILES: readonly string[] = [
${built.map((f) => `  "fonts/${f.name}",`).join("\n")}
];
`,
);

const kb = (n) => (n / 1024).toFixed(0);
for (const f of built) console.log(`  ${f.name}  ${kb(f.from)} KB TTF -> ${kb(f.bytes)} KB woff2`);
for (const [token, e] of byToken) {
  const fam = built.filter((f) => f.family.token === token);
  const to = fam.reduce((s, f) => s + f.bytes, 0);
  console.log(`${token}: ${fam.length} face(s), ${kb(to)} KB woff2 — "${e.css}"`);
}
const totalFrom = built.reduce((s, f) => s + f.from, 0);
const totalTo = built.reduce((s, f) => s + f.bytes, 0);
console.log(
  `fonts: ${kb(totalFrom)} KB -> ${kb(totalTo)} KB woff2 ` +
    `(${(100 - (totalTo / totalFrom) * 100).toFixed(0)}% off, ${built.length} faces across ${byToken.size} families)`,
);
