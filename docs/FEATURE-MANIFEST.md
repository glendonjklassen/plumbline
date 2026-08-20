# Shell feature manifest — the parity contract

> **The shells are Android (Compose, the UX gold standard) and the web PWA
> (`apps/web`).** The GTK and WinUI desktop shells were retired and REMOVED from
> the tree (git history has them). Sections below that cite GTK `M:<line>` refs
> or name GTK/WinUI behaviours are kept as the historical spec of *what* each
> feature does — the line refs no longer resolve, and "deltas owed to GTK/WinUI"
> are void. Where a live delta exists it is between **Android and web**, and it
> is named as such.

> **Write behaviour, not intention.** Every claim here must be one a grep can
> settle: what the code *does*, and the file it does it in. "The binding has the
> endpoint" is not "the shell uses it" — say which one you mean. A stale delta
> (a feature claimed missing that has in fact shipped) costs the same as a
> missing one.

The canonical inventory of everything a Plumbline shell does, written so a
shell can be built **without re-surveying the repo**. Historically the GTK
shell was the reference implementation; line refs below (`M:<line>`) pointed
at its `main.rs`. Shells reach everything through the C ABI (`crates/ffi`) —
Android over the hand-written Kotlin JNA binding, the web over the wasm
build; the *Data* line under each feature names the endpoint(s).

Conventions used everywhere:

- **refKey** `"Gen 1:7"` (OSIS id + `ch:v`) is the wire form of a verse ref;
  `display` is the human form. `reading_key()` orders refs canonically.
- Token flag bits: `1` added (italic), `2` divine name, `4` title, `8` ¶.
- Timestamps are UTC `%Y-%m-%dT%H:%M:%SZ` (`now_stamp`, M:2258).
- Authoring endpoints return **null on success, else an error string**; after
  any write the engine reloads study data from disk — re-fetch, never mutate
  shell-side state.

## Type — two axes, independent of each other and of the theme

**Three orthogonal settings: theme (colour) × `textFont` (scripture) ×
`chromeFont` (the app's own controls).** Every combination is legal and there is
no pairing table anywhere — Fira Code scripture under Synthwave is a supported
choice, not an exotic one. Both faces default to EB Garamond, which is what every
reader had before, so nothing moves under an upgrade.

The VOCABULARY is core data (`crates/core/src/font.rs`): tokens
(`eb-garamond` · `literata` · `inter` · `fira-code`), each face's own name, and
`has_italic`. The FILES are shell assets, because delivery is a platform
concern — the APK bundles TTFs (`assets/fonts/`, byte-identical to the web's
build inputs in `apps/web/fonts-src/`), the web ships per-family subset woff2.
All four are SIL OFL and variable-weight; bold is the `wght` axis in both shells,
never a synthetic smear.

- **Web.** `scripts/subset-fonts.mjs` walks a family table and generates BOTH
  consumers from one source — `public/fonts.css` (@font-face per family) and
  `src/engine/fonts.generated.ts` (token → files, CSS family, fallback stack). The
  scripture face is per-thread state in `reader/measure.ts`: the ENGINE WORKER
  measures with it and the MAIN THREAD paints with it, so both are told the same
  token or lines wrap where they are not drawn. A change is `session.setTextFont`,
  which loads the face into both sides BEFORE bumping `layoutEpoch`. The chrome
  face is `--chrome-font` on the root, read by `body` in `app.css`.
- **The top bar's passage is 19, not 16** (both shells, 2026-08-13). The bar's
  height is set by its touch targets — 48dp of icon button on Android, the 44px
  floor on the web — and the passage is the thing the bar is ABOUT as well as
  its widest tap target, so at 16 it filled about a third of the bar and read as
  lost on a Pixel. Raising it does not grow the bar: measured, the web header is
  65px at either size. Android's copy ALSO takes the reader's text scale now,
  which it had never done (its labels were fixed sp while the web's chrome
  followed `--uiScale`), and the phone's nav group takes the row's spare width
  with the passage ellipsizing — a Compose Row cannot wrap the way the web
  header does, and what would run off the end is the ≡.
- **The passage ellipsizes its NAME, never its chapter number** (both shells,
  2026-08-16). "1 Corinthians 13" at a raised text size used to wrap the web
  phone bar's ⌕ and ≡ onto a second row: shrinkability alone cannot prevent
  that, because flex line-breaking uses an item's UNSHRUNK size. The web
  chapter nav now has a ZERO flex-basis AND `min-width: 0` (the automatic
  minimum would haul the full name back into line collection — a span's
  min-content contribution is its whole text even under overflow:hidden), so
  it can never be what tips the row: it grows into the spare width instead
  (the phone rule stands the spacer down), and the passage is two spans —
  `.pbook` ellipsizes, `.pchap` is `flex: none` and never leaves the screen.
  Android's `TopBar` and the fold-mode `PaneHeader` (which had NO overflow
  handling) split the passage into two `Text`s the same way: the name
  weights-and-ellipsizes, the number stays. Pinned by text-scale.spec.ts
  "a long book name ellipsizes instead of wrapping".
- **Android.** `ui/Fonts.kt` is the same table (same tokens); `Typography.kt`
  builds a `FontFamily` per face and `serifTypography` substitutes it into
  Material's whole scale, so a bare `Text(…, fontSize = 15.sp)` picks the chrome
  face up without naming it. The scripture face reaches the reader, Present,
  Memorize and the maps through `LocalTextFont` rather than a parameter threaded
  through every call site. Both caches are keyed BY FACE (`Keyed<T>`), since a
  process can legitimately be asked for two.
- **Weight is pinned explicitly at 400**, not left to a file's default instance:
  Fira Code's `wght` axis runs 300–700 and DEFAULTS to 300, so a face taken
  as-shipped renders Light as body text — in one family only, which reads as
  "that font just looks thin" rather than as a bug.
- **A face with no italic does not get one.** Fira Code ships none, and a
  synthesised italic is a sheared upright. Translator-supplied words
  (`FLAG_ADDED`) are then marked by the palette's `added` tone alone, which is
  present in every theme. `Font::has_italic` is the one place that fact lives;
  both shells ask it rather than deciding.
- **A STATIC family declares its own bolds.** Atkinson Hyperlegible (added
  2026-08-12, the Braille Institute's low-vision face) has no `wght` axis —
  its bold is its own file. `Font::static_bold` is where that fact lives: the
  web's @font-face declares each static face at its single weight (a static
  400 declared as `400 700` paints bold text regular) and lists the 700s as
  chrome-only (the engine worker measures scripture, which is never bold, so
  they stay out of its FONT_FILES load list); Android's `FontSpec` carries
  `bold`/`boldItalic` asset paths that `loadTypefaces` and `buildSerifFamily`
  prefer over driving the axis.
- **Per-face optical scale** — `Font::scale()` in the core, mirrored as
  `FONT_SCALE` (web, generated) and `FontSpec.scale` (Android): eb-garamond
  1.00 · literata 0.89 · inter 0.87 · fira-code 0.88 · atkinson-hyperlegible
  0.90. The bundled faces'
  x-heights differ enormously (as a fraction of the em: Garamond 0.400,
  Literata 0.507, Fira Code 0.525, Inter 0.546, Atkinson 0.496), so at the same px Inter read
  over a third larger than Garamond and a face switch changed the apparent
  size, not just the voice. The correction is deliberately PARTIAL — half way
  toward equal x-height, not all the way: full equalisation would render Inter
  at 13.2px when the slider says 18, which reads as the app ignoring the
  setting. The numbers are a starting point to be tuned by eye on-device.
  Applied at RENDER TIME only — `readerFontPx` (reader/measure.ts, the one
  place both the measuring worker and the painting main thread get their px),
  composed into `--uiScale` for the web chrome, `ReaderPane`'s `fontPx` and
  `serifTypography` on Android — and NEVER written into `bodySize`: the stored
  setting keeps the number the reader chose, or it would drift on every
  switch. The default face is exactly 1.0, so nothing moves for a reader who
  never opens the picker; an unknown token resolves to 1.0. The layout column
  is NOT compensated: characters-per-line still differ between faces because
  advance widths differ (Garamond 0.528 em vs Literata 0.662) — that is the
  face's voice, and correct.
- **Delivery.** Web: `@font-face` is lazy, so a reader downloads only the
  families they select; the boot PRELOAD names the default family only (a
  preload of four would compete with the data pack for bandwidth), while the
  offline PRECACHE names all of them (~1.1 MB once) so "can I read offline" never
  depends on whether a font fetch happened to be seen by the service worker.
  Android bundles all five in the APK.

**Provenance of the old rule, so it is not re-canonised:** "one face, chrome
included" was never a decision. `body { font-family: "EB Garamond" }` arrived in
the first web-shell scaffold commit (`747bc99`), Android's Roboto chrome was
later read as drift FROM it and matched to it, and this manifest wrote the result
up as design. The accommodations it needed — chrome reading small at the same
`sp`, sizes "to be re-tuned on-device" — were symptoms. It is a setting now.

One deliberate exception stays: the web boot splash asks for Georgia
(`App.svelte`), a face already on the device, so the very first paint waits on no
download — a font is still a network round trip the splash must not be behind.

Intro-pane text is enlarged on both shells (older eyes, and the smaller x-height
compounds it) — sizes and line heights up roughly 2 px / 2.5 sp over the body.

## Constants + styling (M:41–54, 3807–3823)

| name | value | meaning |
|---|---|---|
| MAX_COLUMN | 720 | text column cap; centre in wider panes (`ReaderPane.svelte` `MAX_COLUMN`, `ReaderPane.kt` `MAX_COLUMN_DP`) |
| MARGIN | 28 | text margin, all sides (`MARGIN_DP` on Android — logical units, density-scaled) |
| MIN/MAX/DEFAULT bodySize | 12 / 40 / 20 | the text-size slider in BOTH shells (`SettingsDialog.svelte`, `StudyScreen.kt` `valueRange = 12f..40f`). The config accepts a wider 6–96 (`config.rs`) so an old or hand-edited file is honoured, not clamped away |
| MAX_PANES | 1 / 2 / 3 | reading columns — **web only** (`session.maxPanes`): 1 below 701px, 2 to 1099px, 3 above. Android shows one pane, or two side by side on a fold opened flat (`FoldMode.kt`) |
| PANEL_WIDTH | 380 | the web's study sidebar, × the text-size setting, capped at 40vw (an unscaled 380 is 45% of an unfolded Pixel Fold, and the Bible is the point). Android's study surface is a bottom sheet (phone) or the second fold pane, so it has no width constant |
| PANEL_SHEET_MAX | 700 | the web width at or below which the study surface is a bottom sheet instead of a sidebar (`StudyPanel.svelte`). Matches `s.narrow` and the destination bar — see the foldable delta under **Word study panel** |
| OCC_SHOWN | 300 | concordance cap (`PANEL_OCC_CAP`, `crates/ffi/src/lib.rs`) |
| XREF_SHOWN | 40 | xref/link list caps (`LIST_CAP`, `crates/core/src/panel.rs`) |
| GLOSS_SAMPLE | 80 | verses sampled for the english gloss (`crates/ffi/src/lib.rs`) |
| LINK_INSET / YINSET | 14 / 5 px | connector gutter inset / clamp margin (`ConnectorsOverlay.svelte`) |

Palette: the one source is `plumbline_core::theme::palette(theme)`, served as
`plumbline_theme_palette_json` — **eighteen concrete themes** plus follow-system:
the built-ins (light / dark / night), the named editor presets (Solarized
Light/Dark, Gruvbox, Nord, One Dark, Sepia, Catppuccin Mocha/Latte, Tokyo Night,
Rosé Pine, Synthwave), and the house originals (Scriptorium — parchment with
rubricated accents, light; Blueprint — cyanotype; Phosphor — green CRT with
amber accents; High Contrast — the deliberate low-vision option, light).
Darcula was retired 2026-08-12 as a near-duplicate of One Dark; both `parse`s
alias the stored token there, so a config that holds it opens on its nearest
neighbour instead of snapping to System. Both shells paint reader + chrome +
study panel from the
returned values rather than any hex of their own, and pick the theme from a
dropdown (`ThemeChoice`), not a radio column. The navigator's reading-map tiles
(`read_unread`/`read_partial`/`read_done`) reuse each theme's own
gold/divine/tier_human, so the map always belongs to the active theme. Every
text role clears WCAG-AA on every surface across all eighteen — a core test
(`contrast::every_text_role_clears_aa_on_every_surface`), not a convention. The LIGHT values, which are the
shipped originals: paper `#fcf9f4`; ink `#211f1a`; gold accent `#9e7d38`;
added-word gray `#6b6862`; divine `#4d3326`; popup paper `#f2eee6`; pane-nav bg
`#efeae1`; canon-strip bg `#ebe6db`; section-header gold `#a0894a`; rule
`#d8cba8`; faded `#8a8276`; the four tier marks and the three reading-map hues
(see their sections). Font: EB Garamond, bundled by both shells and
byte-identical between them — upright 851,176 bytes, italic 754,468
(`apps/android/app/src/main/assets/fonts/EBGaramond-Regular.ttf` ≡
`apps/web/fonts-src/EBGaramond.ttf`).

## Wordy buttons are glyphs (both shells, via the catalogs)

Reasonable actions carry a glyph, not a sentence (maintainer UAT, 2026-08-18:
"add is just +"): `tag.add`/`thread.add` **＋**, `panel.yourNote` **✎**,
`panel.note` **✎**, `panel.removeEntry` **✕** — joining the glyphs already in
place (`＋ tag verse`, `✎ edit`, `✕ delete tag`, `⇔ compare`, `↑`/`↓`). The
VALUES live in the i18n catalogs (identical across en/de/es), so both shells
pick them up from the engine with no shell code. Words stay where they earn
their place: destructive verbs in confirm dialogs (`Delete tag`), the search
screen's `go to {passage}`, and context-menu items, which follow menu
convention.

## App icon — the woven cross (shipped in both shells)

A woven Latin cross: two vertical and two horizontal gold (`#9e7d38`) strands on
the reader's warm paper. Each shell holds its own copy in its own platform form —
there is no shared source file anymore (the desktop `plumbline.svg` went with the
GTK/WinUI shells).

- **Android** — an adaptive icon, and only that, because `minSdk = 26`:
  `res/mipmap-anydpi-v26/ic_launcher.xml` + `ic_launcher_round.xml` compose
  `res/drawable/ic_launcher_foreground.xml` (a 108dp `<vector>`) over
  `@color/ic_launcher_background` (`values/colors.xml`, `#fcf9f4`), wired by
  `AndroidManifest.xml`'s `android:icon` / `android:roundIcon`.
- **Web** — `apps/web/public/icon.svg` is the favicon (`index.html`);
  `icon-128.png` + `icon-256.png` are the install icons in
  `public/manifest.webmanifest`, whose `background_color` / `theme_color` are the
  same `#fcf9f4`.

Open product call, recorded in the drawable's own comment: the mark predates the
Plumbline name and carries no plumb-line imagery.

## Reader core

- **Layout is in the Rust core** — the shell provides a text-measure callback
  and paints the returned display list (`verseNumber` + `word` items with
  x/y/w/h, flags, strongs). The config both shells pass, identically
  (`ReaderPane.kt`, `engine.worker.ts`):
  `width = min(paneW − 2·sideMargin, 720)`, `lineHeight = (ascent+descent)·lineSpacing`,
  `spaceWidth` measured, `verseNumGap = space·1.4`,
  `paraIndent = lineHeight·0.9`, `paraSpacing = lineHeight·0.45`, `verseBreak`
  from `versePerLine`, `verseNumbers` from `verseNumbers`. `sideMargin` and
  `lineSpacing` are the reader's config values, defaulting to 28 and 1.35.
- **Two typography switches** (Settings › Advanced, both shells, both ON by
  default and both written as explicit booleans — an absent key is a config
  from before they existed, so every read is `!== false`):
  - `verseNumbers` is a **layout** input, not a paint flag. The number's box and
    the gap after it belong to the line, so a shell that simply declined to draw
    them would flow every verse around an invisible marker; the core emits no
    number items and reclaims the width (`LayoutConfig.verse_numbers`,
    `PlumblineLayoutConfig.verse_numbers`, +4 bytes on the by-value struct — the
    web hand-marshals it at offset 28 of 32). It is therefore in BOTH turn-cache
    keys (`engine.worker.ts`, Android's `ChapterKey`) and out of `font_identity`,
    with `verse_break`, since it cannot change a glyph's advance.
  - `addedItalics` is **paint only**. The measure callback is font-blind, so the
    engine measures supplied words upright either way and the layout is
    untouched — deliberately absent from both cache keys, present in Android's
    `ChapterPaintKey` (the recording bakes the face) and named as an explicit
    dependency of the web's paint effect (the draw is rAF-deferred, so a read
    inside it registers nothing). Off, the words stay marked by the `added` tone
    alone — the same fallback a face with no italic already gets.
- Paint: verse numbers **bold gold**; FLAG_ADDED italic gray; FLAG_DIVINE /
  FLAG_TITLE colors above. Hit-testing: `hit_test(x − margin_x, y − MARGIN)`.
  **No mark for a Strong's-tagged word** (both shells): a faint gold rule under
  every one, and since most words carry a Strong's number, amounted to
  underlining the Bible. Whether a word answers when tapped is
  learned once; the page does not need to keep saying it.
- **Search / goto band** (the jump target): gold α0.12 rect over the verse's
  lines, x from `margin−6`, width `col+12`; persists until that pane next
  navigates (M:2720–2740).
- *Data*: `plumbline_engine_layout_chapter` (+ `plumbline_layout_*`), `plumbline_engine_toc_json`.

## Multi-pane (M:1649–2113)

1–3 columns; each has a nav strip: prev/next chapter `‹ ›`, the passage
navigator (a "Go to…" button opening the book/chapter tap grids — it replaced the
desktop-era book dropdown + chapter spin, see Android notes), **＋** (only when
n<3; inserts a copy of this pane after itself, becomes active), **✕** (only when
n>1). **Active pane** = last touched (canvas click or nav interaction); gets a
2-px gold top border *only when >1 pane*; the header subtitle is just the passage
("John 3"). Search go-to, panel links, canon strip, chord/constellation clicks
all target the active pane. Navigation with a verse polls until the fresh layout
paints, then scrolls the verse into view. Chapter stepping **crosses book
bounds** in both shells (`session.stepChapter` / `StudyScreen.step`) — past a
book's last chapter enters the next at ch 1, before ch 1 the previous at its
last. Both walk the TOC to do it; `core::canon::adjacent_book` exists but no
shell calls it. **End-of-chapter overscroll** (both shells): the scroll range
runs until the chapter's LAST LINE reaches the TOP of the pane, not the usual
bottom-stop — real range, no rubber-band — so a reader lying down can raise
the tail of a chapter past whatever blocks the bottom of the screen
(maintainer ask, 2026-08-11; `maxScroll` in `ReaderPane.svelte` /
`ReaderPane.kt`).

**Delta:** three panes are a web thing (see MAX_PANES above). Android is one
fullscreen reader, or two panes on a fold opened flat.

**Delta — what decides a two-pane layout.** Android decides by FOLD POSTURE and
never by width: `computeUiMode` (`FoldMode.kt`) takes a width class and ignores
it on purpose, because the target foldable's inner display may not clear the
840dp "Expanded" breakpoint. The web has no posture to read, so it decides by
width alone. The two therefore answer differently on the same hardware, and that
is intended; what is NOT allowed is the web's own breakpoints disagreeing with
each other — the 701–900px band is where that once went wrong, and
`e2e/foldable.spec.ts` pins both ends of it.

## Chain-linked panes — same chapter, scrolled together (WEB ONLY)

Two panes on the SAME book+chapter grow a **⛓︎** toggle in the pane strip (next
to ＋/✕; the motivating case is the same chapter in two languages, maintainer
UAT 2026-08-18). While it is on, panes on that chapter scroll TOGETHER,
**verse-aligned rather than offset-copied**: the two texts run to different
heights, so the only sync that stays true down the column is "the verse under
your eye is the verse under theirs". `session.syncLinkedScroll(fromIdx)` maps
(top verse, fraction through its line box) via `paneVerseGeom` — the geometry
each pane already publishes for the connectors — and writes the partners'
`scrollY`; each pane's mirror effect raises its programmatic-scroll flag, so a
linked move never echoes back as a user scroll (called from ReaderPane's
user-scroll branch and Shell's keyboard `scroll()` only). Above verse 1 the raw
offset is mirrored (chapter-heading zone). One GLOBAL toggle, session-only, not
persisted — a link is a reading posture, not a setting. Panes on other chapters
never move. E2E: `linked-scroll.spec.ts` (incl. a Luther pane asserting
verse-alignment against its own geometry, with a precondition that the two
layouts measurably differ).

**SHELL DELTA — Android:** single-pane by design; nothing to chain. Revisit only
if Android ever grows split view.

## Ambient weave connectors (M:2821–2934) — WEB ONLY

**Delta: this is a web feature.** `ConnectorsOverlay.svelte`
is the only consumer of `plumbline_engine_link_pairs_json` in the tree. Android
*declares* the endpoint (`StudyEngine.kt` `LinkPairsJson`) and never calls it — no
connector overlay, and no in-pane weave gutter dot either (`ReaderPane.kt` paints
a gutter dot for personal notes only). That is defensible on a phone, where one
pane means there is nothing to draw a line between; the endpoint being in the
binding does not mean the feature is in the shell.

Transparent overlay above the pane row, input-transparent, redrawn on scroll /
navigate / zoom / rebuild (60 ms delay) / authoring. The deduped canonical
link pairs come from the **core view-model** `plumbline_engine_link_pairs_json`
(each endpoint located + a `resolved` flag) — the shell re-derives no dedup, and
filters to `resolved`. For each pair: map both
endpoints' `(book, chapter)` to showing panes
(later pane wins duplicates); skip unless both shown in *different* panes.
Endpoint y = the verse-number item's `MARGIN + y + h/2` in pane space →
overlay coords; endpoint x = left pane's right edge − 14 / right pane's left
edge + 14 (connectors ride the gutter). Clamp y into the pane's visible band
±5 so an off-screen end lingers as an edge dot. Draw: cubic Bézier, ctrl pts
`(x1+dx·0.4, y1)`/`(x2−dx·0.4, y2)`, stroke gold α0.35 width 1.5; dots r2
gold α0.7. **In-pane gutter dots**: any verse with weave partners gets a gold
dot α0.75 r2.3 at `x=margin−9`, next to the verse number (M:2712).
*Data*: `plumbline_engine_weaves_json` (client folds into pair/xref indexes).

## Hover gloss (M:1744, 3582) — WEB ONLY

Native tooltip timing (the scroll container's `title`, `ReaderPane.svelte`
`hoverTitle`); hit-test under pointer; only when the word has Strong's
refs. Per code: bold code, lemma, italic xlit, then `kjv` (fallback `def`)
trimmed to 80 chars. Asked of THE PANE'S OWN text (`qIn(pane.lang, "strongs",
…)`), so a German pane's gloss agrees with the study card a click opens. **Delta:** there is no hover on a touch screen, so Android
has none — a tap opens the study surface instead. *Data*:
`plumbline_layout_hit_test_json` + `plumbline_engine_strongs_json`.

## Word study panel (click a word; M:3168–3515)

The web's 380-px sidebar (scaled by the text-size setting, capped at 40vw) above
700px, and a bottom sheet at or below it; on Android a dismissible bottom sheet
on a phone, or the second pane on an opened fold.
On-demand; Esc / a swipe hides; clearing search hides. Content order — (F) marks
what the *machine* or *human* gate turns on (see **Per-tier analysis gates**;
"Full" is the name for both gates on).

**Structure — one core producer, thin per-block renderers (P0.1).** The whole
panel is now a **typed block list** built once in `plumbline_core::panel` and served
over `plumbline_engine_word_study_blocks_json` (+ the sibling `*_blocks_json`
endpoints below). A block is `Section{title, mark?}` / `Para{runs, indent,
topGap}` / `Rule`; a run carries text + a **semantic colour role** + a logical
point size + bold/italic + an optional `uri`. The producer owns *everything*
below — tier order, caps, (F)-gating, `humanize`, `RenderKey`, gloss/lemma
formatting, the reverse-lens line, the provenance marks, snippet windowing, and
the pre-baked link URIs. Each shell has a *small* per-block painter
(`RenderBlocks` / `blocks_to_markup`); it derives nothing. The `word` (the
surface that led here — marks its rendering, keys the reverse line) is a
producer argument; the same producer's `code_study_card` is what the
`code:CODE[:word]` verb opens standalone. The items below document *what the
producer emits*, not shell code.

1. Verse ref bold; the word xx-large.
2. (F) Morphology gloss line, small, `#6a5a2a`.
3. Per Strong's code (else "*no Strong's tag on this word*"):
   code bold + "**N occurrence(s) ▸**" → concordance; lemma x-large; xlit
   italic; pron `#888`; definition; `KJV: …` small; then (F) tiers with
   small-caps gold headers, each carrying an authority-tier provenance mark
   (see **Authority tiers** below):
   - **RENDERINGS** *(Human †)* — the other English words this code is translated as
     (corpus-derived, not R&D), most frequent first; the tapped word's own
     rendering is **bold**. Each chip shows `×count` and links
     `rend:CODE:rendering` → a concordance filtered to that rendering (cap
     OCC_SHOWN). When the reverse lens maps the tapped surface word to >1 code,
     a "“word” also translates …" line (`#6b6862`) links the other codes via
     **`code:CODE:word`** → that code's study card (its entry + its own tiers),
     **not** `occ:` — the reader arrives at a code they don't know understanding
     it, not a verse list. New feature — no overlay antecedent. *Data*:
     `plumbline_engine_renderings_json` + `plumbline_engine_word_codes_json`.
   - **SAME ROOT ACROSS TESTAMENTS** *(per-chip marks)* — bridge partners (≤6)
     as gloss chips → concordance links; sources humanized in the core by
     `bridge::source_label` (`lxx`→Septuagint, `quotation`→NT quotation,
     `abbott-smith`→Abbott-Smith (1922), …), never shell-side; then this chip's
     provenance marks from the union of
     its sources' tiers (✝/†/≈, + ⚗ if any source is research-grade); "· disputed
     by usage" in `#b04a3a` when the text-witness disbelieves (shipped data never
     grades, so silent).
   - **SIMILAR CONCEPTS** — **REMOVED** (embedding neighbours plus an "across the
     testaments" cross list). Cut from `panel.rs`, so it went from both shells at
     once, along with the `concept_near` trait method and its FFI implementation,
     which had no other caller. The function-word filter this section needed
     (`rnd::stopwords`, so *believe* does not offer *because*) is still live for
     APPEARS ALONGSIDE below. The embedding index it read went too, with "verses
     like this" (item 7) and the concept map.
   - **APPEARS ALONGSIDE** *(Machine ≈)* — concept community (8), same
     function-word filter.
   - **MOST USED IN** *(Machine ≈)* — top books (5) "Book ×N · …" + "(OT x · NT y)".
   - **LEITWORT** *(Machine ≈)* — "{winCount} of its {n} uses cluster in {label} (p ≈ 10^−{score})".
   - **"▸ open concept map" link** — **REMOVED** with the popup it opened; see
     §Concept map popup below. The three sections above it are the symbolic
     concept engine (co-occurrence over the corpus) and are untouched.
4. (F) Author actions: `＋ tag verse`, `＋ add to thread`.
5. **in N weaves** — which weaves this verse BELONGS TO, each linking to its
   compare card. Distinct from the partner list below it, which answers a
   different question: a verse with six links into one weave is one membership,
   and the partner list buries that under six rows repeating a name. Derived
   from the partners (a weave is a graph of verse↔verse links, so a verse in one
   appears in at least one link), deduped by NAME, first-seen order.
6. **cross-references (N)** — weave partners (≤40), each + weave-name link to
   its compare card.
7. (F) **study cross-references (N) — TSK** *(Human †)* (≤40; ranges "a–b").
8. **verses like this** — **REMOVED** (a per-verse list of statistically similar
   verses — the SIF model over the concept embedding, 6 in-testament and 4
   cross-testament — judged machine-generated noise). It lived in `panel.rs` and
   in the core's `VerseSim`, so it went
   from both shells at once, taking `PanelSource::similar_verses`,
   `plumbline_engine_similar_verses_json` and the wasm-only
   `plumbline_engine_verse_sim_save` / `_load` / `_step` with it. It was the last
   feature reading the concept embedding, so `data/concept-vectors.vec` (+
   `.vecb`, `.meta`, `.freq`) left the data pack too: 3.08 MB of a 6.4 MB
   analysis tier, which now holds morphology and text-witness only. The last
   code path that opened the file — `plumbline_engine_concept_neighbours_json`,
   which no shell ever called — is gone too (see §C ABI surface).
9. (F) **tags** — tags holding this verse; each is a link + `✕` untag (user
   data, not evidence — no tier mark).
10. **margin notes** *(Human †)* — the verse's 1769 notes, small.

A **provenance legend** closes a Full-study card once: "where this comes from:
✝ the text · † curated scholarship · ≈ machine-derived, weigh it · ⚗
research-grade". Weave membership + cross-references (items 5–6) and tags carry no mark (mixed /
user-authored, not trust-weighted evidence). The producer emits it as a `Para`
of tier-coloured runs.

Concept chips render english-first: "**gloss** *lemma*" joined by "  ·  "; the
gloss is the modal KJV rendering across ≤80 occurrences (skip FLAG_ADDED tokens,
strip edge punctuation, ties lexicographic; fallback: distilled def/kjv clause
≤30 chars). All of this is inside the producer; the shell only paints the runs.
The `PanelSource` trait is the producer's only input — a thin set of projected
accessors (`strongs`/`occurrences`/`renderings`/`bridge_partners`/`concept`/
`verse_xrefs`/`verse_notes`/…). One live implementation,
`PlumblineEngine` (`crates/ffi/src/lib.rs`), so both shells get the identical
producer behind the endpoints; `panel/tests.rs`'s `Fake` is the second, which is
how the producer is unit-tested without a corpus.

## Plain-English overlay (the AKJV delta, both shells)

Where the **American King James Version** (Michael Peter Engelbrite, 1999,
public domain) words a verse differently from the KJV, the reader can see it —
off by default, behind a Settings toggle that hides itself when the home has no
overlay.

**A delta, never a second corpus.** `data/akjv.jsonl` keys each difference to a
run of KJV tokens: `[startTok, endTok, "you shall"]`. Same 31,102 verses either
way, so no versification mapping. Built by `scripts/build-akjv-delta.mjs`
(LCS word-diff, case- and punctuation-normalised); packed to `.akjvb` by
`plumbline-hydrate akjvb`; read by `core::akjv`. 6.9% of tokens in 66.7% of
verses; 179 KB gzipped, shipped as a stage-2 core file.

- **Token indices survive the overlay.** A run's first token takes the whole
  replacement and a display-only `FLAG_RERENDERED` (bit 16 — never in
  `kjv.jsonl`, whose bitfield is frozen); interior tokens are BLANKED, not
  removed, and the layout skips whatever renders to nothing. Rebuilding the
  vector would shift every later index and silently open the wrong Strong's
  entry on a tap.
- **Render rule:** `pre(a) + replacement + post(b)`. Edges belong to the KJV
  tokens framing the run; interior punctuation belongs to the replacement
  ("Verily, verily" → "Truly, truly"). `data-prep/README.md` states it too,
  because the producer has to agree with the consumer.
- **The mark is a DOTTED gold underline**, at the natural underline depth.
  Not bold and not grey: italic already means "supplied by the
  translator", a grey word is a de-emphasised one when the overlay makes it the
  word you are reading, and at 6.9% density a heavy mark reads as a ransom note.
  Dotted also survives a search band and works in greyscale.
- **Tapping** a marked word shows the AKJV wording and `KJV: …` under the
  headword, ABOVE the Strong's, because the codes are keyed to the KJV word. A
  multi-token run answers from any word inside it.
- **Integrity — this is what keeps "KJV-only" true.** The overlay is applied in
  exactly ONE place, on the way into `layout_chapter`. Verse text, copied text,
  memory drills, Present and shared links are the KJV by construction, and an
  e2e test asserts it with the overlay ON. A modernised word on a memory card
  would make this a second translation whatever About says.
- Applied engine-side (`plumbline_engine_set_akjv_overlay`) rather than per
  layout call, so two panes can never disagree; Android sets it inside the same
  lock as the layout for the same reason.
- **`plumbline_engine_open` does NOT load the overlay** — only
  `load_core_data` does, and the toggle hides itself while `akjv_available` is
  false. The web calls it in its background stage; **Android calls it straight
  after open**, before the engine reaches the UI, because every file is on local
  disk there and staging would only add a race. The requirement is kept next to
  the claim because a shell can hold every piece of this feature — binding,
  dotted mark, tap header, Settings toggle — and still ship it invisible if
  nothing calls that one function.

## Authority tiers — provenance marks on evidence

Ported from overlay `Bridge.hs` `Tier` + `Panels.hs` `provIcon`/`tierMarks`.
Every piece of study evidence shows where it comes from, so the reader always
knows its provenance. The model is `plumbline_rnd::bridge` (`crates/rnd/src/bridge.rs`):

- `Tier = God | Human | Machine`. **God** = the text itself (TR/Masoretic words,
  and scripture-quotes-scripture, "the words read twice"). **Human** = curated
  scholarship (lexicons, the 1769 renderings, TSK). **Machine** = a
  learned/aligned artifact (the LXX alignment and the rest of the R&D layer),
  and the default for an unrecognized source so nothing over-claims.
- `source_tiers(src) -> &[Tier]` — a *set* (a source can carry two): `quotation`
  → `[God, Machine]`; `etymology`/`rendering`/`abbott-smith`/`stepbible-*`/`tsk`
  → `[Human]`; `lxx`/`embedding`/`text-witness` → `[Machine]`; unknown →
  `[Machine]`. `research_grade(src)` = `quotation | text-witness` (method not
  yet held-out-graded). `tiers_of(&[src])` = deduped union, ordered God→Human→
  Machine (additive — never one "winning" tier). `source_label` = the lay label.
- **Marks** (glyph + color): God `✝` gold `#9e7d38`; Human `†` green `#6f8f6a`;
  Machine `≈` gray `#999`; research-grade `⚗` red `#b04a3a`. They are text
  glyphs, not images, and the **producer bakes them into the block runs** with
  the semantic colour role `tierGod`/`tierHuman`/`tierMachine`/`tierResearch` — so
  a shell shows them by painting runs, which is why both shells have them without
  either one owning tier code. The colour roles resolve through the palette
  (`Palette.kt` `colorOf` on Android, the role map in `BlockList.svelte`).
  **Per-chip** on SAME ROOT partners (real per-source provenance);
  **per-section** on the headers above; a **legend** at the foot. Human-baseline
  blocks (the dictionary entry) and user data (weaves, tags) are unmarked.
- **Wire**: `plumbline_engine_bridge_partners_json` carries additive `tiers`
  (`["god","human","machine"]`) + `researchGrade` per partner, for a shell that
  reads partners directly rather than as blocks. Fixed-by-block
  sections (APPEARS ALONGSIDE = Machine, TSK = Human, …) are marked by the
  producer too, not shell-side.

## Link routing — one verb vocabulary (P1.4)

All panel interactivity funnels through one URI dispatcher, and the verb
vocabulary is **parsed once in the core**: `plumbline_core::panel::parse_link(uri) ->
PanelLink` — co-located with the producers that *emit* the URIs, so a verb can't
drift between what the panel bakes and what a shell handles. Both shells route
through `plumbline_route_link_json(uri)` (`{verb, …}`, tagged) — neither
re-splits the string. The 22 verbs, all of them in `PanelLink`:
`go:Book:ch[:v]` · `occ:CODE` · `rend:CODE:rendering` · `code:CODE[:word]` ·
`thread:i` · `tag:i` · `weave:i` · `addtag:refkey` ·
`addthread:refkey` · `untag:i:refkey` · `makeweave:i` · `approve:i` · `reject:i` ·
`deletethread:i` · `deletetag:i` · `deleteweave:i` ·
`editthreadnotes:i` · `editweavenotes:i` · `editentrynote:ti:ei` ·
`editnote:refkey` · `guide` · `about`. An unknown verb or a malformed payload
parses to `None` and the shell ignores the click.

The three `delete*:i` verbs carry the **library ordinal** (`deleteweave:` the
flat `weave:i` ordinal, not `reject:`'s suggested one), are emitted on the
detail cards (`thread_detail` / `tag_detail` / `compare_card`; the compare card
omits delete on a suggestion — the review queue's reject is that act), and are
destructive: both shells confirm before authoring, then return to the item's
list, because ordinals shift after every write.

`conceptmap:CODE` was a twentieth verb; it left the vocabulary with
the popup it opened (see §Concept map popup).

Navigation + native prompts + the write choreography (author endpoint →
reload → refetch) stay shell-side. `parse_link` handles multi-word books
("1 John") and colon-bearing refkeys; `code:CODE[:word]` keeps its `word`
(a surface token) and opens the standalone code-study card — distinct from
`occ:CODE` (verse list) and `rend:CODE:rendering` (filtered list).

## Concordance (`occ:`; M:3783)

Code + lemma large + count; verse links capped at 300, "… N more".
*Data*: `plumbline_engine_strongs_occurrences_json` (cap 500 engine-side).

## Thread/tag pickers + delete (both shells)

`Add to thread…` and `Tag verse…` share one picker idiom: what exists is a list
you tap (with entry/member counts), freetext only for a genuinely new one, and
`✕` deletes the thread/tag and everything on it (the verses are untouched). A
bare freetext prompt would make the common case — adding a passage to a thread
built up over a week — retype the name exactly, and a typo would silently FORK
a second thread instead of failing.

Core: `thread::remove_thread` / `tag::remove_tag` (case-insensitive, like their
add twins; an absent name is a no-op, not an error). **C ABI**:
`plumbline_engine_thread_remove` / `plumbline_engine_tag_delete` (`tag_delete`,
because `tag_remove` was already taken by the member-level untag). Shells:
`ThreadPickerSheet` + `TagPickerSheet` (`ui/VerseActions.kt`) /
`ThreadPicker.svelte` + `TagPicker.svelte`. The same deletes are reachable from
the library detail cards via the `deletethread:i` / `deletetag:i` verbs, and
weaves — which have no picker — delete from the compare card via
`deleteweave:i` → `plumbline_engine_weave_delete(index)` (flat-library ordinal;
`weave::reject_weave` does the file removal in core).

## Tag categories — headings for the tag lists (core + web; Android pending)

A tag carries an optional **`category`** (overlay-tag-v1, additive: absent key
when none; trimmed, empty clears) — "tags need categories otherwise it'll be
soooo long" (maintainer UAT, 2026-08-18). Assigned on the MANAGEMENT screen
only — never mid-reading. The flow is the PICKER IDIOM (`TagsScreen`
"Categorize a tag" card → pick the tag → then existing categories are a
list you tap, with "New category…" (`tags.categoryNew`) opening the freetext
prompt and "No category" (`tags.uncategorized`) clearing; with no categories
yet it goes straight to the prompt, which is where the first one is ADDED).
Retyping a heading per tag was the typo-forks-a-second-category trap the
thread/tag pickers were built against (e2e:
`tag-categories.spec.ts` "an existing category is picked, not retyped"). Core: `tag::set_tag_category` (case-insensitive lookup
like rename; same-value set writes nothing, so `updated` only moves for real
changes). **C ABI**: `plumbline_engine_tag_set_category(name, category)`. Wire:
`WireTag.category` (camelCase, additive).

Where it SHOWS: every tag list groups under category headings **the moment any
tag has one, and stays dead flat until then** — a reader who never files
anything sees no change. Grouped surfaces: the tag PICKER
(`TagPicker.svelte`, `.ghead` rows) and the LIBRARY panel (`panel::tags_list`,
LABEL runs — `tag:{i}` links keep the tag's index in `tags()` order, not its
display position, so grouping cannot re-aim a tap). Categories sort
alphabetically; the uncategorized bring up the rear under `tags.uncategorized`
("No category"), which only appears once a real heading exists. E2E:
`tag-categories.spec.ts`; core: `a_category_is_set_trimmed_cleared_and_survives_the_file`.

**SHELL DELTA — Android:** the engine already groups the library panel (blocks
are core-built) once the `.so` is rebuilt, and the format/ABI are in place; the
Compose `TagPickerSheet` grouping and a management surface for assigning are
pending the APK catch-up batch.

## Ask before destroying anything (both shells)

One confirmation per shell, and whether an action asks is a property of the
**action**, not of whoever wrote its button. `ui/Confirm.kt` (`ConfirmRequest` + `ConfirmDialog`) and
`shell/ConfirmDialog.svelte` (behind `session.askConfirm(title, body, verb)`,
which returns a promise). The confirm button **names the act** — "Delete thread",
"Remove card", "Reject" — never "OK", so a reader who half-read the sentence still
knows what it does; that button is the tinted one (`tierResearch`, the app's one
alarm colour).

Behind it, per shell — grepped, because this is exactly the kind of list that
rots:

| destructive act | web | Android |
|---|---|---|
| delete a thread | asks (`ThreadPicker.svelte`, `study/links.ts`) | asks (`VerseActions.kt`, `StudyScreen.kt`) |
| delete a tag | asks (`TagPicker.svelte`, `study/links.ts`) | asks (`VerseActions.kt`, `StudyScreen.kt`) |
| delete a weave | asks (`study/links.ts`) | asks (`StudyScreen.kt`) |
| reject a suggested weave | asks (`study/links.ts`) | asks (`StudyScreen.kt`) |
| untag a verse | asks (`study/links.ts`) | asks (`StudyScreen.kt`) |
| delete a personal note | asks — both doors: the notes browser's ✕ (`StudyPanel.svelte` → `deleteNote`, `study/links.ts`) and SAVING AN EMPTIED EDITOR (`editNote` there), which is the same act spelled the old way (`usernote.rs`: empty text removes the file) | asks — the browser's ✕ and its editor (`Notes.kt`), the verse sheet's editor (`VerseActions.kt`), and the study panel's `editNote` prompt (`StudyScreen.kt`); every emptied save confirms |
| remove a memorization card | asks (`MemorizeHost.svelte`) | **no remove affordance at all**; `MemoryRemove` is an uncalled wrapper |
| clear a chapter's reading record | asks (`MarkReadDialog.svelte`) | **does not ask** — `onClear` calls `ReadingForget` and toasts |

The last two are live Android gaps, not design.

## Threads / Tags browsers (M:3380–3471)

List → detail. Threads list: "Threads (N)", each name + "N passage(s)".
Thread detail: name, notes small + `✎ notes`; per entry: verse link, snapshot
`text.join(" ")` truncated 70 + "…", note `#888` + `✎ note`. Tags list ↔
detail analogous; members: verse → go link, concept → concordance link; note
trailing. Authoring: `＋ tag verse` prompts a name (find-or-create,
case-insensitive) → `tag_add(name, "verse", refkey, null, now)`;
`＋ add to thread` snapshots the whole verse (span 0..last token, words
vector) → `thread_add`; `untag` → `tag_remove`. Both detail cards carry a
faded `✕ delete thread` / `✕ delete tag` header link (`deletethread:i` /
`deletetag:i` — confirmed, then back to the list; §Link routing). Note edits:
`thread_set_notes` / `thread_entry_set_note` / `weave_set_notes` via a
pre-filled text prompt (empty submission clears). *Data*: `threads_json`,
`tags_json` + the above.

**Thread entries REORDER, two ways.** The ↑/↓ links on each entry row
(`moveentry:T:E:±1` → `thread_entry_move`; core `move_in_thread`) are the
touch/assistive path in both shells. The web ALSO drags: the entry's header
row carries `drag: "{thread}:{entry}"` on the block wire (additive Para field,
absent everywhere else; core `panel::thread_detail` sets it), `BlockList.svelte`
renders a `⠿` grip (pointer-based, document-level listeners — grip-scoped
pointer capture loses the pointer when the row re-renders), and a drop calls
the same `thread_entry_move` write (`links.ts dragEntry`). Duplicates are
LEGAL: nothing dedupes `thread_add` at any layer, and Present survives a verse
appearing twice (thread-editing.spec.ts). **SHELL DELTA — Android:** decodes
`drag` (Wire.kt, unused); drag-reorder is owed with the APK catch-up batch,
alongside the `moveentry:`/`removeentry:` verbs that are inert there today
(§"Threads are EDITED"). E2E: `thread-editing.spec.ts` "verses in a thread can
be dragged into a new order".

## Suggested-weave review (M:2631, 3477)

Filter library `suggested == true`. Per weave: name bold + kind label gray,
notes, links ≤40 as "a ↔ b" go-links; actions `⇔ compare` `✓ approve`
`✕ reject` `✎ note`. Approve merges into `weaves/` (all links approved);
reject deletes the file and **asks first in both shells** — it is deleted, not
hidden, and does not come back for review. Ordinals shift after every write — always re-fetch.
*Data*: `suggested_weaves_json`, `weave_approve/reject(index)`.

**Delta — how the 194 suggestions reach each shell.** Android bundles them in the
APK and seeds them like the rest of the stock set (its asset copy recurses, so
`weaves/suggested/` is there). On the web they are a **download** — 422 KB raw of
machine-proposed links is not something to put on the boot path of a phone that
may never open the weave library — offered by a Settings row and fetched only when
asked. Mechanically: one bundled JSON object (110 KB gz; individually gzipped the
194 files are 784 KB, since small files compress badly) on a fourth pack stage,
`optional`, found by `role: "suggestedWeaves"` so a rename cannot unhook it.
`optional` is the first stage nothing fetches on its own, which makes two things
load-bearing and both are pinned by e2e: the update sweep must reconcile only the
files *this* device's pack consists of (else a deploy pushes the bundle onto
everyone and then jams the pin, `network.spec.ts`), and an install must not
overwrite a file the reader already has at one of those paths
(`suggested-download.spec.ts`). The pack builder's directory walk must recurse, or
the web silently ships without them and the two shells disagree about what the
stock set contains.

## Weave authoring (M:2137–2236)

**Weave links are authored via the tag→weave sheet** (`makeweave:` — tag passages
as a topic, then chain them), in both shells; a single left-click opens word
study, matching the Compose tap. `plumbline_engine_weave_add_link_spans` remains
in the ABI for span-precise links; no shell surfaces it today. **Compare card** (`weave:i`): name + kind +
"(suggested)"; "N link(s)" + (F) `✎ note`; per link ≤40: label `"…"` gold,
each side verse link + verse text small with **span words bold** and added
words italic gray. *Data*: `plumbline_engine_weave_add_link_spans`, `weaves_json`,
`verse_json` (tokens for span rendering).

**Opening a weave pulls its passages up (both shells).** The
`weave:` verb, besides loading the compare card, navigates the reader to the
weave's first resolved link (else its first link) so nobody hunts the text
down through the card: **web** — endpoint `a` in the active pane, endpoint
`b` in the next pane (created when only one is open; skipped when both
endpoints share a book+chapter), each with the verse band + scroll target;
**Android** — the reader goes to `a` (`goToRef`, behind the card sheet on a
phone) and the fold's second pane is pointed at `b`, so flipping it back
from Study lands on the other side. *Data*: `weaves_json` link endpoints
(frozen refKey form).

## Canon strip (M:2938–2989) — WEB ONLY

**Delta.** `CanonStrip.svelte` (mounted under the pane row in
`Shell.svelte`) is the only canon strip in the tree. **Android has none** — the
whole-canon overview a reader reaches for there is the passage navigator
(`ui/BookNav.kt`: OT/NT → book → chapter tap grids, tinted by the reading map),
which is a destination rather than persistent chrome, because 30 px of always-on
strip is a poor trade against a phone's vertical space. Android *does* consume
`canonSegments` — for the map popups (`ui/Maps.kt`) and the memorize coverage
band (`ui/Memorize.kt`) — so the endpoint is live there, just not as a strip.

The web strip: 30 px under the panes. 8 sections (Law 0–4, History 5–16, Wisdom
17–21, Prophets 22–38, Gospels 39–42, Acts 43, Letters 44–64, Revelation 65), odd
sections shaded black α0.04, centred 11-px labels when they fit; OT/NT divide
line at index 39. Pin per pane at `x=(order+0.5)/66·w` (active gold, others
faded). Click: `idx = x/w·66` → active pane to that book ch 1.

The segments + divide are the **single source** `core::reference::CANON_SEGMENTS`
/ `OT_NT_DIVIDE`, served over the wire by `plumbline_engine_canon_segments_json`.
Neither shell hardcodes the bands; both fetch the endpoint once and share the
answer between the strip/navigator and the map popups.

## Addressable chapters — WEB ONLY, by nature

Pane 0 mirrors into `location.hash` as `#/John/3`, so a chapter can be
bookmarked, shared or reloaded onto itself. `replaceState` on an ordinary
chapter turn, `pushState` only when a transient surface opens — that
distinction is the feature, not an implementation detail: without it a reader
flicking through Psalms needs forty Back presses to leave, and Back steps them
back through their own reading instead of closing the sheet in front of them.
Back peels **one layer per press** (`Session.popOneLayer` — the same ladder
Escape climbs, each screen up to the parent its own ‹ names), re-arming the
surface entry while layers remain; it used to dismiss the whole stack in one
press, which was the web's back-button complaint and a parity break with
Android's per-surface `BackHandler`s.

Precedence, all asserted in `e2e/routing.spec.ts`: an incoming address beats the
position the reader left; a `?at=` share link beats a stale hash in the same
URL; a malformed address falls through to the normal restore path rather than to
a blank reader — "an address the app cannot read" is what a shared link becomes
after a rename, and a blank first screen is the worst possible answer to it.

**Delta — and it runs both ways.** Android has no URL surface to mirror into, so
there is nothing to port: the popstate handler's twin is the `BackHandler` that
Android has always had, and this change closed that delta from the web side
rather than opening a new one. What is still open is the reverse: `AndroidManifest.xml`
declares only a `MAIN`/`LAUNCHER` intent filter, so a `plumblinebible.org` link
**opens the PWA in a browser even on a phone with the APK installed**. Closing it
means an `android:autoVerify` App Links filter plus the hosted
`.well-known/assetlinks.json`, which pins the app's signing certificate — that
ties a shell delta to the release keystore, so it is a deliberate decision
rather than an oversight.

## Search — a DESTINATION (M:660, 3739)

The ⌕ in the top bar opens a SCREEN, on both shells: its own field, a row of
scope chips, and results in the reader's own column. It was a field in the web
header answering into the 380px study sidebar (a bottom sheet over the text on
a phone) until 2026-08-17; Android's was already fullscreen, so there the
change is the scope.

`goto` answer → big "go to" link (navigates active pane; verse target gets the
band). `hits` → "N result(s)" + tier phrase small; per hit: verse link, gray
`why`, "※ note" marker for margin-note matches; "… N more" past cap.
*Data*: `search_scoped_json` / `search_blocks_scoped_json`.

**Scope chips** (`core::search::SearchScope`) — Everywhere · this book · this
chapter · Old Testament · New Testament · **Range…**, as the tokens `all` |
`book:<osis>` | `chapter:<osis>:<ch>` | `ot` | `nt` |
`span:<osis>:<ch>:<osis>:<ch>`.

**A RANGE, and the canon's own sections** (web, 2026-08-17). The chips answer
"where I already am"; a range answers a question the reader arrived with — the
Sermon on the Mount, Paul on the law. `Range…` opens a PANEL under the chips
(not a dialog: the query and its results stay on screen while they are being
narrowed) holding one-tap presets and a From/To pair of book+chapter dropdowns.
Choosing either closes the panel and leaves a chip naming what was searched
("Genesis 1–3", "Matthew 1 – John 21"), so the count on screen is always
explained.

The presets ARE `reference::CANON_SEGMENTS` — the same eight rows the canon
strip paints — resolved to spans by the shell, so a preset can never name a
stretch the strip draws differently and there is no second list of groupings.
A span is one CONTIGUOUS run, which is what keeps the engine filter a single
range test; the selections readers actually want (a book, a section, a stretch
of a Gospel) are contiguous in canon order. Reversed ends are NORMALIZED rather
than refused — a reader who fills the far end in first has not asked for
nothing — and a chapter past the end of its book clamps to that book's end,
which is how "to the end of Revelation" and a preset built against a corpus
with different chapter counts both arrive.

**Chapter scopes resolve from the DIRECTORY** (`Corpus::chapter_range`), not by
looking up verse 1: a corpus slice can open a chapter at verse 16, and the old
lookup emptied the scope when it did. Same fix, and same reason, as
`Corpus::book_range`. Every scope is a CONTIGUOUS run of
canonical verse indices (`Corpus::book_range`, answered from the chapter
directory without decoding a verse), checked at the mouth of `Rows::push` so
every tier filters in one place and the total counts only what the scope
covers. Rules that are tests, not conventions:

- a REFERENCE query ignores the scope ("John 3" is navigation, not filtering);
- an unresolvable scope matches NOTHING, never everything;
- an unparseable scope TOKEN searches everything, never nothing — a shell
  ahead of the engine still gets an answer;
- multi-word queries narrow BEFORE the phrase confirmation, or a phrase
  outside the scope wins the tier and silences the every-word hits inside it.

The two narrow chips carry the CONCRETE book/chapter they were built from, so a
result list keeps meaning what it meant when it was drawn. The scope resets to
Everywhere each time the screen is entered (web); Android's chip re-runs the
query already typed rather than waiting for another Search press.

**Delta:** the web searches per keystroke (180 ms trailing debounce); Android
searches on the IME Search action, once.

## Keyboard + wheel (M:1806–1875) — WEB ONLY

Up/Down ±line (`font·3` px); PageUp / PageDown|Space ±85% page (**Shift** =
all panes lockstep); Home/End; Right|`]` / Left|`[` next/prev chapter (this
pane); Alt+←/→ and mouse buttons 4/5 step that pane's back/forward history;
Ctrl+0 zoom reset, Ctrl+± zoom ±1 pt; `?`/F1 the shortcuts overlay; Esc hides
panel / closes popups. Wheel scrolls hovered pane; Ctrl+wheel zooms; Shift+wheel
scrolls all panes. Zoom **persists config on every change**.

**Delta:** Android's equivalents are gestures, not keys — vertical scroll,
horizontal swipe for the chapter step, pinch on the maps — and system Back closes
whatever is layered on top rather than stepping a per-pane history. There is no
per-pane back/forward stack on Android and no shortcuts sheet (nothing to list).

## Config / session (`core::config`)

`<config_dir>/plumbline/config.json`, where `config_dir` is `%APPDATA%` on
Windows, `~/Library/Application Support` on macOS, else `$XDG_CONFIG_HOME` (falling
back to `~/.config`). Both shipping shells take the XDG branch and point it at
their own storage: **Android** `Os.setenv("XDG_CONFIG_HOME", filesDir)` in
`MainActivity.onCreate`, before any `plumbline_config_*` call (without it every
launch loaded defaults); **web** `XDG_CONFIG_HOME=/home/.config` in the WASI env
(`engine/engine.ts`), which is why the backup zip carries
`.config/plumbline/config.json`.

`{"studyMode":"simple"|"full","bodySize":20.0,"openPanes":[{"book","chapter","verse"}],
"activePane":0,"versePerLine":false,"theme":"system","copyStyle":"verseRef",
"sideMargin":28.0,"lineSpacing":1.35,"humanAnalysis":false,"machineAnalysis":false,
"history":[{"book":"John","chapter":3}]}`.
All additive (default on absence); a save must round-trip fields it doesn't
expose (each shell carries them forward). `copyStyle`
(`verse`|`verseRef`|`verseMarkdown`) is the one-tap copy shape; `sideMargin`
(px, 0–160) + `lineSpacing` (×text-height, 1–3) are reader spacing — the sliders
in both shells offer the narrower 8–96 and 1.0–2.2. `history`
is recent (book, chapter), most-recent-first, deduped, core-capped at 50
(`config::HISTORY_CAP`) — powers "start where I left off" + a history list.
`first_run` only when the file is absent; a damaged file is renamed
`config.json.bad` and the session opens on defaults, no re-prompt. Restore panes
(≤3 on the web; default John 3) + active + zoom at startup; persist on close,
first-run pick, every zoom, and on the way to the background (Android `ON_PAUSE`,
web `visibilitychange`). Scroll position is NOT transient anymore — `openPanes`
entries carry `verse`. *Data*: `plumbline_config_load_json` /
`plumbline_config_save_json`.

## First run — who is opening the Book? (both shells)

The first launch asks who's here (`FirstRun.svelte` / `FirstRun.kt` — keep
the copy in sync). **Four paths, in this order: Curious about the
Bible sits ABOVE New in the faith** — a stranger to the Bible is the likelier
first-time reader, and the path that asks least of someone should be seen first.

- **Curious about the Bible** — for someone unsure what they believe: what this
  book is, that it is loved and died for, an invitation to start in John and to
  pray. Quotes John 3:16, Prov 2:4–5, Mark 9:24 ("Lord, I believe; help thou mine
  unbelief"), Matt 7:7 + Jer 29:13, Ps 34:18. Ends in John 1, same as the
  welcome.
- **New in the faith** — a welcome from the maintainer (next steps: read —
  Ps 12:6–7; find a church — Heb 10:24–25; memorize — Ps 119:11; assurance —
  Rom 5:8, John 3:16, 1 John 5:13, John 10:28–29, Phil 1:6, 1 John 1:9,
  2 Tim 3:16–17). **The verses are QUOTED inline** (the new believer reads
  scripture itself, not a row of links); every reference is
  tappable and opens **beside John**: web — second pane; fold —
  second pane; phone — the passage opens with John 1 as the saved start.
  "Open the book of John" lands in John 1 with **both analysis tiers off** —
  just the text.
- **Sharing the gospel** — straight into Present with the Romans Road
  (default tiers; the picker shows if the stock thread was removed). Asks for the
  church first, since this is the path that hands the app to someone.
- **Established believer** — the church fields, then the analysis-tier picker
  (scholars' / machine, with examples), both boxes **unchecked** (the tiers are
  opt-in). The text is always on; tiers change any time in Settings.
  Dismissing without choosing (click-away / system back) keeps the defaults.

**The curious and new paths also ask how the words should read**
(2026-08-16, both shells): the classic/modernized wording choice
(§Settings › Wording — same strings, same `akjvOverlay` key) sits above the
Open button, classic preselected, applied on the spot, with a note that
Settings can change it later. These are the readers likeliest to be stopped by
"shouldest", and the choice is theirs to make before the first chapter. Shown
only when the engine reports the overlay available, and never on a re-read (a
re-read moves no settings).

A link shared from Present offers only the first two paths — it was handed to
someone in person, and the rest is setup for a reader who already has a Bible
habit. Either welcome is re-readable later (the `intro` config field remembers
which one a reader was given): the **Welcome** entry in the ≡/⋮ utilities —
its ONE home in both shells, shown for every reader (no recorded intro falls
back to the new-believer welcome).

**Delta — where the quoted verse text comes from.** The two shells differ, on
purpose: **Android** asks the engine
(`FirstRun.kt`, `ALL_QUOTED` → `VerseJson` per refKey, off-thread into a `bodies`
map). **The web writes the text out in the source** (`FirstRun.svelte`'s `REF`
table, 15 entries) — asking for ten verses one at a time makes the quotes pop in a
beat after the page, and this is the first screen a new believer sees. The 1769 text is frozen, so a copy cannot drift; each
entry was taken verbatim from `data/kjv.jsonl` as `Verse::body()` renders it. If
you add a quote to the web welcome you are adding *text*, not a reference.

The old Simple/Full first-run modal is gone; `studyMode` still round-trips in
the config for readers of an older file.

## Primary menu (≡) and the role bar

**The bar carries ROLES, the tools live one layer down.** Both shells' bar is
**Read · Study · Preach · Share · Sing** — the hats a reader wears, not a
feature list. Study opens the hub (internally still the Explore screen); Preach
opens its own hub (below); Share is its own destination (below); Sing is the
hymnal.
Memorize is a CARD inside Study — a study discipline, not a role — and its
screen lights the Study tab in both shells. On narrow screens the web draws the
same **bottom nav bar Android has**, using the very same Material paths
(`ui/NavIcons.kt` → the `NAV` table in `Shell.svelte`: book · school ·
present_to_all · share · music_note), with gold on the current tab and
Compose's α0.14 gold pill behind its icon; above 700px the web's header carries
the four non-Read roles as text buttons instead. Read is not a destination so
much as the absence of one: the reader is always mounted underneath, so its tap
clears whatever is layered over it. **Delta:** Android's destinations are
mutually exclusive because it shows one screen at a time; the web layers, and
on a desktop the study panel is a sidebar. The highlighted tab always names the
surface actually in front of the reader. The subtitle is just the passage
("John 3" — no edition suffix; the e2e boot signal matches `/\w+ \d+/`).

**The ≡/⋮ utilities — reachable from EVERY destination.** Welcome · History ·
Guide & about · (web only) Keyboard shortcuts · Settings. The web renders ONE
fixed-position menu (`s.menuOpen`, session state) raised from the header's ≡ or
any destination ScreenBar's ≡; Android's `UtilityMenu` composable is the same
list, on the Read top bar and every destination's `ScreenBar` `actions` slot.
Before this, Settings from any non-Read destination cost a trip back through
the Read tab in BOTH shells. Welcome shows for every reader (falls back to the
new-believer welcome); Church is NOT a menu item — it rides the Share screen.

**Preach, the hub** (`PreachScreen.svelte` / `PreachScreen` in
`ui/StudyScreen.kt`): the presentation and the materials it is built from
(maintainer direction, 2026-08-11) — Present as the headline card (raising the
same fullscreen Present overlay the tab used to raise directly; it still lights
the Preach tab), then Weaves · Tags · Notes. The cards reuse the Study hub's
`explore.*` keys plus `preach.present.desc`; the same tools appearing in both
hubs is deliberate — the bar carries roles, and one tool can serve two hats.

**Share, the destination** (`ShareScreen.svelte` / `ui/ShareScreen.kt`): the
"Scan for the app" QR + the link via system share / copy (the matrix is
**encoded at render time** in both shells — `QrCode.svelte` over
qrcode-generator, `QrShare.kt` over zxing-core, both forcing UTF-8 byte mode —
because the link carries the reader's church and there is no one fixed URL to
bake in; the same QR closes Present's end card), plus **Your church**: the
three fields that used to sit at the bottom of Settings, edited beside the QR
their setting feeds, with a visit button when a URL is set (the recipient's
path to the congregation a shared link named). The old header Share icon and
`ShareAppDialog` are gone in both shells. Share is the app AND the Gospel
(maintainer direction, 2026-08-11): a **Share the Gospel** card
(`share.gospel*` keys) opens Present straight onto the Romans Road — the
first-run "sharing the gospel" landing, reachable every day after.

**Settings splits everyday from Advanced.** Everyday, visible: language (a
DROPDOWN since 2026-08-16, like the theme — a radio column grew a row per
language), the **Wording** choice (below), theme, the two type faces, the three
reader sliders (+ web-only verse-per-line), and a **Default style** button
(`settings.defaultStyle`) that puts size/spacing/margins, both faces and the
theme back to core::config's defaults (18 / 28 / 1.35 / eb-garamond ×2 /
system) — style only, never the reading aids or the reader's data. Everything
else — the analysis tier gates, copy format, bundled set, present-as-new, (web)
suggested pack / offline download / report, and Backup/Restore — sits behind
ONE collapsed **Advanced** disclosure (`<details class="advanced">` on web, an
expandable row in `SettingsDialog`/`StudyScreen.kt`). Church and Welcome left
Settings entirely (Share screen and ≡ respectively).

**Wording is an everyday CHOICE, not an Advanced switch** (2026-08-16; it began
as "Modernized wording" inside Advanced). Two radio options under
`settings.wording`, each carrying its cost in its description: **Classic**
(`wordingClassic` — the KJV as printed, classic and beautiful, potentially
unfamiliar words) vs **Modernized** (`wordingModern` — plainer, less elegant;
changes marked with a dotted underline, tap to see the KJV word). It drives the
same `akjvOverlay` config key and stays hidden unless the engine reports the
overlay available, so the German/Spanish corpora never see it. Still a reading
aid over the SAME text — memorize/Present/copy/share stay the KJV's words.

**The hub never redraws empty** (web). Its band reads and card counts go
through `session.qStale` — stale-while-revalidate: `invalidate()` runs on every
authoring write and dwell tick, and `q`'s null-while-refetching made the hub
pop back one answer at a time, the grid shifting under the reader's thumb
("widgets are spazzy on load", UAT 2026-08-18; measured at CLS 0.17 vs ~0
after). `qStale` is OPT-IN for count/summary surfaces only — a stale list
whose ordinals aim taps must keep using `q`. The skeleton also ghosts the
reads line, so the first-ever settle doesn't grow by a row.

**The Study hub's contents** (both shells, a described card list so the tools
aren't cryptic): Reading plans (web only until Android's plans ship) ·
Memorize · Notes · Threads · **Tags** · Weaves · **Visualizations** (one card
holding Constellation · Weave map — two views of the same thing, not two more
tools; UAT 2026-08-12). That card is a **door, not a branch**: it opens a PAGE
one layer down (`shell/VizScreen.svelte`; on Android a second `MapOverlay`),
whose ‹ returns to the hub rather than to the reader — the relationship Plans
and Memorize already have with it. It expanded in place at first, with the maps
as indented sub-cards, and that tree was the odd one out in a shell where a
destination replaces what came before rather than unfolding inside it
(maintainer, 2026-08-13). Every
card's label from the same `explore.*` keys in both shells (the Android
weave-map card had drifted onto `map.chordMap`; fixed).

**Weaves is a DOOR too** (2026-08-19). The web hub used to spend two sibling
cards — Weaves and Suggested — on two views of the same library; one Weaves
card now opens a page (`shell/WeavesScreen.svelte`, screen id `"weaves"`,
wired into `popOneLayer`'s sub-page rung) holding **Browse weaves** and
**Review suggested weaves** (`weaves.browse*` / `weaves.review*` keys), each
raising the panel its hub card used to raise (`{kind:"weaves"}` /
`{kind:"suggested"}`). Review is disabled with the reason in its description
when the queue is empty, per the Tags-page rule. The hub band's "to review"
row still opens the queue directly. *Delta:* Android was already one door —
its `WeavesScreen` is a list with an All/Suggested filter rather than a page
of cards (e2e: `study-hub.spec.ts` "Weaves opens a page holding the library
and the review queue").

**Threads are EDITED, not just accumulated** (2026-08-18). A thread's detail
card carries per-entry **↑ ↓ · remove** beside its note link. The arrows are
omitted at the ends rather than shown disabled — no room on a phone for a
control that cannot act, and the list's shape already says which end you are on.
Removing ASKS FIRST and names the passage (`deletethread:`'s rule); rearranging
does not, because it is undoable by doing it again. `core::thread`'s
`move_in_thread` clamps a destination past the end to a no-op AND DOES NOT WRITE
— rewriting would move `updated` and make a no-op look like an edit — and
`remove_from_thread` leaves the THREAD standing when its last entry goes: an
empty thread is a heading someone is still filling in, and discarding its name,
notes and id is `remove_thread`'s job, asked for deliberately.
**SHELL DELTA:** the controls render on Android too (the blocks come from the
core), but its link dispatcher does not carry the `moveentry:`/`removeentry:`
verbs yet — the taps are INERT there (safely: `RouteLinkJson` parses are wrapped
in `runCatching`), pending the Android catch-up batch while the APK is on hold.

**A thread may hold the same verse twice, and Present must survive it.** Nothing
in the format or the authoring path forbids a repeat, and a road can legitimately
come back to a verse. `PresentHost` keyed its verse list by refKey, so a
duplicate threw Svelte's `each_key_duplicate` and killed the component
mid-render — which the maintainer met as "I added a couple of verses and it's
all smushed", a Present that would not open its thread. Keyed BY POSITION now;
the list is replaced wholesale on every change, so position is a sound identity.
The focused-verse view also gained `min-height: 0` + `overflow-y: auto`, for
`.overview`'s reason: a verse at 54px on a phone in landscape ran off the bottom
with no way to reach it. Its centring is AUTO MARGINS on the two children, not
`justify-content` — plain `center` pushes an overflowing verse's first line
above the top edge where scrolling cannot reach it, and `safe center` is a
keyword WebKit shipped late, where an unsupported keyword drops the declaration
and top-aligns every short verse on exactly the iPhones the PWA is the install
path for. Auto margins absorb the free space when the verse fits and resolve to
zero when it does not. Both failure shapes are mutation-tested
(`e2e/thread-editing.spec.ts`).

**WHICH THREAD SHARES THE GOSPEL is a setting** (`config.gospelThread`, web).
Share's one gospel button and the first-run path of the same name walk it.
EMPTY MEANS THE STOCK ROMANS ROAD rather than "none", and a name that no longer
matches any thread falls back to it too — deleting the thread you chose leaves
the button working rather than dead. One resolver (`Session.gospelThread`), so
Share and first-run cannot disagree. *Delta:* Android still opens the stock road
by name.

**Tags is a DOOR too** (2026-08-14), for the reason Visualizations is: there is
more than one thing to do with a tag library, and a card that raised the library
panel directly had nowhere to put the rest. The page holds Browse (what it always
did) · **Rename** · **Merge** — the two operations a tag collection accumulates a
need for, because names drift ("grace", "Grace", "God's grace") and end up
wanting to be one tag. An action with nothing to act on is DISABLED rather than
hidden, with the reason in its description: a menu whose items appear as you
acquire data is a menu you cannot learn.

`tag::rename_tag` KEEPS THE TAG'S IDENTITY — the file is `slug(name).json`, so a
rename is a write plus a delete, and [`Tag::id`] is carried across so the tag on
the other side is the same tag rather than a new one wearing the name (which is
what that field was added for). It refuses a blank name, and refuses a name
another tag already answers to: that is a MERGE, and merging is destructive
enough to have to be asked for by name. A change of case only is a legal rename
onto itself — the slug is unchanged, so the file is rewritten in place and the
`dest != lt.file` guard is what stops it deleting what it just wrote.

`tag::merge_tags` folds one tag into another and deletes the source. Members are
identity by `TagTarget`, so a verse in both stays one entry and the SURVIVOR's
copy wins — letting the source overwrite would quietly discard a note the reader
wrote on the tag they chose to keep. Merging a tag into itself is refused
(source and destination would be one file, written and then removed). Both
guards are mutation-tested; both shells ask before merging and name which side
survives.

**The hub carries STATE, not just a menu** (2026-08-13). It was eight identical
rectangles of fixed text, so it looked the same on install day as after a year
of study — "every time I click study it just doesn't excite me… not like the
other pages", which tell you today's chapters or hold actual hymns. Two
additions, both live:

- the band's reads are **WARMED IN THE BACKGROUND** (`Session.warmStudyHub`,
  2026-08-17), so arriving at the hub paints numbers rather than the ghost. The
  four band reads and the four card counts are fetched at boot idle and again
  after anything that empties the cache they live in (an authoring write, a
  dwell report, a boot stage) — coalesced, skipped while the hub is on screen,
  and walked SEQUENTIALLY because the engine is one thread and eight reads fired
  at once would sit in front of the reader's next tap. The placeholder below is
  still the answer for a cold arrival (a warm that has not run, or a read that
  failed), so it is a floor, not dead code. The day-keyed reads take their
  stamp from ONE `dayStamp()` — a warm computing that key differently from the
  screen it warms would miss every time and look merely slow.
  **SHELL DELTA:** Android still reads the band when the hub is composed
  (`LaunchedEffect(refreshEpoch)` in `StudyScreen.kt`, on `Dispatchers.Default`
  so the UI thread is free but the numbers still arrive after the reader). The
  same warm belongs there — hoisted to launch and re-taken on `refreshEpoch` —
  and is deferred with the rest of the Android work while the APK is on hold;
- the band draws a **placeholder of its own shape** while its four reads are in
  flight — `q` answers null while fetching, which renders identically to
  "nothing running", so the band drew empty and then GREW, shoving the card grid
  down a beat after it settled. The ghost is sized by MEASUREMENT, not by eye:
  two ghost rows made it worse in the other direction (the grid jumped 49px UP
  when the real band turned out shorter), and one row plus the coverage strip
  took the shift from 49px to 3. A read that never answers must not strand it,
  so after three seconds the band shows whatever it has;
- an **IN PROGRESS band** above the cards, drawing only rows that have
  something to say (a hub reading "0 due · 0 to review" every day is the fixed
  text again): ONE ROW PER RUNNING PLAN in order, each naming the chapters it
  still wants; the memorize queue when anything is due; the review queue when
  anything waits; and — when nothing is running at all — one invitation instead
  of an empty box. The plan rows go through `planToday.ts`'s `todayPlans`, the
  same shaping the nav-strip chip and the navigator's today card share, which
  is what keeps four rules right at once: a concept study is not a schedule and
  has no day (and is not a builtin, so its raw id would otherwise render as a
  name), a paused plan asks nothing, a finished one has dropped out, and
  `remaining` narrows each row to what is left rather than restating the whole
  day. Every plan running, not just the first — the band read `running[0]` when
  it shipped, so a reader with three schedules saw one and no sign of the
  others while the chip two screens away said "+2 more" (maintainer,
  2026-08-13). All plans done for the day says so, rather than falling through
  to the start-a-plan invitation;
- a **count on every card that holds a collection** (notes, threads, tags,
  weaves), absent at zero so an empty tool reads as quiet rather
  than as a score of nought. Plans and Memorize carry none: they are activities
  rather than collections, and the band already says what they want today.

The band also carries the **lifetime counter**: how many times this reader has
been through the whole Bible. Seeded ONCE by hand (a numeric prompt — `inputmode`
on the web, a `KeyboardType.Number` field on Android) because somebody arriving
with thirty years behind them should not start at nought, and EARNED after that:
nothing in the UI edits it, and the only thing that moves it is finishing the
canon. `bibleReads` is **-1 for "never said"**, deliberately not 0 — a reader who
answers "none" has told us something and must not be asked again. Crediting is
exactly once per finished canon: `bibleReadsCredited` marks the CURRENT complete
state as counted and is cleared if the map drops below full, so the number moves
on finishing rather than on every visit (the mutation that credits on every
observation reaches 1,002 and fails its test). Seeding sets the flag TRUE
whatever the canon says, so a reader who is already finished is not immediately
credited with the read they just declared; the band reconciles it.

*Not built:* nothing resets the reading map, so a second full pass needs the map
to drop below complete and return. A "start a new pass" action is the follow-up.

Closing the band is the reading map as **one number and one bar** — chapters
read of the canon's 1,189, painted in the map's own `readDone` over a faint
`readUnread` track, so it belongs to whichever of the eighteen themes is on and
tapping it opens the navigator. Chapters rather than a word-weighted percentage:
"412 of 1,189" is a number a reader can hold.

NO NEW ENGINE CALLS: every number is a query some other screen already makes
(`plans`, `memoryDue`, `suggestedWeaves`, `userNotes`, `threads`, `tags`,
`weaves`, `readingBooks`), so the web reads them through the session cache and
the counts move on an authoring write — a count fetched once looks perfect
until the reader writes something, which is what `e2e/study-hub.spec.ts` pins.
Android has no general study epoch, so it refetches on the note epoch and on
every entry to the hub. *Delta:* the band's plan row is web-only, following
Reading plans themselves.

**Where you were, PER SEATING** (`core::session_slot`, 2026-08-13). A reader's
last chapter is not one thing: somebody who studies on weekday mornings, sits in
a Sunday service and goes to a Wednesday meeting has three separate places they
were, and one "last chapter" serves whichever they did most recently — so
arriving at church reopened Saturday night's study. Four slots, and the
boundaries are a judgement stated in the core rather than buried in a shell:
`sunday-morning` (Sunday before noon) · `sunday-evening` (Sunday from noon) ·
`wednesday-evening` (Wednesday from 5pm) · `other`. Wednesday MORNING is
deliberately `other` — the slot is for the midweek meeting, and a Wednesday
morning is a weekday morning. The RULE is the core's
(`plumbline_session_slot`, engine-independent like the theme palette) and the
shells pass their own LOCAL date and hour, because a slot computed in UTC puts a
Sunday-evening service in Monday for half the world. `config.slots` is keyed by
token and additive; a seating never used falls through to the plain last
position, which is what every reader has today and has its own test. The web
restores fire-and-forget (the panes are built before the answer lands) guarded
by `#navigatedSinceBoot`, so a reader who taps in those few ms is not yanked
away; Android resolves it synchronously, being a string comparison.

**History reads as RUNS** — "Genesis 1–3", not three lines. Adjacency is in the
LIST, not merely similarity: `[Gen 3, John 1, Gen 2]` stays three lines, because
merging those two Genesis entries would claim the reader went 2→3 without
leaving, and the order they did things in is the entire content of a history. A
tap opens the run's MOST RECENT chapter. *Written twice on purpose* — the shells
own this list (`pushHistory` prepends locally and the config only reaches the
engine on a debounced save, so anything the core derived would be stale the
moment the reader turned a page), so the rules live in HistorySpansTest (7 cases)
and `e2e/history.spec.ts`.

## Languages (both shells)

Full detail in [I18N.md](I18N.md). The contract, in one place:

**Every word the reader sees is core data.** The catalogue lives in
`crates/core/src/i18n/*.json`, keyed by stable dotted ids, and a shell reaches
it with two engine-independent calls at startup —
`plumbline_i18n_catalog_json` for what the shell spells and
`plumbline_i18n_set_language` for what the core spells (book names, references,
the reading map). Both take the reader's setting AND the device's locale,
because `i18n::resolve` owns the rule that an empty setting means "follow the
device" and a rule implemented twice disagrees with itself once.

English and German ship. `config.language` holds the choice; empty is the
device's. `refKey` does not move under any language — `VRef::ref_key` is frozen
storage, `VRef::display` is what localizes ("Joh 3,16", with a comma).

Both shells are done: every string, a picker in Settings, and
`scripts/check-i18n.mjs` failing the build on a stray literal in either. The web
reloads to apply a language change and Android recreates its activity — same
reason, which is that the table of contents is built once when the engine opens.

**SHELL DELTA.** `settings.bundledReloads` is web-only: the web reloads to apply
the bundled study set and Android does not. It is the ONLY copy difference left
between the shells — extracting Android turned up eleven others that were pure
drift and are now one wording each.

**SHELL DELTA.** The welcome pages are English in both shells on purpose: they
are the maintainer's own first-person writing, and a machine draft of that is
not a translation. `ENGLISH_ONLY` in `i18n.rs` is the list.

**Provenance.** Every user-authored file (note, thread, tag, weave, memory card)
carries `lang`, stamped at CREATE and never on re-save. Nothing reads it: it is
what makes the versification migration runnable later. Absent means "unknown",
not English — see I18N.md.

**The language registry** (`crates/core/src/i18n.rs`). A language is ONE ROW:
code, endonym, exonym, catalogue, corpus (file + tokenization stamp + the name a
reader knows that Bible by), Strong's dictionary and whether its definitions are
machine-translated, an optional modernization of that language's standard
translation, an optional printed-numbering table. `corpus_for`, `strongs_for`,
`tokenization_is_ours`, the modernization gate, the printed-numbering
annotation, the study card's rendering label, the hydrator's file list, the web
pack's roles and the Android asset check all READ THE ROW. Adding a language is
a variant, a row, a catalogue and the data files it names. See I18N.md.

**The other Bibles** (both shells). Luther 1912 (`luther1912-tok1`) and
Reina-Valera 1909 (`rv1909-tok1`), both public domain, both AT THE KJV'S OWN
VERSE ADDRESSES — each source was already mapped to KJV numbering, so `refKey`
means one verse in all three and no migration exists. Each carries its own
Strong's tags, so word study is real study in all three. The language must be set
BEFORE the engine opens, because that is when the text is chosen.

Morphology and the AKJV modernization are withheld from a text they were not
built against: both are keyed by token index against `kjv1769-tok2`. The
modernization is a per-language COLUMN, not a special case — it is one English
feature, as Luther's verse numbering is one German one — and it is selected by
the OPEN corpus's tokenization, so a reader whose download has not landed still
gets it over the KJV they are actually reading.

Spanish needs no printed-numbering annotation at all (Reina-Valera keeps the
KJV's breaks); its dictionary ships the Reina-Valera renderings with Strong's own
English definitions until a translation run fills them in, and
`machine_translated: false` on its row is what stops the study card claiming
otherwise.

**PER-PANE TEXT LANGUAGE — WEB ONLY (2026-08-17).** A reading pane picks its own
Bible from a chip on its header (KJV · Luther · Reina-Valera) and the app's
language does not move: German beside English for John 3. Full study per pane —
a word tapped in the German column is studied in German, from that language's
own Strong's, and the concordance opened from that study lists German verses.
The reader's data is SHARED, not copied, because every text sits at the KJV's
verse addresses. `config.openPanes[].lang` persists it (additive; absent = the
reader's own text) — and the RESTORE reopens the engine it names
(`session.svelte.ts` calls `openPaneLang` per restored language, panes holding
their layout via `langLoading` until it lands; failure keeps the language and
shows the pane's error line). Without the reopen, a restored German pane's
first layout threw "not open on this device" and sat blank, its word study
dead — UAT 2026-08-18's "Strong's isn't working except for English". Pinned by
pane-language.spec.ts.

The engine is `plumbline_engine_open_lang(home, lang)` — a second engine on the
same home, which deliberately does NOT fall back to English, because a pane
labelled Deutsch painting the KJV is the failure the whole path exists to
avoid. Authoring stays on the primary handle; the alt handles re-read the study
files after a write. Design, the rejected alternative, the measured cost
(~61 MB of wasm heap per extra Bible; three at once takes the heap from 104 MB
to 226 MB) and the test traps: [docs/PER-PANE-LANGUAGE.md](PER-PANE-LANGUAGE.md).

**SHELL DELTA — Android does not have this yet.** Its language remains one
setting for the whole app, set BEFORE the engine opens. Every corpus is already
bundled in the APK, so the port has no download to arrange.

**SHELL DELTA — delivery.** Android BUNDLES every language's corpus and
dictionary in the APK (~2 MB each, marker `.data-v6`); the web fetches them on
demand as `stage: "optional"` under `corpus:<code>` / `lexicon:<code>` roles when
the reader picks that language, because nothing on the web is bundled and an
English reader must not download a Spanish Bible. Same split as the hymnal.

**SHELL DELTA — hymn texts.** 84 of 90 hymns carry English text and 31 carry
German; NONE carries Spanish, in both shells. The mechanism is language-keyed and
ready; what is missing is a public-domain source with verifiable attribution, and
hymn text is not something to reconstruct from memory.

## Hymnal (both shells)

The fifth destination. A book of public-domain hymns with chords, meant to be
sung from: the maintainer's brief was "text size presentable so people can share
a phone and sing together, automated scroll, and show and transpose the chords
so we can play it".

**The engine owns the music theory.** `core::hymnal` loads `data/hymnal.json`
(format tag `hymnal-v1`, frozen like every other on-disk format), parses the
ChordPro-style inline brackets — `A[G]mazing [C]grace` — into (chord?, text)
segments, and transposes. Two endpoints: `hymnal_json` (the index) and
`hymn_json(id, transpose)`, which hands back stanzas **already split and already
transposed**, so neither shell parses a bracket or knows that G+3 is Bb. Chords
are spelled for the key they LAND in (G+3 is Bb, never A#), and `transposedKey`
comes back so a transpose control can display it. An unparseable bracket stays in
the lyric as literal text rather than vanishing — the files are maintainer-
authored data and a swallowed typo is invisible to the one person who can fix it,
so `scripts/build-hymnal.mjs` validates every bracket against the same grammar at
build time, which is the only place that can catch one.

**One hymn, one entry, per-language texts.** A translation is a second text on
the same hymn (`texts: {"en": …, "de": …}`), not a second hymn, because the
language toggle is the seed of the multi-language release that follows this one
and a hymn split across entries would need stitching back together the day that
lands. Both texts share the tune, so they share one chart. The reader's language
is a *preference, not a promise*: a German-only hymn shows German to an English
reader rather than showing nothing.

**The finder matches number, title and first line** in any of a hymn's
languages, and a token that *names* a language narrows the book to hymns
carrying it — `de` (code), `German` (English name) or `Deutsch` (endonym),
case-insensitively, stackable with the text query (`de jesu`). The three labels
come from one source: `i18n::Lang::{code,exonym,endonym}` in the core, crossing
in the catalogue wire's `languages: [{code, endonym, name}]`, so a future
language is searchable by all three the day it is added with no shell change.

**Sing mode** reuses Present's sunlight palette (fixed light, near-black on
white, big type) for exactly Present's reason — a phone held up between people in
a lit room — but is its own surface, not wired into the Present thread flow. Its
auto-scroll is a continuous creep rather than a jump per line, since singing is
continuous and a page that steps makes everyone find their place again; speeds
1–9 with 0 meaning hold. Scaled by each frame's own elapsed time so a 120Hz
screen does not run at double speed, with fractional pixels carried between
frames (at the slowest speed a frame is a tenth of a pixel, and flooring that
every frame holds the page still forever). Chords are **off by default**: most
people singing are not playing.

**Transposition is per hymn**, stored with the id and reset on open — a singer
who dropped one hymn a tone has said nothing about the next.

*Deltas:* none of substance. Both shells draw the same five-item bar from the
same Material paths (`NavIconHymnal` ↔ the `NAV` table), both use the engine's
split, and sing mode is a fullscreen overlay hosted above the nav in both
(`HymnalSingOverlay` ↔ the `sing-host` block). The web additionally lists Hymnal
in the desktop top-bar nav, as it does for every destination.

*Data*: bundled in both shells — `data/hymnal.json` ships in the APK assets and
on the web's **study** pack stage. It is deliberately NOT on the web's eviction
list: the first read happens whenever the reader opens the hymn tab, which can be
an hour into a session. For the same reason the engine's cache of it is never
set from an empty parse (the `strongs` stance) — the file lands *after* open, and
a tab opened in that window would otherwise pin an empty book for the session.

## The two map popups — a shared note

Both (chord / constellation) are **core view-models the shell only
paints** — positions cross the wire as fractions and logical units, never pixels
or colours. The pixel sizes quoted below are the desktop-era popup dimensions and
survive as proportions, not as layout: the web frames them in `MapFrame.svelte`,
Android in a fullscreen `MapOverlay` over a pinch-zoom/pan canvas (`ui/Maps.kt`,
the shared `zoomable` clamping the offset so a map can't be flung off-screen).
Anything described below as a hover tooltip or a keyboard page step is web-only for
the usual reason. **Delta (theme):** Android's maps paint on `palette.panelBg`, so
they follow the reader's theme; the web's keep light popup paper in every theme
(`MapFrame.svelte`). One of the two is wrong and it is a product call, not a bug.

## Chord/arc "Map" popup (M:887–935, 2994–3087)

1000×360, Esc or the close button closes. **The book-pair fold lives in the core
view-model** `plumbline_engine_chord_map_json` → `{pairs:[{a,b,count}] (canon book
indices, a≤b), max, otNtDivide, bookCount}`. The shell only paints: canon axis
with section bands + labels (from the shared `canonSegments` fetch), gold
baseline, OT/NT seam; ribbons heaviest-first, alpha
`0.12+0.30·(cnt/max)`, foot width `2+8·(cnt/max)`; colours OT `(0.82,0.70,0.43)`
/ NT `(0.50,0.70,0.90)` / cross `(0.78,0.59,0.86)` (+0.08 α, cap 0.5); apex
`min(0.42·h, 22+0.26·h·|dx|/w)`; self-pair = small loop. Click: x→book →
navigate active pane + close. The map counts every deduped pair, resolved or not —
unlike the connector overlay, which draws only resolved ones. **A portrait
viewport flips the whole map** (both shells, UAT 2026-08-12): the canon axis
runs down the LEFT edge (top = Genesis), ribbons bulge right, labels read
spine-wise — landscape logic painted through one rotation transform, taps
mapped y→book; landscape gave a phone's 66 books a thumb's span of axis.

## Constellation popup (M:937–1529)

1200×640; ‹prev/next› + caption; Esc or the close button closes; Left/Right page.
**The whole layout is the core view-model** `plumbline_engine_constellation_json(page,
pins_json)` (pins = a JSON array of weave indices) → lanes of nodes + edges as
**fractions** (`x` a canon fraction, `laneFrac` 0..1 within a lane, `size` a
0..1 witness degree) plus `nPins/freeTotal/page/maxPage/caption/laneCapacity`.
Usable = weaves with ≥1
resolvable link, largest-first; `laneCapacity` (18) lanes, pinned (by weave
**index**) first. The shell maps fractions to pixels + paints: `laneH =
(h−topPad−10)/laneCapacity`, node `(plotLeft + x·(w−plotLeft), topPad +
(lane+laneFrac)·laneH)` with plotLeft 162 / topPad 18 / gutter 150; 7-colour
cycle ×0.72; node square half-size `1.4+2.4·size`; pin gutter x<150 (filled gold
8×8 pinned / hollow gray); lane name ≤22 chars; canon ruler + OT/NT seam; hover
tooltip "verse · weave". Hit priority **node > edge > pin-gutter**; node →
navigate (stays open); edge → compare card (closes); gutter → toggle pin. The
caption comes from the model.
Node size normalises by the **global** max degree, and the lane-height metric and
the caption come from the model, so no two shells can drift on them (the desktop
pair had, on all three).

## Concept map popup — REMOVED

The radial graph opened by "▸ open concept map" (`conceptmap:CODE`) is gone: a
Strong's code ringed by its embedding-near and community neighbours over a canon
dispersion strip, plus the cross-testament bridge row that rode inside the same
payload. Judged machine-generated noise.

It was a core view-model, so it went from both shells at once: the endpoint
`plumbline_engine_concept_map_json`, the `conceptmap:` link verb, and the
shells' painters. What stays: the **chord map** and the **constellation** (weave
visualisations, nothing to do with this), and the symbolic concept engine behind
APPEARS ALONGSIDE / MOST USED IN / LEITWORT, which is co-occurrence statistics
over the corpus and never read the embedding.

The embedding half of the spoke union (`nearest_concepts`) was one of the three
features reading `data/concept-vectors.vec`; with all three gone the artifact
left the data pack. See item 7 of §Word study panel.

## C ABI surface (crates/ffi) — endpoint ↔ feature map

**108 native fns**, plus 6 wasm-only shims in
`crates/ffi/src/wasm.rs` that cbindgen excludes by name. Don't trust a count in
prose — the guarantee is mechanical: `plumbline-bindgen`'s `verify_surface`
requires every `plumbline_*` symbol in `include/plumbline.h` to appear in
`bindings/kotlin/Plumbline.kt` and vice versa, and CI fails on drift. So "the
endpoint is in the Kotlin binding" is *automatic* and says nothing about whether
Android calls it; only a grep for the wrapper's call sites answers that.

Pre-existing: `open`/`open_from_bytes`/`free`, `toc_json`, `chapter_count`,
`verse_json`, `token_json`, `layout_chapter` + `layout_*` + `hit_test`,
`strongs_json`, `strongs_occurrences_json`, `search_json`, `threads_json`,
`tags_json`, `verse_xrefs_json`, `suggested_weaves_json`, authoring
(`thread_add`, `tag_add`, `tag_remove`, `weave_add_link`, `weave_approve`,
`weave_reject`, `thread_set_notes`, `thread_entry_set_note`,
`weave_set_notes`), R&D (`bridge_partners_json`, `morph_json`).
`similar_verses_json` and `concept_neighbours_json` were here too, before their
removal.

**`concept_neighbours_json` is REMOVED**: it outlived every shell caller (it
never had one) and the embedding it read — the pack stopped shipping
`data/concept-vectors.vec`, so it could only answer empty. It left the extern
surface, the header and both bindings in the same change set that took the
other embedding readers; no `plumbline_*` symbol or wrapper remains (a grep
for `concept_neighbours` finds only this paragraph).

Added for shell parity:

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_verse_notes_json(ref)` | `{verse, notes[]}` or null | margin notes |
| `plumbline_engine_study_xrefs_json(ref)` | `{verse, refs:[{to, toDisplay, end?, votes}]}` | TSK tier |
| `plumbline_engine_weaves_json()` | full library: weaves + links incl. `approved`, `spanA/B`, `resolved`, `suggested` | compare card, weaves list, panel xrefs (chord map + constellation now have their own view-model endpoints) |
| `plumbline_engine_concept_json(code)` | `{total, ot, nt, topBooks, byBook, collocates, community, leitwort?}` | ALONGSIDE / MOST USED IN / LEITWORT / dispersion |
| `plumbline_engine_gloss(code)` | plain english gloss or null | concept chips |
| `plumbline_engine_weave_add_link_spans(name, a, b, aLo, aHi, bLo, bHi, added)` | null/error (negative span = none) | word-span links |
| `plumbline_config_load_json()` / `plumbline_config_save_json(json)` | config wire above (+`firstRun` on load) | session/mode/zoom |

Added for the rendering lens:

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_renderings_json(code)` | `{code, renderings:[{rendering, total, capped, refs:[{verse, display, span:[s,e]}]}]}` (refs cap 500) | RENDERINGS tier + filtered concordance |
| `plumbline_engine_word_codes_json(word)` | `{word, codes:[{code, count}]}` | "also translates" reverse line |

Extended for authority tiers: `plumbline_engine_bridge_partners_json`
partners gained **additive** fields `tiers` (`["god"\|"human"\|"machine"]`,
deduped, ordered God→Human→Machine) and `researchGrade` (bool). Existing
`code`/`sources`/`prior` unchanged; a consumer that ignores the new fields sees
the pre-tier behaviour. No extern-surface change → bindings unchanged.

Added for the view-model consolidation (architecture-review P0.3 —
moves shared derivation out of the shells into the core):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_link_pairs_json()` | `{pairs:[{a, aBook, aChapter, aVerse, b, bBook, bChapter, bVerse, resolved}]}` | ambient connectors (web only — Android's wrapper is uncalled) |
| `plumbline_engine_canon_segments_json()` | `{segments:[{label, first, last}], otNtDivide}` | canon strip (web) / passage navigator + map ruler bands + memorize coverage (both) |

Both are thin wrappers over the one core source: `link_pairs` wraps
`plumbline_core::weave::link_pairs`; `canon_segments` wraps
`core::reference::CANON_SEGMENTS` / `OT_NT_DIVIDE`.

Added for the popup view-models (architecture-review P0.2 — the
map popups' derivation moved into the core; positions cross the wire as
**fractions/logical units**, never pixels/colours):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_chord_map_json()` | `{pairs:[{a, b, count}] (canon book indices, a≤b), max, otNtDivide, bookCount}` | chord/arc "Weave map" (retires the shell fold + max) |
| `plumbline_engine_constellation_json(page, pins_json)` | `{lanes:[{weaveIndex, name, pinned, nodes:[{x, laneFrac, size, refKey, book, chapter, verse, display}], edges:[{aX, aLaneFrac, bX, bLaneFrac}]}], nPins, freeTotal, page, maxPage, caption, laneCapacity}` (pins = JSON array of weave indices) | constellation (retires the usable/degree/jitter/paging/pin derivation) |

Added for the hymnal — the shells paint, the engine does the music:

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_hymnal_json()` | `{hymns:[{id, number, titles:{lang→title}, firstLines:{lang→line}, tune, meter}]}` (number order) | the hymnal index + its number/title/first-line finder |
| `plumbline_engine_hymn_json(id, transpose)` | `{id, number, tune, meter, key, transpose, transposedKey, texts:{lang→{title, author, translator, year, stanzas:[{lines:[{parts:[{chord, text}]}]}], chorus}}}` | one hymn, chords split and transposed (retires all ChordPro parsing and every shell-side key calculation) |

Producer: both wrap `plumbline_core::hymnal`. `firstLines` are chord-stripped so
the finder searches what a singer would actually type. An empty `hymns` means the
pack has no `data/hymnal.json` yet, never an error; an unknown id is null.

Producers: `chord_map` wraps `plumbline_core::weave::chord_pairs`; `constellation`
wraps `plumbline_core::weave::constellation`. Both shells consume the
JSON and map fractions → pixels; neither re-derives anything. A third row,
`plumbline_engine_concept_map_json(code)`, was removed with the concept map popup.

Added for the panel content-model + link router (P0.1 + P1.4 — the
whole study panel and its verb vocabulary move into the core). Every block
endpoint returns `{blocks:[Section|Para|Rule]}` (runs carry a semantic colour
role + logical size + optional uri); `full` gates the R&D tiers + author actions.

| endpoint | returns / for |
|---|---|
| `plumbline_engine_word_study_blocks_json(ref, tok, full)` | the tapped word's dictionary + Full tiers + this verse's xrefs/notes |
| `plumbline_engine_code_study_blocks_json(code, word?, full)` | the standalone `code:` card |
| `plumbline_engine_concordance_blocks_json(code)` / `plumbline_engine_rendering_concordance_blocks_json(code, rendering)` | full / rendering-filtered concordance |
| `plumbline_engine_threads_blocks_json()` / `plumbline_engine_thread_blocks_json(i)` | threads list / detail |
| `plumbline_engine_tags_blocks_json()` / `plumbline_engine_tag_blocks_json(i)` | tags list / detail |
| `plumbline_engine_weaves_blocks_json()` / `plumbline_engine_suggested_blocks_json()` | weaves list / review queue |
| `plumbline_engine_compare_blocks_json(i, full)` | weave compare card |
| `plumbline_engine_search_blocks_json(query)` | search results (goto link or ranked hits + snippets); null on blank |
| `plumbline_engine_search_blocks_scoped_json(query, scope)` | the same, narrowed to a `SearchScope` token |
| `plumbline_route_link_json(uri)` | parse a panel link into `{verb, …}` (engine-independent) |

One producer (`plumbline_core::panel`) over the `PanelSource` trait feeds all of
these; the FFI implements the trait on `PlumblineEngine`, so both shells read the
identical blocks. **Golden coverage (P2.6):** `panel_blocks_via_abi`
and `route_link_via_abi` exercise the block payloads + parser over the ABI, and
the producer itself has 15 unit tests over a fake source; the block kinds are a
Rust enum (a shell that meets an unknown kind renders nothing — forward-compat).

Not ported into any shell (by decision / data): signed patches + rules;
text-witness grading (shipped data never passes, so the "disputed" marker
stays silent); quotation detection (awaits hydrated inputs).

## Tier 0 daily-driver features

The eight small, additive daily-driver features from [TODO.md](../TODO.md) Tier
0. Shared logic lives in `plumbline-core`; both shells reach it through FFI
endpoints (all additive; bindings regenerated):
`plumbline_engine_copy_text`, `plumbline_engine_user_note_json` / `_notes_json` / `_set`,
`plumbline_theme_palette_json`,
`plumbline_engine_warm_indexes`, `plumbline_panel_guide_blocks_json` / `_about_blocks_json`.
New panel-link verbs: `editnote:REF`, `guide`, `about` (parse + wire in both).

- **1. Copy & context menu.** Formatting is `plumbline_core::export::copy_text`
  (verse / verse+ref / markdown / chapter). Long-press (or right-click) a verse →
  the sheet/menu: Copy · Copy chapter · Share · Tag… · Note… · Add to thread… ·
  Memorize (`ui/VerseActions.kt` / `ContextMenu.svelte`). Marking a chapter read
  lives on the passage navigator, not this menu (see the reading map). The three
  copy variants collapse into ONE **Copy** that honours Settings ▸ Copy format — a
  menu is not the place to re-ask a question the settings already answer — and
  there are no highlight swatches (highlighting was removed, see #4).
  Verse-under-point = hit word's verse, else nearest verse-number by y.
- **2. Back/forward history — WEB ONLY.** Per-pane `(book, chapter)` stack +
  cursor, seeded with the opening chapter; navigation pushes (unless it *is* a
  history move), forward entries drop on a new jump. Alt+←/→ and mouse buttons 4/5
  (`session.historyStep`, `ReaderPane.svelte`). **Delta:** on Android system Back
  dismisses the layered surface (`BackHandler`) and never steps chapters; the
  recents list from the `history` config field is the way back to a passage there.
- **3. Personal margin notes.** `plumbline_core::usernote`: one JSON file per verse
  under `home/notes/`, refKey-keyed, atomic store; empty text deletes. A new
  `PanelSource::user_note` surfaces the "your note" block (both modes) via the
  content model; the `editnote:` verb prompts (multi-line). Both shells read
  `user_notes_json` for the gutter set and `user_note_json` to prefill the prompt,
  and both mark the verse in the gutter — a square left of the weave dot on the
  web (`reader/paint.ts`), a dot beside the verse number on Android
  (`ReaderPane.kt`), which has no weave dot for it to sit beside.
- **4. Highlighting — REMOVED.** Tag colour, the six-tone
  palette, whole-verse washes, the word-precise cross-verse drag ranges, and the
  five ABI endpoints behind them (`tag_set_color`, `highlight_add` / `_remove` /
  `_clear_verse`, `chapter_highlights_json`, `theme_highlight_tones_json`) are all
  gone from the core, the ABI and both shells. Product call: *"tags + notes +
  threads are a better way to annotate and tie together scripture as we read"* —
  a tag or a note says WHY a verse matters, a colour only says THAT it does, and
  three ways to mark a verse was two too many.

  **What was kept**, because it is the reader's data and not ours: an
  `overlay-tag-v1` file written before the removal still loads whole. Serde
  ignores unknown fields, so its `color` and `highlights` keys are read past and
  fall away the next time that tag is written — the tag, its members and their
  notes survive untouched (tested: `tag::tests::a_tag_file_from_before_highlights_were_removed_still_loads`).
  Nothing migrates, nothing is deleted out from under anyone, and git has the
  implementation if it is ever asked for again.
- **5. Dark + night themes.** `plumbline_core::theme::Palette` is the one source
  (`palette(theme)`), served as `plumbline_theme_palette_json`; light values are the
  shipped ones (no regression), dark (candlelight-warm) + night (true-black) are
  new. Config gains `theme` (`system`/`light`/`dark`/`night`, additive). The
  Dark's ink went from `#e8e0d0` to `#f5f1e8` on 2026-08-17 — readers called the
  body text dim; the warm cast stays, the lightness moved, and `added` moved with
  it. `index.html`/`404.html` carry the same two hexes for the pre-boot splash
  and `e2e/splash.spec.ts` re-derives them from `theme.rs` rather than trusting
  a copy. The
  reader canvas + chrome + study panel paint from the palette in both shells —
  Android maps it into a Compose `Palette` (`ui/Palette.kt`, `colorOf` for the
  block runs' semantic roles), the web into CSS custom properties; Settings /
  the ≡ menu offers light/dark/night/follow-system and persists the choice.
  **Delta: only the web's analytical popups stay light in dark/night**
  (`MapFrame.svelte`); Android's maps sit on `palette.panelBg` and follow the
  theme. Reconcile deliberately — pick one.
- **6. Kill the first-study-click pause.** `plumbline_engine_warm_indexes` forces the
  lazy analytics (concept / leitwort) to build. **Delta:** Android calls it
  on a coroutine at startup; the web cannot — its engine is one worker thread, so
  a single blocking warm would starve every layout/tap RPC behind it, and it warms
  in slices instead (`warm_next`, see **Web shell ▸ the boot warm**).
- **7. In-app guide, shortcuts, About.** `panel::guide_blocks` / `about_blocks`
  are shared block lists (served engine-free); a Help button opens the guide in
  the panel, the guide links to About and vice-versa. **The shortcuts overlay is
  web-only** (`Shortcuts.svelte`, `?`/F1) — there are no keybindings to list on a
  phone, and Android's ⋮ menu goes straight to Guide & About.
- **8. Small unifications.** Cross-book stepping — past a book's last chapter
  enters the next, before ch.1 the previous — in **both** shells (each walks the
  TOC; `core::canon::adjacent_book` is unused, see §Multi-pane). All search
  hits band in any visible chapter (a hit set on the reader, painted at the band
  site). On the web a Shift/Ctrl-click on a `go:` link opens the other pane;
  Android has no modifiers and no second pane to open on a phone.

## Analysis tiers are OPT-IN

A first-time reader must not silently inherit a study apparatus — and on the web a
background download of the analysis pack — before ever asking for one. So
**absent means off**, in five places that must agree:
`core::config::Config::default` + `from_wire`, `ffi::wire` `config_from_wire`,
`session.gates` (`=== true`, not `!== false`), `engine.worker.ts`'s `machineOn`
(so a first visit does not prefetch the pack), and both first-runs' checkboxes
start unchecked. The flip is in the ABSENT case ONLY — a reader who switched a
tier on has an explicit `true` and keeps it.

Two traps this guards against: `SettingsDialog.toggleGate` toggles with
`!== true`, not `=== false` — under opt-in, `=== false` leaves the first click on
a never-set toggle doing nothing (`undefined === false` is false); and
`App.svelte`'s `rndDeferred` must not conflate "download deferred" with "tier is
off" (it requires `config.machineAnalysis === true`), or every phone StudyPanel
shows a "Load analysis" offer for a tier its reader never asked for. The e2e suite's `boot()`
helpers tick both tier boxes, because the tests below that measure the analysis
pack are about a reader who HAS it on.

## Per-tier analysis gates + tag→weave

Street-use feedback retired two ideas at once: the all-or-nothing
Simple/Full switch ("weirdly selective") and highlight-tones-as-annotation
(highlighting was removed outright — see Tier 0 #4).
**Tags are the primary annotation** (topic study accumulates over time); the
**weave comes later** from the tag. Both shells have the whole list —
`blocks2` (`StudyEngine.ts`), the two gate switches
(`SettingsDialog.toggleGate`), `TagPicker.svelte`, `TagWeave.svelte`, and
scroll-verse restore.

- **Gates.** `plumbline_core::panel::Gates { human, machine }` replaces the
  producer-level `full: bool`: *human* gates curated scholarship (RENDERINGS +
  reverse lens, morphology gloss, SAME ROOT, TSK), *machine* gates the
  learned/statistical tiers (ALONGSIDE, MOST USED IN, LEITWORT). The text and the
  reader's own data — author actions (`＋ tag verse` / `＋ add to thread`),
  the verse's tags + `untag`, weave xrefs, margin + personal notes, the
  compare card's `✎ note` — are **never gated**. Legacy `full:bool` fns
  remain as exact wrappers (Full = all on), so an older caller keeps working;
  neither shipping shell uses them for word/code study anymore.
- **Note-first panel.** The reader's own note block moved to the **top** of
  the word study (right under the tapped word), in every mode.
- **ABI (additive).** `plumbline_engine_word_study_blocks2_json(ref, tok, gates)`
  and `plumbline_engine_code_study_blocks2_json(code, word?, gates)` — `gates`
  bitmask: bit 0 human, bit 1 machine. `plumbline_engine_weave_from_tag(tag,
  refsJson|null=all verse members, weaveName|null=tag name, added)` chains
  the tag's passages canon-ordered (`weave::add_chain`: sorted, deduped,
  consecutive pairs; find-or-create + link-dedup make re-runs additive).
  Bindings regenerated (see §C ABI surface for the live count and the CI guard).
- **Verbs.** `makeweave:I` (→ `{verb:"makeWeave", tag}`) — emitted by the tag
  detail card whenever ≥2 verse members; the shell offers the member subset +
  name, then calls `weave_from_tag`.
- **Config (additive, round-tripping).** `humanAnalysis` / `machineAnalysis`
  booleans (absent → derived from `studyMode`), and `openPanes` entries gain
  `verse` — the pane's first visible verse, so a session **reopens
  mid-chapter**. `studyMode` still round-trips for old readers.
- **Compose.** Long-press sheet gained **Tag…** (opens
  TagPickerSheet — the reader-level tag path); `TagWeaveSheet` (member
  checkboxes, name field, Create) behind the tag card's ⇔ make weave;
  study-sheet dismissal clears the tapped-word pin (`clearPinEpoch`);
  personal-note **gutter dots** by the verse number; scroll restore via the
  config verse + `ON_PAUSE` persist; **theme switch** in Settings
  (system/light/dark/night over the core palette); Settings' Full-study
  toggle replaced by the two gate switches (persisted); **bottom nav stays
  visible** under Notes / Weaves / maps / the memorize drill / the Present
  picker (in-content overlays) — only search and the live presentation remain
  fullscreen-by-design.
- **No deltas left on this feature.**

## Backup / restore (both shells)

Settings exports the authored home — `tags/ threads/ weaves/ notes/ memory/
reading/ plans/` + the config as `.config/plumbline/config.json` + a `plumbline-backup.json`
marker — as a **zip with a shared layout**, so one archive restores across
devices (phone ↔ browser). Restore is merge-by-overwrite (same-name items
replaced), path-filtered to the authored dirs (no traversal), then the engine
re-opens over the restored home. Web: dependency-free zip (store-only write;
store+deflate read) in `apps/web/src/engine/zip.ts`, IndexedDB write with ALL
persistence frozen until the reload (three clobber paths guarded, covered by
the Playwright round-trip test). Android: `ui/Backup.kt` over SAF
Create/OpenDocument + java.util.zip; restore recreates the activity.

## The reading map — where you've read, and how long ago (both shells)

`plumbline_core::reading` (`plumbline-reading-v1`, one file per book under
`home/reading/`, plus `_since.json` for the reader's start date). Coverage of a
chapter is a **percentage**, gated two ways at once:
`min(words above the furthest verse reached, dwell × 500 wpm) ÷ chapter words`.
Scrolling to the bottom instantly credits nothing; sitting on verse 1 credits only
verse 1. Dwell is **aggregate, not per-verse** — time over verse 3 pays for verse
30 once you get there — and a pass completes at **85% with the chapter's last
verse reached** and snaps to 1.0. The snap is a tolerance on the CLOCK (nobody
re-reads a chapter because their pace ran ahead of the credited rate), not on
the chapter: 85% of the words with the end never on screen stays `Partial`
(UAT, 2026-08-12). Stored per chapter: `reached`, `dwell`
(both belong to the pass under way, cleared when it completes), `lastRead` and
`touched`. The reading rate is 500 wpm, tuned twice and both times upward: at
220, Jude's 613 words wanted 2.8 minutes of dwell, which a brisk reader beats;
at 300 × 90%, 1 Thess 3's 295 words wanted 53s and a real ~450 wpm read banked
~36s after grace — reached the end, called `Partial` (street use, 2026-08-08).
The grace period and the high-water mark are what refuse a flip-through (a
flipper banks seconds, not half-minutes); the rate does not need to be slow
as well.

Two signals in the navigator's grids: **hue** = `Standing` (unread gold
`readUnread` / partial copper `readPartial` / read sage `readDone`, all three in
`core::theme`), **bloom** = the invitation.

The bloom ramps from the most recent **contact** — `touched` (any credited
reading) or `lastRead` (a completed pass), whichever is later — flat zero for 30
days, full at 365. **Recency outranks coverage**: a chapter you were in this
morning is silent whether you finished it or stopped halfway, and one finished a
year ago but dipped into today is silent too. Without that rule a chapter read but
left short of the 90% bar would glow the moment you closed it — a false positive. A
chapter never opened is lit **from the first launch** (ramping from the reader's
start date instead would leave the map dark on precisely the day it is most use);
a part-read one tops out at what is LEFT of it. Books are the
**word-weighted** roll-up of their chapters, so chapters visibly sum to the book;
a book's `days` is the exception and reports its most recent read.

Shells own only the clock: `ReadingTracker` (Android `ui/ReadingTracker.kt`, web
`state/readingTracker.ts`) with three refusals — a grace period before accrual
(so paging through credits nothing), an idle cutoff (a phone on a table is not
reading), and stop-on-background. All thresholds come from `reading::spec()` over
the ABI, so the shells cannot drift. Reports land every 30 s and on the way out of
a chapter/app; the high-water verse comes from `onVerseReached` (Android) /
`pane.reached` (web), monotonic per chapter.

**By hand**, in the passage navigator (reading standing lives on the navigator,
and a per-verse gesture would be unfindable and not bulk-usable): a chapter tile takes a long-press (or
right-click), and the book has a **Mark whole book read** button for backfilling
from a fresh state. Full credit, for reading done in a paper Bible.

*Delta:* web pops a two-item menu at the tile — **Mark read** (today, one tap)
and **Mark read on date…** (opens `MarkReadDialog`); Android's long-press opens
that date dialog directly (its today/yesterday/last-week shortcuts are the same
one tap). Both reach today and a picked date; both offer Clear; the whole-book
button confirms then loops `mark_read` over the book's chapters. Web
`shell/BookNav.svelte`, Android `ui/BookNav.kt`.

**Perf**: write paths read ONE book file, not the store (dwell is timer-driven);
`ChapterWords` is built once per engine and cached; the web persists only
`reading/` via `home.persistUserDir` rather than diffing the whole user subtree
every 30 s.

**C ABI** (`plumbline_engine_reading_*`, 5 fns): `books_json` / `chapters_json` /
`record_json` / `mark_read` / `forget`.

## Reading plans + the Concept Study (docs/READING-PLANS.md is the design contract)

`plumbline_core::plan`: one JSON file per running plan under `home/plans/`
(`plumbline-plan-v1`, in the backup zip — `plans/` is in both shells' backup
filters and every zip-layout enumeration). Two kinds, frozen as `"schedule"` /
`"conceptStudy"`:

- **Schedule plans** are word-weighted walks generated in core from
  `ChapterWords` (never stored materialized), sequence-anchored (no backlog,
  no streaks), with completion **derived from the reading tracker** and cached
  in the plan file's `done`. One plan per class (`wholeBible` /
  `newTestament` / `devotional`); replacement is shell-confirmed. The
  chronological plan rides a curated table (`data/chronological.json`,
  `plumbline-plan-table-v1`: era-ordered segments compiled by
  `scripts/build-chronological.mjs` from `data-prep/chronological/order.json`,
  exactly-once canon coverage verified at build). `plan::load_table` reads it;
  the FFI offers the picker row — and lets a start proceed — only where the
  table actually loads, so a home without the file hides the plan instead of
  starting one that reads instantly "finished". Android bundles the table via
  `syncData`; the web pack stages it as `study`.
- **The Concept Study** (built as "speedrun", renamed through every layer
  before shipping) is a class-free, non-linear concept sweep with a preset
  tag and its own reader mode. The mode is `config.conceptStudy` (the active
  run's id; empty = normal reading) so every pane and both shells agree what
  a tap means. In the mode: verse tap = tag-with-confirm (the confirm button
  names the act, "Tag “{tag}”"), the shell's reading tracker is SUSPENDED,
  chapters are swept generously (high-water/navigation, no dwell). Exiting
  restores word-study taps and the tracker; stopping the run never touches
  the tag or its members.

**Web** (`shell/PlansScreen.svelte`, `shell/Shell.svelte` banner,
`state/session.svelte.ts` mode + sweep): Study ▸ Plans is a full SCREEN (a
destination off the Study hub, the Memorize pattern — it was a study-panel
kind until the maintainer's "crammed, not luxurious" UAT call, 2026-08-11),
sectioned Running · Study a concept · Start a plan — running-plan
cards (day card, progress, stop-with-confirm), the builtin picker, the
Concept Study launcher; a persistent mode banner with Exit and a live
`{done} / {total}` sweep count; tap-to-tag through `ReaderPane`. In the mode
the passage navigator (`shell/BookNav.svelte`) paints the RUN's coverage
instead of the (deliberately frozen) reading map — swept chapters tint as
done, part-swept books as partial — and its long-press menu / whole-book
button become "Mark chapter studied" / "Mark whole book studied" (the spec's
mark-swept-by-hand; the UI says "studied" — "sweep" survives only as the
mechanism's internal name),
writing the run via `concept_study_sweep`, never the reading record. Decision
#5's reader-side surfaces (`shell/PlanChip.svelte` + the BookNav today card,
shaped once in `shell/planToday.ts`): a nav-strip chip above the canon strip
("Day 12 · Gen 30–31", "+{n} more" opening the Plans screen) rides the reader
while a schedule runs — tap → today's first unread chapter — and the passage
navigator leads with a today card whose chapters are the buttons, read ones
marked. Both stand down in concept-study mode (the tracker is suspended, so
schedule reading there earns no credit). Finishing a full plan-day ADVANCES
the chip to the next day's portion — it does not retire (it did, per the
2026-08-12 UAT; reversed by 2026-08-18's "people want to be able to work
ahead"). `doneToday` stays on the wire (`plan::done_today` dating a finished
day by its chapters' `last_read`): the Study hub's band reads it for its
"Today's reading is done." line, drawn above the plan rows, which themselves
always show the next portion day-numbered (`plans.chip` copy, not
`plans.today`). Plans PAUSE and RESUME (`paused` in the plan file,
additive): a paused plan keeps its file, progress and class but asks nothing
— no chip, no today card — and its Plans-screen card is introduced by its
identity, the plan name plus the day it was started ("Started 3 Aug 2026 ·
paused"). E2e:
`e2e/concept-study.spec.ts` (tap-to-tag, tracker suspension, tag survival,
the Plans-screen launch path, the touch-tap ghost-click regression, the
progress surfaces) and `e2e/plans-today.spec.ts` (chip → today, the navigator
card, the mode standing both down, the day's-worth chip advance, pause →
nothing asked → resume).

**C ABI** (6 fns): `plumbline_engine_plans_json` (`now` dates each schedule's
`doneToday`; concept-study entries carry
`sweepProgress` AND the per-chapter `swept` map the navigator paints from;
every running entry carries `started` + `paused`) /
`plan_start` / `plan_stop` /
`plan_set_paused` /
`concept_study_start` (returns the run id, `!`-prefixed error otherwise) /
`concept_study_sweep`.

**Deltas**: **Android has none of this feature yet** — no `PlansScreen`, no
concept-study mode, no banner, no tap-to-tag, no sweep-coverage navigator
paint or mark-swept, no today card or plan chip; the Kotlin binding carries
the endpoints but nothing calls them. (Decision #5's today card + nav-strip
chip shipped on the web — Android owes them with the rest of the feature.)

## Memorization — spaced repetition (Tier 2 #15)

`plumbline_core::memory`: one SM-2 SRS card per verse (VRef-keyed) — ease / interval /
reps / lapses / due + a full **review log** — one JSON file per verse under
`home/memory/` (`overlay-memory-v1`, refKey + `kjv1769-tok2`). `review()` (SM-2),
`is_due`, `due_queue`, `mastery` (new/learning/young/mature), `grade_verse` +
`Card::new`/`write_card` (seed) + `remove_card`. Pure-text drills over a verse's
`body()`: `first_letters`, `blank_out(level)` (0…`MAX_BLANK_LEVEL`), and
`score_recall` (LCS-aligned per-word hit/miss + accuracy). Aggregations, both from
the log "by construction": `coverage`/`coverage_by_section` (per-verse mastery +
recency + the 8-section rollup — the coverage map) and `activity_by_day` (the
activity heatmap). Tiny civil-date math, no time dep.

**C ABI** (`plumbline_engine_memory_*`, 9 fns): add / grade / remove / card_json /
due_json / coverage_json / activity_json / drill_json / score_json. Grades cross
as `again`/`hard`/`good`/`easy`; timestamps caller-supplied UTC. Cards load fresh
per call from `home/memory` (small set); no home → read-empty / author-error.

**Both shells ship the full UI**: Memorize is a first-class destination (bottom
nav / header) opening a hub — every card, canon-sorted, with Review due /
Coverage / Activity. Android `ui/Memorize.kt`, web `memorize/` + `MemorizeHost`.
The review flow steps the due queue with a first-letter / blank-out-slider /
typed-recall drill + the four grade buttons; the coverage map reuses the canon
dispersion language shaded by mastery; activity is a calendar heatmap beside the
history log. Long-press a verse → Memorize adds a card (or a passage — see
**Passage memorization**). **Delta owed on both:** a "memorize this tag/thread"
bulk-enqueue and printable flashcards.

## Web shell (apps/web)

One of the two shipping shells: Svelte 5 + TS over the **same C ABI**, compiled
unchanged to
`wasm32-wasip1` and run in the browser under `@bjorn3/browser_wasi_shim` with
an in-memory home (data pack fetched + gunzipped into it; authored files
mirrored to IndexedDB after every write; the corpus idxcache persisted for
fast reopens). `apps/web/src/engine/StudyEngine.ts` is the method-for-method
TS sibling of `StudyEngine.kt`. Build:
`npm run pack:data && cargo build -p plumbline-ffi --release --target
wasm32-wasip1 && npm run pack:wasm && npm run build` (in apps/web). **Six
wasm-only ABI exports** live in `crates/ffi/src/wasm.rs` — `plumbline_web_alloc` /
`_free`, `plumbline_web_measure_fnptr` (the `plumbline.plumbline_js_measure`
import surfaced as a `PlumblineMeasureFn`), and the sliced-work entry points
`plumbline_engine_warm_step` / `_load_rnd_step` / `_defer_builds`. The list also
once held `_verse_sim_save` / `_verse_sim_load` / `_verse_sim_step`, which went
with "verses like this". plumbline-bindgen
excludes them from the native bindings **by name**: extend that list in
`src/bin/plumbline-bindgen.rs` when the module gains an export, or the header/Kotlin
drift check fails.

Feature state (per this manifest): reader core (canvas painter, measure via
canvas `measureText`, all flags/bands/runs/gutter marks), multi-pane
(≤3) + canon strip + ambient connectors, the whole panel content-model +
link router (incl. `makeweave:`), live search, hover gloss (native tooltip),
keyboard map + wheel + touch (pan, long-press menu, horizontal chapter
swipe), context menu (copy / copy chapter / note / tag / thread / memorize /
mark chapter read),
tag picker + tag→weave sheets, the
two map popups from the core view-models (pinch-zoom), memorization (hub /
drill / coverage / activity), Present mode (sunlight, share + the hosted
PWA link + its QR on the end card), notes browser, history, first-run,
guide/about/shortcuts,
light/dark/night/system themes from the core palette, per-tier gates,
config round-trip incl. scroll-verse restore (flushed on tab hide — the
ON_PAUSE twin), PWA (installable, offline after first visit; every pack file
content-addressed per file as `?h=<hash of its raw bytes>` — see the depot + pin
rules in CLAUDE.md, which this section does NOT restate).

**Installed-app chrome — launcher shortcuts + the icon badge (web only, by
nature).** The webmanifest declares three `shortcuts` (the long-press menu on
the installed icon): Review due → `./?open=review`, Memorize →
`./?open=memorize`, Hymnal → `./?open=hymnal`. The query is the whole contract:
`launchDestination` (`shell/church.ts`) whitelists the three values, App.svelte
opens the destination on top of the restored reader (the same states the bottom
nav sets, so the Read tab and Back dismiss it identically) and strips the query
from the address bar; anything unrecognized boots the reader as if no query
came. `manifest.spec.ts` holds the manifest's URLs to the whitelist and
`launch-shortcuts.spec.ts` holds the whitelist to the actual boot. The
**Badging API** mirrors the due-card count onto the installed icon:
`session.refreshAppBadge()` (feature-detected, fire-and-forget) runs at boot
idle, on resume, and on every authoring write (`rpc.onAuthored`) — the count
can only move while the app is running, since there is no server to push from.
**Delta:** Android has neither — a static `<shortcuts>` resource and a
notification badge are possible there but not built, and the APK's launcher
menu is empty today.

**The engine lives in ONE worker thread**, not on the main thread
(`engine/engine.worker.ts` behind the promise RPC in `engine/worker-client.ts`).
That is load-bearing rather than incidental: a single long synchronous engine call
starves every layout/tap RPC queued behind it, which is why background loading and
the boot warm are chunked with yields and why the boot-responsiveness e2e test
exists.

**The boot warm covers every index a study needs.** Nothing an
engine builds survives the tab, so if the warm covered only the SEARCH
index the occurrence index, the rendering lens, cross-refs, concepts,
leitwort and the fused bridge would all be built on the reader's
FIRST word click, in every session. `warm_next` walks
seven phases off one macrotask each: the three biggest are fed in verse slices
(`OccurrenceIxBuilder`, `RenderingsBuilder`, both mirroring the existing
`SearchIxBuilder`, both with tests pinning sliced == one-shot at every slice
size). The phase counter is explicit, so the walk
terminates rather than looping on a phase whose build cannot happen yet.
Measured in wasm: first study after a
relaunch **1235ms → 13ms**, with a regression test budgeted at 250ms from both
measured ends. The concept model is sliced too: `ConceptBuilder` carries a cursor
through twelve stages — two corpus folds, PPMI, kNN gather/top/mutual, label
propagation by node, assemble — with `Concept::build` reduced to "run it out",
so there is one implementation. Its worst slice is 16ms native, and slicing it
took the worst warm chunk in wasm from ~640ms to ~256ms. It also fixed a real
nondeterminism found while testing: edge order came out of a HashMap and broke
weight TIES, so two builds over identical data could disagree about a concept's
neighbours — the kNN truncation and the collocate lists now tie-break on the
code, matching the rest of the pipeline. `xref_ix` and `leitwort` are sliced too,
leaving `bridge` as the one phase still built in a single call, at 3ms.

**Version in About**: without it a screenshot cannot be dated and "have you
relaunched yet?" is unanswerable. `PLUMBLINE_VERSION` (the tag) is stamped by the release workflow
into `__APP_VERSION__`; About shows it with the engine version, the pack
version and the build id, selectable for pasting into a bug report. Android
reads its versionName from the package manager. **Delta:** Android's footer
notes that sideloaded builds do not auto-update.

**Updating**: `index.html` is network-first, so a relaunch with a
connection already picks up a new build — the SW script itself rarely changes
and is not the signal. Two mechanisms keep this clean: (1) every versioned URL is
content-addressed (`?h=<that file's raw-byte hash>` per pack file — `?v=<pack
version>` only for a manifest entry with no hash — `?v=<build id>` for the wasm,
hashed filenames for JS/CSS), so an update ADDS an entry beside the old one and
without a sweep nothing removes the old — three data updates would mean three
whole ~12 MB packs stranded on the device. `cache::pruneStale` sweeps, at idle after the shell
is safely re-stored, anything stamped for a version this build isn't running;
un-versioned entries (index.html, fonts, webmanifest) are never touched, so an
interrupted update cannot leave a device holding neither copy. (2) a session
that stays open — an installed PWA can sit for weeks — now compares the
deployed `index.html`'s entry-bundle hash against the running one at idle and on
resume (throttled to 15 min), and offers a toast with an Update button. Offered,
never automatic: reloading someone mid-verse to save them a tap is not a
kindness. **Delta (Android):** the APK has no auto-update at all — no Play
Store, so a sideloader fetches the new release by hand.

Web deltas. **Boot ships the
core pack only** (TODO #28): the analysis-stage artifacts
stream in after first paint —
`loadRndPack` → `plumbline_engine_load_rnd_data` —
at idle / on the first-run machine choice / on the Settings toggle, with
`studyEpoch` refreshing any open panel; until they land, the morphology gloss is
simply absent, exactly like an Android install (which never bundles
it). Phones defer the tier out of the BOOT path only — **not out of the
session**: it loads itself once first paint is behind us, so the reader is never asked twice.
The explicit "Load analysis" offer survives for exactly one case, a device on
Data Saver that hasn't got the pack yet; when the pack is already cached the
load costs no network at all and asking about a download that will not
happen is theatre. **The stage is 1.3 MB gzipped** (down from 4.0 MB once the
concept embedding left with the last features that read it); what
remains is `morphology.morphb` and the never-read `text-witness.json`. A study waiting on it says so ONCE — the
pack's own progress line, with the generic slow-first-read note suppressed
underneath it. The morphology sidecar ships **packed**, because the browser cannot
keep a parsed artifact between launches and so would repeat the whole parse on
every start:

| artifact | packed as | why | wasm parse |
|---|---|---|---|
| `morphology.jsonl` | `.morphb` — interned string table + fixed-width records | 31,091 serde calls, 355,603 entries over only 13,990 Strong's / 2,840 codes / 6 homographs | 82ms → 44ms |

`plumbline-hydrate morphb` writes it; `morph::load_morph` prefers it and falls
back to the text for any home that
lacks one (an older pack, a hand-built home, an unreadable packed file), so the
text form stays valid. `.morphb` is also ~230 KB smaller over the wire than the
JSONL, so there is no trade. (`concept-vectors.vec` was packed the same way, to
`.vecb`; that row and the `vecb` subcommand behind it were removed with the
artifact.) **Still owed:** morphology's remaining
cost is allocation, not parsing — 355,603 entries × three owned `String`s — so
lazy per-verse decoding off the packed bytes would take most of the rest;
`entries()` has exactly one caller (`plumbline_engine_morph_json`), which wants
a single token, so the change is contained. Remaining web-side deltas: the
analytical popups keep light paper while Android's follow the theme; user data
lives per-browser (export/import is the portability story); Present's "In context"
fade is Android-only (`Present.kt`'s Hide/In context button has no web twin).
Hosting: GitHub
Pages at <https://plumblinebible.org/> (custom domain; the old
github.io URL 301s there), deployed by the release workflow on every `v*`
tag (base "./", so any host or subpath works without a rebuild; the
scripture font's @font-face lives in `public/fonts.css` to keep its URLs
base-relative — that file is GENERATED by `scripts/subset-fonts.mjs`, which
subsets EB Garamond to the codepoints the corpus, Strong's and the UI actually
use and emits content-hashed woff2, 1,605 KB of TTF down to ~224 KB. The full
TTFs are build inputs in `apps/web/fonts-src/`. The charset is deliberately
generous, whole Unicode blocks: layout is measured in the engine worker and
painted on the main thread, so a glyph missing from the subset would make one
context measure a fallback advance width and the other paint Garamond's, and
lines would wrap where they are not drawn.)

## Android notes

> What follows is (a) Android-first product work, (b) the build gate, and (c) the
> **live** Android deltas, each one grepped. The sections above describe what both
> shells do; do not re-open a delta for a feature already shipped in both.
>
> The live Android deltas, all of them, in one place:
> **no canon strip** (§Canon strip) · **no ambient connectors and no weave gutter
> dot** (§Ambient weave connectors) · **no hover gloss** (§Hover gloss) · **no
> keyboard map, shortcuts sheet, or per-pane back/forward** (§Keyboard + wheel,
> Tier 0 #2) · **one pane, or two on an opened fold** (§Multi-pane) · **no
> machine-tier artifacts** (below) · **three unhandled link verbs** (below) ·
> **no way to remove a memorization card, and Clear-reading-record does not ask**
> (§Ask before destroying anything) · **first-run verse text is fetched, not
> written into the source** (§First run) · **maps follow the theme** where the
> web's stay light (Tier 0 #5) · **receives neither `?church=` nor `?at=`**
> (below) · **no auto-update** (§Web shell ▸ Updating).

- **Machine-tier data: the APK does not ship it.** `assets/data/` holds
  `kjv.jsonl`, `kjv-notes.jsonl`, `strongs.json`, `cross-references.tsv` and
  `akjv.akjvb`, plus `assets/bridge/` (abbott-smith, lxx-alignment,
  stepbible-tipnr) — extracted to `filesDir` once behind the `.data-v2` marker
  (`MainActivity`). It bundles **no** `morphology.jsonl`, and nothing calls
  `LoadRndData`. One observable consequence is left: the morphology gloss line
  never appears.
  The corpus-derived machine tiers — ALONGSIDE, MOST USED IN, LEITWORT —
  **do** work, because `Concept::build` folds the corpus and needs no artifact.
  The bridge data is reachable: the shell opens from `filesDir` and copies the
  bridge assets in.

- **On-device feedback, Android-first.** Landed
  Android-first from on-device street-use feedback; the web has since matched all
  of it except where marked:
  - **Present mode** (`ui/Present.kt`): a thread as a fullscreen, high-contrast
    ("sunlight") large-type presentation for showing someone in person —
    scrollable overview (bounce anywhere), tap-to-focus a passage huge, "In
    context" fades surrounding verses in, end card with plain-text Share. The
    sunlight palette sits on the app's warm paper (#FCF9F4, warm rules, gold
    accents, picker cards, a ✦ on the end card), still fixed-light and
    daylight-readable. **The thread picker follows the app theme** (it's the
    owner's screen; only the presentation itself stays fixed-light), and a
    destination tap always dismisses the fullscreen maps. EB Garamond ships in
    the APK (`assets/fonts/`, the web's variable-weight files; the reader's bold
    is a `wght 700` instance). Present accepts a **preselected thread**
    (`presentThreadName` / `presentThread`) so first-run "Sharing the gospel"
    opens the Romans Road directly. The share's closing line carries the hosted
    PWA link and the end card shows its QR (both shells; the verse text is
    inlined, and the take-home hands the recipient the app, not just the text).
    **Delta:** the "In context" fade is Android's alone; the web's Present has no
    equivalent.
  - **Sharing a passage is a QR, not the share sheet** (both
    shells): Present's Share opens a code rather than handing a wall of text to
    the system share sheet — this is the screen you hold up to someone in front
    of you, and what they scan should put the passage on *their* phone. The link
    carries `?at=<refKey>` (the frozen compact ref) so it opens **at the
    thread's first verse**; `shareUrl` in both `shell/church.ts` and
    `ui/Church.kt` builds it, `sharedAtRef` consumes it, and a value that isn't
    ref-shaped is rejected before it reaches navigation. The plain-text share
    stays behind a button for when the person isn't with you.
    **Delta (deliberate, matches the church one):** an Android reader *sends*
    `at` but never receives it — a plumblinebible.org link opens the PWA, not
    the APK.
  - **Embedded study maps** — **REMOVED (both shells)** (`ui/StudyMaps.kt`
    and `study/EmbedMaps.svelte`, both deleted). Two cards inside the word-study
    panel: the embedded concept map, which went with the popup, and the canon
    dispersion heatmap beside it. The heatmap looked salvageable and was not: it
    read `byBook` off the **concept-map** payload, a canon-ordered `List<Int>`
    carrying `otNtDivide` and `bookCount`. `concept_json`'s `byBook` is an
    unordered `Map<String, Int>` with neither, so keeping the strip meant writing
    a new producer, not retaining a feature. Both shells reached that conclusion
    separately, so no delta opened.
  - **Notes browser** (`ui/Notes.kt`, Explore ▸ Notes): every personal note,
    browsable; tap → passage, Edit in place. Matched on the web (Explore ▸ Notes →
    the `notesBrowser` panel).
  - **Tag picker** (`ui/VerseActions.kt` TagPickerSheet): `addtag:` offers
    existing tags first, freetext "New tag…" secondary. Plain alphabetical is the
    whole ordering, since tag colour is gone and every tag is a topic. Matched by
    `study/TagPicker.svelte`.
  - **Memorize hub layout**: coverage is an inline strip above the verse list
    (not a screen); Activity is a half/half calendar-heatmap + history-log
    split. Matched on the web.
  - Phone idioms, in both shells now: **bottom nav bar** (Read · Explore
    · Present · Memorize, one-handed reach; `ui/NavIcons.kt` — **also on the
    web's narrow layout**, same icon paths, so it is a phone idiom in
    both shells rather than an Android one), the **passage
    navigator** (`ui/BookNav.kt`: OT/NT → book → chapter tap grids, replacing
    the book dropdown — **both shells**, and there is no verse stage: sizing
    that grid would mean probing the engine for the chapter's
    verse count, so every chapter tap would wait on a round trip. Verse targeting
    still arrives via links, cross-references and search, and ReaderPane
    scrolls the target verse into view), the near-fullscreen-expandable study
    sheet, and the reader
    whitespace fix (manifest MARGIN/MAX_COLUMN are logical units — density-
    scaled on Android).
- **What the Compose shell paints (all of it).** The reader
  (`ui/ReaderPane.kt`) + word study + search + the fold layouts, plus:
  **memorization** (review drill · coverage · activity — `ui/Memorize.kt`); the
  **constellation / chord** maps as pinch-zoom/pan canvases
  (`ui/Maps.kt`); the **whole panel
  content-model** — every `*_blocks_json` / `*_blocks2_json` payload walked by
  `ui/StudyPane.kt` into `AnnotatedString` runs with the palette colour-role map,
  which is how the **RENDERINGS tier and the authority-tier marks (✝ † ≈ ⚗) and
  legend** arrive without Android owning any tier code; **link routing** through
  `plumbline_route_link_json` for 19 of the 22 verbs; the **verse-action sheet**
  (`ui/VerseActions.kt`: copy · copy chapter · share · tag · note · thread ·
  memorize · mark-chapter-read, plus the tag and thread pickers and
  `PassageEndPicker`); **Explore** (notes · threads · tags · weaves+suggested ·
  constellation · chord); **Present** (`ui/Present.kt`); the **passage navigator**
  (`ui/BookNav.kt`) tinted by the reading map; **first run** (`ui/FirstRun.kt`) and
  **church** (`ui/Church.kt`); **backup/restore** (`ui/Backup.kt`); the four themes
  off `plumbline_theme_palette_json` (`ui/Palette.kt`); the two analysis gates in
  Settings; `WarmIndexes` on a coroutine at startup.

  **Live authoring gap:** `editThreadNotes` / `editWeaveNotes` / `editEntryNote`
  are the three verbs `StudyScreen.onLink` does not handle — they need an
  index→name lookup, and the comment saying so is in the code beside the `when`.
  (`untag` and the three `delete*` verbs do that lookup now and route in both
  shells.) The web routes all twenty-two (`study/links.ts`). Also still untested
  on hardware: posture-driven fold-mode switching.
- The Kotlin/JNA binding (`crates/ffi/bindings/kotlin/Plumbline.kt`, package
  `dev.plumbline.core`) is the low-level `PlumblineNative` interface +
  JNA types (`PlumblineLayoutConfig`, `MeasureCallback`) **only**; the single
  PascalCase wrapper is `app/.../StudyEngine.kt`. It covers the whole C ABI, and
  it cannot fall behind: `plumbline-bindgen`'s `verify_surface` compares it symbol
  for symbol against the generated header and CI fails on any difference. (The C#
  sibling `bindings/csharp/PureStudy.cs` went with the WinUI shell —
  `crates/ffi/bindings/` holds `c/` and `kotlin/` now.) The native lib
  cross-builds with cargo-ndk into `jniLibs/arm64-v8a/libplumbline_ffi.so`
  (NDK r29, `--platform 26`), verified independently of the emulator/SDK.
- Build gate: Android NDK + `cargo-ndk` for the `.so`; the Rust and
  the JSON contract are identical.
- Measure callback: back it with `android.graphics.Paint.measureText` (or
  Compose's TextMeasurer); the core does the rest.
- **Phone shell — form-factor UX from on-device
  feedback.** The phone drops the always-split layout + the Split/Single and
  Bible/Study text toggles. Two layouts only (`FoldMode.UiMode`): a single
  fullscreen reader (phone/closed/tabletop), or two side-by-side panes when the
  fold is opened flat with a vertical hinge. On the phone the study surface
  (word tap / library / link / search result) is a **dismissible bottom sheet**;
  on the fold it's the right pane (Bible∥Study), toggled from the overflow menu.
  Chrome leans on **icons over text** (search, overflow, chapter arrows). Search
  is a full-screen overlay behind a 🔍 icon (field + result list) instead of an
  always-on box. The reader gets a **horizontal-swipe chapter step** (left→next,
  right→prev) and honours the new `sideMargin` / `lineSpacing` config prefs; the
  overflow menu exposes **Text & spacing** (size + margin + line-spacing sliders)
  and **Copy format** (the `copyStyle` chooser), so the long-press has a single
  **Copy** action. The web took the same shape: one Copy honouring Settings ▸ Copy
  format, and the same three sliders in `SettingsDialog.svelte`.
  Requires the `material-icons-core` dependency.
- **Phone shell, round 2.** Overflow menu cut to
  five entries — Memorize / Explore / History / Guide & About / Settings — so it
  never scrolls; the fold's second-pane flip is a top-bar icon, not a menu item.
  **Memorize** is a hub (a list of every card from `MemoryCoverageJson`, canon-
  sorted, + Review due / Coverage / Activity buttons). **Explore** is a described
  card list (Notes, Threads, Tags, Weaves, Constellation, Chord) so the tools
  aren't cryptic; **Weaves** is one screen with an All/Suggested filter (was two
  items — the web folded its Suggested card into a Weaves page on 2026-08-19).
  **History** is a bottom sheet over the `history` config field; the reader
  restores the last-viewed passage from `openPanes` and persists it + history on
  every chapter change (off-thread). **Settings** folds the two analysis gates +
  text size / margin / line-spacing + copy format + theme + church + bundled set
  into one dialog. Guide &
  About are combined in the core (`guide_blocks` inlines `about_body`; the
  standalone About card stays for the `about` link verb). Word study / library /
  search run off the main thread and the analytics index warms at startup
  (`WarmIndexes`) to kill the first-tap stall. Map pan is bounded (the shared
  `zoomable` clamps the offset so a map can't be flung off-screen; pinned at 1×).
  A memorize add shows a Toast.

- **Passage memorization + the church (both shells).**
  A memory card can cover a **passage read and recalled as one chunk**, not only
  a single verse: `Card.through` (additive in the frozen `overlay-memory-v1` —
  an older reader sees the file as a card on its opening verse rather than
  losing it) holds the inclusive last verse, always in the same book and
  chapter. A card is still keyed and filed by its FIRST verse, so every existing
  endpoint addresses it unchanged; `memory_add_passage` seeds one, and the drill
  and the recall check run over the verses joined into one text
  (`memory_span`). Two lists now differ on purpose: `coverage.verses` shades
  **every verse** a card covers (the canon coverage map), while the new
  `coverage.cards` is **one row per card** carrying `label` ("Ps 23:1–6") — the
  hub lists cards, and a remove button on an inner verse would have removed
  nothing. `coverage_by_section.cards`/`mature` count verses, so a section still
  reads as "how much of it do I know".
  **Selection UX (both shells):** the long-pressed verse is the START and the
  reader taps the END from a grid of that chapter's remaining verse numbers
  (`PassagePicker.svelte`, `PassageEndPicker` in `VerseActions.kt`) — the
  tap-grid idiom the passage navigator already uses, no new gesture, and the grid
  only offers verses that exist, which makes the same-chapter limit
  self-evident. `chapter_verse_count` is the one round trip it costs, taken when
  the picker opens.
  **Church parity:** `ui/Church.kt` is the twin of
  `shell/church.ts` (same readable `?church=…&churchInfo=…&churchUrl=…` link);
  Settings has **Your church** + "Present shares as a new believer", first run
  has the **Curious about the Bible** path and asks for the church on the two
  paths that hand the app on, the welcome a reader was given is remembered
  (`intro`) and re-readable, and Present's take-home QR + share text carry the
  reader's link instead of a bare URL. The QR is generated at render time
  (zxing-core, UTF-8 byte mode) — a build-time constant matrix for one
  fixed URL cannot carry a church. **Delta (Android):** a reader SETS and
  SHARES a church but never RECEIVES one — a plumblinebible.org link opens the
  PWA, so `App.svelte`'s incoming-church capture has no Compose counterpart; and
  Church/Welcome are overflow (⋮) items rather than top-bar buttons, because the
  phone top bar is deliberately tight.
- **Proper nouns are not concepts (engine-wide).** The concept card's
  collocate/community lists drop proper
  nouns (`strongs::is_proper_noun`) — "faith" sitting next to Ephraim, Jerusalem
  and Shechem reads as noise. Names stay fully reachable in word
  study, concordance and search; `CONCEPT_KEEP_NAMES` keeps the divine name and
  Christ, which in this corpus are concepts rather than incidental names.
  Candidates are over-fetched before filtering so a list never comes back short.
- **A cold read explains itself (both shells).** The first
  definition of a session builds the occurrence index and the first analytical
  map sweeps the corpus; a bare flash of "loading" (or blank paper) for seconds
  reads as a hang. Once a read outlasts ~600 ms, both shells add a still,
  non-pulsing note that the wait is **one-time** and the rest are instant
  (`StudyPanel.svelte`/`MapFrame.svelte`; `StudyPane.kt`/`Maps.kt`). Timed
  rather than flagged: whatever index is cold, the wait itself is the signal.
