# Shell feature manifest — the parity contract

> **2026-07-25 — the shells are Android (Compose, the UX gold standard) and
> the web PWA (`apps/web`, on `main` — the `web-shell` branch was merged).**
> The GTK and WinUI desktop shells were retired and REMOVED from the tree (git
> history has them). Sections below that cite GTK `M:<line>` refs or name
> GTK/WinUI behaviours are kept as the historical spec of *what* each feature
> does — the line refs no longer resolve, and "deltas owed to GTK/WinUI" are
> void. Where a live delta exists it is between **Android and web**, and it is
> named as such.

> **Write behaviour, not intention (2026-07-29).** Every claim here must be one
> a grep can settle: what the code *does*, and the file it does it in. The
> AKJV overlay shipped invisible on Android for a day (commit "Android had the
> whole overlay and never loaded it") because this document said the feature was
> wired when what was true was that every piece existed and nothing called it.
> "The binding has the endpoint" is not "the shell uses it" — say which one you
> mean. The audit of 2026-07-29 removed five blocks that claimed Compose lacked
> features it had shipped weeks earlier; a stale delta costs the same day a
> missing one does.

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

## Type — one face, chrome included (2026-07-28)

Both shells ship the SAME two font files, byte-identical variable TTFs (upright
851,176 bytes / italic 754,468, `fvar`, wght 400–700; paths in §Constants), and
both always used them for scripture.
The CHROME had drifted: the web sets `body { font-family: "EB Garamond" }`
(`app.css`), so every control there is Garamond, while Android called a bare
`MaterialTheme { }` and inherited Material 3's default — Roboto — everywhere but
FirstRun and Present. Fixed at the theme: `ui/Typography.kt` builds one
`FontFamily` (bold off the `wght` axis, not synthetic) and `serifTypography`
substitutes it into Material's whole type scale; `MaterialTheme` provides
`typography.bodyLarge` as `LocalTextStyle`, so a bare `Text(…, fontSize = 15.sp)`
picks the family up without naming it. Existing `sp` values are UNCHANGED —
Garamond's x-height is smaller than Roboto's, so the chrome reads slightly
smaller until re-tuned on-device.

One deliberate exception stays: the web boot splash asks for Georgia
(`App.svelte`), a face already on the device, so the very first paint waits on no
download. Garamond reaches the browser as two subset woff2 files (~112 KB + ~111 KB,
`font-display: swap`) — small, but still a network round trip the splash must not
be behind.

Intro-pane text was enlarged on both shells the same day (older eyes, and the
smaller x-height compounds it) — sizes and line heights up roughly 2 px / 2.5 sp.

## Constants + styling (M:41–54, 3807–3823)

| name | value | meaning |
|---|---|---|
| MAX_COLUMN | 720 | text column cap; centre in wider panes (`ReaderPane.svelte` `MAX_COLUMN`, `ReaderPane.kt` `MAX_COLUMN_DP`) |
| MARGIN | 28 | text margin, all sides (`MARGIN_DP` on Android — logical units, density-scaled) |
| MIN/MAX/DEFAULT bodySize | 12 / 40 / 18 | the text-size slider in BOTH shells (`SettingsDialog.svelte`, `StudyScreen.kt` `valueRange = 12f..40f`). The config accepts a wider 6–96 (`config.rs`) so an old or hand-edited file is honoured, not clamped away |
| MAX_PANES | 3 | reading columns — **web only** (`session.addPane`, and none at all when narrow). Android shows one pane, or two side by side on a fold opened flat (`FoldMode.kt`) |
| PANEL_WIDTH | 380 | the web's study sidebar, × the text-size setting (the reader zoom scales the whole study surface, width and type; 2026-07-25). Android's study surface is a bottom sheet (phone) or the second fold pane, so it has no width constant |
| OCC_SHOWN | 300 | concordance cap (`PANEL_OCC_CAP`, `crates/ffi/src/lib.rs`) |
| XREF_SHOWN | 40 | xref/link list caps (`LIST_CAP`, `crates/core/src/panel.rs`) |
| GLOSS_SAMPLE | 80 | verses sampled for the english gloss (`crates/ffi/src/lib.rs`) |
| LINK_INSET / YINSET | 14 / 5 px | connector gutter inset / clamp margin (`ConnectorsOverlay.svelte`) |

Palette: the one source is `plumbline_core::theme::palette(theme)`, served as
`plumbline_theme_palette_json` — **four themes** (light / dark / night /
follow-system), and both shells paint reader + chrome + study panel from the
returned values rather than any hex of their own. The LIGHT values, which are the
shipped originals: paper `#fcf9f4`; ink `#211f1a`; gold accent `#9e7d38`;
added-word gray `#6b6862`; divine `#4d3326`; popup paper `#f2eee6`; pane-nav bg
`#efeae1`; canon-strip bg `#ebe6db`; section-header gold `#a0894a`; rule
`#d8cba8`; faded `#8a8276`; the four tier marks and the three reading-map hues
(see their sections). Font: EB Garamond, bundled by both shells and
byte-identical between them — upright 851,176 bytes, italic 754,468
(`apps/android/app/src/main/assets/fonts/EBGaramond-Regular.ttf` ≡
`apps/web/fonts-src/EBGaramond.ttf`).

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
  from `versePerLine`. `sideMargin` and `lineSpacing` are the reader's config
  values, defaulting to 28 and 1.35.
- Paint: verse numbers **bold gold**; FLAG_ADDED italic gray; FLAG_DIVINE /
  FLAG_TITLE colors above. Hit-testing: `hit_test(x − margin_x, y − MARGIN)`.
  **No mark for a Strong's-tagged word** (both shells, 2026-07-28): there was a
  faint gold rule under every one, and since most words carry a Strong's number
  it amounted to underlining the Bible. Whether a word answers when tapped is
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
shell calls it.

**Delta:** three panes are a web thing (see MAX_PANES above). Android is one
fullscreen reader, or two panes on a fold opened flat.

## Ambient weave connectors (M:2821–2934) — WEB ONLY

**Delta, verified 2026-07-29: this is a web feature.** `ConnectorsOverlay.svelte`
is the only consumer of `plumbline_engine_link_pairs_json` in the tree. Android
*declares* the endpoint (`StudyEngine.kt` `LinkPairsJson`) and never calls it — no
connector overlay, and no in-pane weave gutter dot either (`ReaderPane.kt` paints
a gutter dot for personal notes only). That is defensible on a phone, where one
pane means there is nothing to draw a line between; it is written down here
because the endpoint being in the binding used to read as the feature being in
the shell.

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
trimmed to 80 chars. **Delta:** there is no hover on a touch screen, so Android
has none — a tap opens the study surface instead. *Data*:
`plumbline_layout_hit_test_json` + `plumbline_engine_strongs_json`.

## Word study panel (click a word — 2026-07-25, was double-click; M:3168–3515)

The web's 380-px sidebar (scaled by the text-size setting); on Android a
dismissible bottom sheet on a phone, or the second pane on an opened fold.
On-demand; Esc / a swipe hides; clearing search hides. Content order — (F) marks
what the *machine* or *human* gate turns on (see **Per-tier analysis gates**;
"Full" is the pre-2026-07-25 name for both gates on).

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
   - **SIMILAR CONCEPTS** — **REMOVED 2026-07-30**, at Glendon's call: embedding
     neighbours plus an "across the testaments" cross list. Cut from
     `panel.rs`, so it went from both shells at once, along with the
     `concept_near` trait method and its FFI implementation, which had no other
     caller. The function-word filter this section needed (`rnd::stopwords` —
     2026-07-25, *believe* was offering *because*) is still live for APPEARS
     ALONGSIDE below. The embedding index it read went too, later the same day,
     when "verses like this" (item 7) and the concept map followed it out.
   - **APPEARS ALONGSIDE** *(Machine ≈)* — concept community (8), same
     function-word filter.
   - **MOST USED IN** *(Machine ≈)* — top books (5) "Book ×N · …" + "(OT x · NT y)".
     (Named WHERE IT CONCENTRATES until the 2026-07-30 copy pass.)
   - **LEITWORT** *(Machine ≈)* — "{winCount} of its {n} uses cluster in {label} (p ≈ 10^−{score})".
   - **"▸ open concept map" link** — **REMOVED 2026-07-30** with the popup it
     opened; see §Concept map popup below. The three sections above it are the
     symbolic concept engine (co-occurrence over the corpus) and are untouched.
4. (F) Author actions: `＋ tag verse`, `＋ add to thread`.
5. **cross-references (N)** — weave partners (≤40), each + weave-name link to
   its compare card.
6. (F) **study cross-references (N) — TSK** *(Human †)* (≤40; ranges "a–b").
7. **verses like this** — **REMOVED 2026-07-30**, at Glendon's call: a per-verse
   list of statistically similar verses (the SIF model over the concept
   embedding, 6 in-testament and 4 cross-testament). Machine-generated noise, in
   his judgement. It lived in `panel.rs` and in the core's `VerseSim`, so it went
   from both shells at once, taking `PanelSource::similar_verses`,
   `plumbline_engine_similar_verses_json` and the wasm-only
   `plumbline_engine_verse_sim_save` / `_load` / `_step` with it. It was the last
   feature reading the concept embedding, so `data/concept-vectors.vec` (+
   `.vecb`, `.meta`, `.freq`) left the data pack too: 3.08 MB of a 6.4 MB
   analysis tier, which now holds morphology and text-witness only. One code
   path still opens the file — `plumbline_engine_concept_neighbours_json`, which
   no shell has ever called (see §C ABI surface).
8. (F) **tags** — tags holding this verse; each is a link + `✕` untag (user
   data, not evidence — no tier mark).
9. **margin notes** *(Human †)* — the verse's 1769 notes, small.

A **provenance legend** closes a Full-study card once: "where this comes from:
✝ the text · † curated scholarship · ≈ machine-derived, weigh it · ⚗
research-grade". Weave cross-references (item 5) and tags carry no mark (mixed /
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

## Plain-English overlay (the AKJV delta, 2026-07-27, both shells)

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
- **The mark is a DOTTED gold underline**, at the natural underline depth — it
  sat 3px lower while it had to clear the Strong's rule above it, and moved up
  when that rule was removed (2026-07-28). Not bold and not grey: italic already means "supplied by the
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
  disk there and staging would only add a race. Android had every piece of this
  feature — binding, dotted mark, tap header, Settings toggle — and shipped it
  invisible from 2026-07-27 to 2026-07-28 because nothing called that one
  function, so this line is here to keep the requirement next to the claim.

## Authority tiers — provenance marks on evidence

Ported from overlay `Bridge.hs` `Tier` + `Panels.hs` `provIcon`/`tierMarks`.
Every piece of study evidence shows where it comes from, so the reader always
knows its provenance. The model is `plumbline_rnd::bridge` (`crates/rnd/src/bridge.rs`):

- `Tier = God | Human | Machine`. **God** = the text itself (TR/Masoretic words,
  and scripture-quotes-scripture, "the words read twice"). **Human** = curated
  scholarship (lexicons, the 1769 renderings, TSK). **Machine** = a
  learned/aligned artifact (the LXX alignment, embeddings, the R&D layer), and
  the default for an unrecognized source so nothing over-claims.
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
re-splits the string. The 19 verbs, all of them in `PanelLink`:
`go:Book:ch[:v]` · `occ:CODE` · `rend:CODE:rendering` · `code:CODE[:word]` ·
`thread:i` · `tag:i` · `weave:i` · `addtag:refkey` ·
`addthread:refkey` · `untag:i:refkey` · `makeweave:i` · `approve:i` · `reject:i` ·
`editthreadnotes:i` · `editweavenotes:i` · `editentrynote:ti:ei` ·
`editnote:refkey` · `guide` · `about`. An unknown verb or a malformed payload
parses to `None` and the shell ignores the click.

`conceptmap:CODE` was the twentieth until 2026-07-30; it left the vocabulary with
the popup it opened (see §Concept map popup).

Navigation + native prompts + the write choreography (author endpoint →
reload → refetch) stay shell-side. `parse_link` handles multi-word books
("1 John") and colon-bearing refkeys; `code:CODE[:word]` keeps its `word`
(a surface token) and opens the standalone code-study card — distinct from
`occ:CODE` (verse list) and `rend:CODE:rendering` (filtered list).

## Concordance (`occ:`; M:3783)

Code + lemma large + count; verse links capped at 300, "… N more".
*Data*: `plumbline_engine_strongs_occurrences_json` (cap 500 engine-side).

## Thread picker + delete (2026-07-28, both shells)

`Add to thread…` was a bare freetext prompt, which made the common case — adding
a fifth passage to the thread you have been building all week — require retyping
its name exactly, and a typo silently FORKED a second thread instead of failing.
It is now the same picker idiom tags already had: existing threads are a list you
tap (with entry counts), freetext only for a genuinely new one, and `✕` deletes a
thread and everything on it (the verses are untouched). `addTag`'s study-panel
route was switched to the tag picker for the same reason.

Core: `thread::remove_thread` (case-insensitive, like `add_to_thread`; an absent
name is a no-op, not an error). **C ABI**: `plumbline_engine_thread_remove`.
Shells: `ThreadPickerSheet` (`ui/VerseActions.kt`) / `ThreadPicker.svelte`.

## Ask before destroying anything (2026-07-29, both shells)

One confirmation per shell, and whether an action asks is a property of the
**action**, not of whoever wrote its button — the app had four different answers
to that question. `ui/Confirm.kt` (`ConfirmRequest` + `ConfirmDialog`) and
`shell/ConfirmDialog.svelte` (behind `session.askConfirm(title, body, verb)`,
which returns a promise). The confirm button **names the act** — "Delete thread",
"Remove card", "Reject" — never "OK", so a reader who half-read the sentence still
knows what it does; that button is the tinted one (`tierResearch`, the app's one
alarm colour).

Behind it, per shell — grepped, because this is exactly the kind of list that
rots:

| destructive act | web | Android |
|---|---|---|
| delete a thread | asks (`ThreadPicker.svelte`) | asks (`VerseActions.kt`) |
| reject a suggested weave | asks (`study/links.ts`) | asks (`StudyScreen.kt`) |
| untag a verse | asks (`study/links.ts`) | **the `untag` verb is unhandled** — see Android notes |
| remove a memorization card | asks (`MemorizeHost.svelte`) | **no remove affordance at all**; `MemoryRemove` is an uncalled wrapper |
| clear a chapter's reading record | asks (`MarkReadDialog.svelte`) | **does not ask** — `onClear` calls `ReadingForget` and toasts |

The last three are live Android gaps, not design.

## Threads / Tags browsers (M:3380–3471)

List → detail. Threads list: "Threads (N)", each name + "N passage(s)".
Thread detail: name, notes small + `✎ notes`; per entry: verse link, snapshot
`text.join(" ")` truncated 70 + "…", note `#888` + `✎ note`. Tags list ↔
detail analogous; members: verse → go link, concept → concordance link; note
trailing. Authoring: `＋ tag verse` prompts a name (find-or-create,
case-insensitive) → `tag_add(name, "verse", refkey, null, now)`;
`＋ add to thread` snapshots the whole verse (span 0..last token, words
vector) → `thread_add`; `untag` → `tag_remove`. Note edits: `thread_set_notes`
/ `thread_entry_set_note` / `weave_set_notes` via a pre-filled text prompt
(empty submission clears). *Data*: `threads_json`, `tags_json` + the above.

## Suggested-weave review (M:2631, 3477)

Filter library `suggested == true`. Per weave: name bold + kind label gray,
notes, links ≤40 as "a ↔ b" go-links; actions `⇔ compare` `✓ approve`
`✕ reject` `✎ note`. Approve merges into `weaves/` (all links approved);
reject deletes the file and **asks first in both shells** — it is deleted, not
hidden, and does not come back for review. Ordinals shift after every write — always re-fetch.
*Data*: `suggested_weaves_json`, `weave_approve/reject(index)`.

## Weave authoring (M:2137–2236)

**The desktop-era pin/＋link flow was removed 2026-07-25** (single left-click
now opens word study, matching the Compose tap): weave links are authored via
the tag→weave sheet (`makeweave:` — tag passages as a topic, then chain them),
in both shells. `plumbline_engine_weave_add_link_spans` remains in the ABI for
span-precise links; no shell surfaces it today. **Compare card** (`weave:i`): name + kind +
"(suggested)"; "N link(s)" + (F) `✎ note`; per link ≤40: label `"…"` gold,
each side verse link + verse text small with **span words bold** and added
words italic gray. *Data*: `plumbline_engine_weave_add_link_spans`, `weaves_json`,
`verse_json` (tokens for span rendering).

**Opening a weave pulls its passages up (2026-07-25, both shells).** The
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

**Delta, verified 2026-07-29.** `CanonStrip.svelte` (mounted under the pane row in
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

## Addressable chapters (2026-07-30) — WEB ONLY, by nature

Pane 0 mirrors into `location.hash` as `#/John/3`, so a chapter can be
bookmarked, shared or reloaded onto itself. `replaceState` on an ordinary
chapter turn, `pushState` only when a transient surface opens — that
distinction is the feature, not an implementation detail: without it a reader
flicking through Psalms needs forty Back presses to leave, and Back steps them
back through their own reading instead of closing the sheet in front of them.

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

## Search (M:660, 3739)

Live per keystroke; empty query closes the panel. `goto` answer → big "go to"
link (navigates active pane; verse target gets the band). `hits` → "N
result(s)" + tier phrase small; per hit: verse link, gray `why`, "※ note"
marker for margin-note matches; "… N more" past cap. *Data*: `search_json`.

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

`{"studyMode":"simple"|"full","bodySize":18.0,"openPanes":[{"book","chapter","verse"}],
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

## First run — who is opening the Book? (2026-07-26, both shells)

The first launch asks who's here (`FirstRun.svelte` / `FirstRun.kt` — keep
the copy in sync). **Four paths, in this order (2026-07-28): Curious about the
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
  2 Tim 3:16–17). **The verses are QUOTED inline** (2026-07-26 — the new
  believer reads scripture itself, not a row of links); every reference is
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

A link shared from Present offers only the first two paths — it was handed to
someone in person, and the rest is setup for a reader who already has a Bible
habit. Either welcome is re-readable later (the `intro` config field remembers
which one a reader was given): web a **Welcome** header button, Android an
overflow (⋮) item.

**Delta — where the quoted verse text comes from.** The two shells differ, on
purpose, and the manifest used to claim both fetched: **Android** asks the engine
(`FirstRun.kt`, `ALL_QUOTED` → `VerseJson` per refKey, off-thread into a `bodies`
map). **The web writes the text out in the source** (`FirstRun.svelte`'s `REF`
table, 15 entries) — asking for ten verses one at a time made the quotes pop in a
beat after the page, and this is the first screen a new believer sees
(feedback 2026-07-27). The 1769 text is frozen, so a copy cannot drift; each
entry was taken verbatim from `data/kjv.jsonl` as `Verse::body()` renders it. If
you add a quote to the web welcome you are adding *text*, not a reference.

The old Simple/Full first-run modal is gone; `studyMode` still round-trips in
the config for readers of an older file.

## Primary menu (≡)

**Reworked 2026-07-26 (web): destinations vs utilities.** The web header now
mirrors the Compose IA — Read is the base layer; **Explore · Present ·
Memorize** are first-class header buttons, plus search, a first-class **Share**
button, and a ≡ menu holding utilities only (History · Guide & about · Keyboard
shortcuts · Settings).

**Phone parity 2026-07-28 (web): the bottom nav bar.** Narrow screens used to
fold the three destinations into the ≡ menu above a divider, which put the whole
information architecture two taps away behind a glyph. The web now draws the
same **bottom nav bar Android has** — Read · Explore · Present · Memorize, in
thumb reach — using the very same Material paths the Compose shell does
(`ui/NavIcons.kt` → the `NAV` table in `Shell.svelte`), with gold on the current
tab and Compose's α0.14 gold pill behind its icon. The ≡ menu is utilities only
at every width now. Read is not a destination so much as the absence of one: the
reader is always mounted underneath, so its tap clears whatever is layered over
it. **Delta:** Android's four destinations are mutually exclusive because it
shows one screen at a time; the web layers, and on a desktop the study panel is
a sidebar, so switching to Present or Memorize leaves an open Explore panel
behind it to return to. The highlighted tab always names the surface actually in
front of the reader. Threads/Tags/Weaves live inside Explore, as on Android. The
subtitle is just the passage ("John 3" — no edition suffix; the e2e boot
signal matches `/\w+ \d+/`). **Share the app** (2026-07-25, both shells)
opens a QR of the hosted PWA link + the link itself via system share / copy; the
same QR closes Present's end card. The matrix is **encoded at render time** in
both shells — `QrCode.svelte` over qrcode-generator, `QrShare.kt` over
zxing-core, both forcing UTF-8 byte mode — because the link carries the reader's
church and there is no one fixed URL to bake in (it was a build-time constant
matrix until 2026-07-27). Two more conditional header buttons appear when there is
something to point at: **Welcome** (re-read the intro) and **Church** (the link
the reader was handed, or their own) — front and centre rather than in Settings,
because someone gave this reader a church and they should not go hunting for it.
Android carries both as overflow (⋮) items instead; its top bar is deliberately
tight.

Historical menu notes (the retired desktop shells): **Weave views** (Suggested,
Weave map, Constellation — disabled outside Full study), **Reading** (Simple/Full
radio + Verse-per-line toggle), **Theme** (light/dark/night/follow-system radio),
and **Guide / Keyboard shortcuts / About**. GTK: a `gtk::MenuButton` + `gio::Menu`
backed by `win.*` `SimpleAction`s (string-stateful radios, boolean toggle). WinUI:
a `DropDownButton` + `MenuFlyout` with `RadioMenuFlyoutItem` /
`ToggleMenuFlyoutItem`.

**Explore's contents** (both shells, a described card list so the tools aren't
cryptic): Notes · Threads · Tags · Weaves · Constellation · Weave map. *Delta:*
the web lists **Suggested** as its own seventh card (`ExploreScreen.svelte`);
Android folds it into one Weaves screen with an All/Suggested filter
(`WeavesScreen`).

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
unlike the connector overlay, which draws only resolved ones.

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

## Concept map popup — REMOVED 2026-07-30

The radial graph opened by "▸ open concept map" (`conceptmap:CODE`) is gone, at
Glendon's call: a Strong's code ringed by its embedding-near and community
neighbours over a canon dispersion strip, plus the cross-testament bridge row
that rode inside the same payload. Machine-generated noise, in his judgement.

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

**95 native fns as of 2026-07-30** (97 until `similar_verses_json` and
`concept_map_json` were removed that day), plus 6 wasm-only shims in
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
`weave_set_notes`), R&D (`concept_neighbours_json`, `bridge_partners_json`,
`morph_json`). `similar_verses_json` was here too until 2026-07-30.

**`concept_neighbours_json` is a dead endpoint** and has been for some time: both
shells carry a wrapper (the binding covers the whole ABI automatically) and
neither has a call site. Since 2026-07-30 it is also the only code left that
opens `data/concept-vectors.vec`, which the pack no longer ships, so it can only
answer empty. A candidate for deletion.

Added for shell parity (2026-07-14):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_verse_notes_json(ref)` | `{verse, notes[]}` or null | margin notes |
| `plumbline_engine_study_xrefs_json(ref)` | `{verse, refs:[{to, toDisplay, end?, votes}]}` | TSK tier |
| `plumbline_engine_weaves_json()` | full library: weaves + links incl. `approved`, `spanA/B`, `resolved`, `suggested` | compare card, weaves list, panel xrefs (chord map + constellation now have their own view-model endpoints) |
| `plumbline_engine_concept_json(code)` | `{total, ot, nt, topBooks, byBook, collocates, community, leitwort?}` | ALONGSIDE / MOST USED IN / LEITWORT / dispersion |
| `plumbline_engine_gloss(code)` | plain english gloss or null | concept chips |
| `plumbline_engine_weave_add_link_spans(name, a, b, aLo, aHi, bLo, bHi, added)` | null/error (negative span = none) | word-span links |
| `plumbline_config_load_json()` / `plumbline_config_save_json(json)` | config wire above (+`firstRun` on load) | session/mode/zoom |

Added for the rendering lens (2026-07-16):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_renderings_json(code)` | `{code, renderings:[{rendering, total, capped, refs:[{verse, display, span:[s,e]}]}]}` (refs cap 500) | RENDERINGS tier + filtered concordance |
| `plumbline_engine_word_codes_json(word)` | `{word, codes:[{code, count}]}` | "also translates" reverse line |

Extended for authority tiers (2026-07-16): `plumbline_engine_bridge_partners_json`
partners gained **additive** fields `tiers` (`["god"\|"human"\|"machine"]`,
deduped, ordered God→Human→Machine) and `researchGrade` (bool). Existing
`code`/`sources`/`prior` unchanged; a consumer that ignores the new fields sees
the pre-tier behaviour. No extern-surface change → bindings unchanged.

Added for the view-model consolidation (2026-07-16, architecture-review P0.3 —
the warm-up that moves shared derivation out of the shells into the core):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_link_pairs_json()` | `{pairs:[{a, aBook, aChapter, aVerse, b, bBook, bChapter, bVerse, resolved}]}` | ambient connectors (web only — Android's wrapper is uncalled) |
| `plumbline_engine_canon_segments_json()` | `{segments:[{label, first, last}], otNtDivide}` | canon strip (web) / passage navigator + map ruler bands + memorize coverage (both) |

Both are thin wrappers over the one core source: `link_pairs` wraps
`plumbline_core::weave::link_pairs`; `canon_segments` wraps
`core::reference::CANON_SEGMENTS` / `OT_NT_DIVIDE`.

Added for the popup view-models (2026-07-18, architecture-review P0.2 — the
map popups' derivation moved into the core; positions cross the wire as
**fractions/logical units**, never pixels/colours):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_chord_map_json()` | `{pairs:[{a, b, count}] (canon book indices, a≤b), max, otNtDivide, bookCount}` | chord/arc "Weave map" (retires the shell fold + max) |
| `plumbline_engine_constellation_json(page, pins_json)` | `{lanes:[{weaveIndex, name, pinned, nodes:[{x, laneFrac, size, refKey, book, chapter, verse, display}], edges:[{aX, aLaneFrac, bX, bLaneFrac}]}], nPins, freeTotal, page, maxPage, caption, laneCapacity}` (pins = JSON array of weave indices) | constellation (retires the usable/degree/jitter/paging/pin derivation) |

Producers: `chord_map` wraps `plumbline_core::weave::chord_pairs`; `constellation`
wraps `plumbline_core::weave::constellation`. Both shells consume the
JSON and map fractions → pixels; neither re-derives anything. A third row,
`plumbline_engine_concept_map_json(code)`, stood here until 2026-07-30.

Added for the panel content-model + link router (2026-07-18, P0.1 + P1.4 — the
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

## Tier 0 daily-driver features (2026-07-19)

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
  Memorize · Mark chapter read… (`ui/VerseActions.kt` / `ContextMenu.svelte`).
  **Trimmed 2026-07-29:** the three copy variants collapsed into ONE **Copy** that
  honours Settings ▸ Copy format — a menu is not the place to re-ask a question
  the settings already answer — and the highlight swatches went with highlighting
  itself. Verse-under-point = hit word's verse, else nearest verse-number by y.
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
- **4. Highlighting — REMOVED 2026-07-29 (v0.33.0).** Tag colour, the six-tone
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

## Analysis tiers are OPT-IN (2026-07-28)

They were opt-out. A first-time reader silently inherited a study apparatus —
and on the web a background download of the analysis pack — before ever asking
for one. Now **absent means off**, in five places that must agree:
`core::config::Config::default` + `from_wire`, `ffi::wire` `config_from_wire`,
`session.gates` (`=== true`, not `!== false`), `engine.worker.ts`'s `machineOn`
(so a first visit does not prefetch the pack), and both first-runs' checkboxes
start unchecked. The flip is in the ABSENT case ONLY — a reader who switched a
tier on has an explicit `true` and keeps it.

Two traps this sprang, both fixed: `SettingsDialog.toggleGate` used
`s.config[key] = s.config[key] === false`, which under opt-in left the first
click on a never-set toggle doing nothing (`undefined === false` is false) — it
is `!== true` now; and `App.svelte`'s `rndDeferred = deferRnd && !info.rndAuto`
conflated "download deferred" with "tier is off", which would have shown every
phone StudyPanel's "Load analysis" offer for a tier its reader never asked for —
it now also requires `config.machineAnalysis === true`. The e2e suite's `boot()`
helpers tick both tier boxes, because the tests below that measure the analysis
pack are about a reader who HAS it on.

## Per-tier analysis gates + tag→weave (2026-07-25, product round 4)

Street-use feedback retired two ideas at once: the all-or-nothing
Simple/Full switch ("weirdly selective") and highlight-tones-as-annotation
(highlighting was removed outright three days later — see Tier 0 #4).
**Tags are the primary annotation** (topic study accumulates over time); the
**weave comes later** from the tag. Landed core-first on Android; **the web has
the whole list now too** — `blocks2` (`StudyEngine.ts`), the two gate switches
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
- **Compose (landed 2026-07-25).** Long-press sheet gained **Tag…** (opens
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
- **No deltas left on this feature.** (The "deltas owed to GTK/WinUI" that stood
  here are void — those shells are gone — and the web's list closed when the
  shell merged to `main`.)

## Backup / restore (2026-07-25, both shells)

Settings exports the authored home — `tags/ threads/ weaves/ notes/ memory/
reading/` + the config as `.config/plumbline/config.json` + a `plumbline-backup.json`
marker — as a **zip with a shared layout**, so one archive restores across
devices (phone ↔ browser). Restore is merge-by-overwrite (same-name items
replaced), path-filtered to the authored dirs (no traversal), then the engine
re-opens over the restored home. Web: dependency-free zip (store-only write;
store+deflate read) in `apps/web/src/engine/zip.ts`, IndexedDB write with ALL
persistence frozen until the reload (three clobber paths guarded, covered by
the Playwright round-trip test). Android: `ui/Backup.kt` over SAF
Create/OpenDocument + java.util.zip; restore recreates the activity.

## The reading map — where you've read, and how long ago (2026-07-28, both shells)

`plumbline_core::reading` (`plumbline-reading-v1`, one file per book under
`home/reading/`, plus `_since.json` for the reader's start date). Coverage of a
chapter is a **percentage**, gated two ways at once:
`min(words above the furthest verse reached, dwell × 300 wpm) ÷ chapter words`.
Scrolling to the bottom instantly credits nothing; sitting on verse 1 credits only
verse 1. Dwell is **aggregate, not per-verse** — time over verse 3 pays for verse
30 once you get there — and a pass completes at **90%** and snaps to 1.0, so there
is never a trailing verse to chase. Stored per chapter: `reached`, `dwell`
(both belong to the pass under way, cleared when it completes), `lastRead` and
`touched`. The reading rate went 220 → 300 wpm the same day: at 220, Jude's 613
words wanted 2.8 minutes of dwell, which a brisk reader beats, so a real read came
out `Partial`. The grace period and the high-water mark are what refuse a
flip-through; the rate does not need to be slow as well.

Two signals in the navigator's grids: **hue** = `Standing` (unread gold
`readUnread` / partial copper `readPartial` / read sage `readDone`, all three in
`core::theme`), **bloom** = the invitation.

The bloom ramps from the most recent **contact** — `touched` (any credited
reading) or `lastRead` (a completed pass), whichever is later — flat zero for 30
days, full at 365. **Recency outranks coverage**: a chapter you were in this
morning is silent whether you finished it or stopped halfway, and one finished a
year ago but dipped into today is silent too. Without that rule a chapter read but
left short of the 90% bar glowed the moment you closed it (2026-07-29: "I just read
the book of Jude and it now shows a bronze glow — bit of a false positive"). A
chapter never opened is lit **from the first launch** (it used to ramp from the
reader's start date, which left the map dark on precisely the day it is most use);
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

**By hand**: long-press a chapter's *first verse* → "Mark chapter read…" → a date
picker with today/yesterday/last-week shortcuts, plus Clear. Full credit, for
reading done in a paper Bible; kept to verse 1 so it is findable but not
bulk-usable.

**Perf**: write paths read ONE book file, not the store (dwell is timer-driven);
`ChapterWords` is built once per engine and cached; the web persists only
`reading/` via `home.persistUserDir` rather than diffing the whole user subtree
every 30 s.

**C ABI** (`plumbline_engine_reading_*`, 5 fns): `books_json` / `chapters_json` /
`record_json` / `mark_read` / `forget`.

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

## Web shell (apps/web — 2026-07-25, merged to `main`)

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
`plumbline_engine_warm_step` / `_load_rnd_step` / `_defer_builds`. There were
nine until 2026-07-30: `_verse_sim_save` / `_verse_sim_load` / `_verse_sim_step`
went with "verses like this". plumbline-bindgen
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

**The engine lives in ONE worker thread**, not on the main thread
(`engine/engine.worker.ts` behind the promise RPC in `engine/worker-client.ts`).
That is load-bearing rather than incidental: a single long synchronous engine call
starves every layout/tap RPC queued behind it, which is why background loading and
the boot warm are chunked with yields and why the boot-responsiveness e2e test
exists. (This section claimed "engine runs on the main thread, a worker is the
escape hatch" until 2026-07-29 — it had been a worker for a while.)

**The boot warm covers every index a study needs** (2026-07-27). Nothing an
engine builds survives the tab, and the warm used to cover only the SEARCH
index — so the occurrence index, the rendering lens, cross-refs, concepts,
leitwort and the fused bridge were all built on the reader's
FIRST word click, in every session, forever ("wipe data, click a word, it
thinks; close and reopen, click a word, it thinks again"). `warm_next` walks
seven phases off one macrotask each: the three biggest are fed in verse slices
(`OccurrenceIxBuilder`, `RenderingsBuilder`, both mirroring the existing
`SearchIxBuilder`, both with tests pinning sliced == one-shot at every slice
size). The phase counter is explicit, so the walk
terminates rather than looping on a phase whose build cannot happen yet.
Measured in wasm: first study after a
relaunch **1235ms → 13ms**, with a regression test budgeted at 250ms from both
measured ends. The concept model is sliced too (2026-07-27): `ConceptBuilder` carries a cursor
through twelve stages — two corpus folds, PPMI, kNN gather/top/mutual, label
propagation by node, assemble — with `Concept::build` reduced to "run it out",
so there is one implementation. Its worst slice is 16ms native, and slicing it
took the worst warm chunk in wasm from ~640ms to ~256ms. It also fixed a real
nondeterminism found while testing: edge order came out of a HashMap and broke
weight TIES, so two builds over identical data could disagree about a concept's
neighbours — the kNN truncation and the collocate lists now tie-break on the
code, matching the rest of the pipeline. `xref_ix` and `leitwort` were sliced
next (2026-07-30), leaving `bridge` as the one phase still built in a single
call, at 3ms.

**Version in About** (2026-07-27): the web build had no idea which release it
was, so a screenshot could not be dated and "have you relaunched yet?" was
unanswerable. `PLUMBLINE_VERSION` (the tag) is stamped by the release workflow
into `__APP_VERSION__`; About shows it with the engine version, the pack
version and the build id, selectable for pasting into a bug report. Android
reads its versionName from the package manager. **Delta:** Android's footer
notes that sideloaded builds do not auto-update.

**Updating** (2026-07-27): `index.html` is network-first, so a relaunch with a
connection already picks up a new build — the SW script itself rarely changes
and is not the signal. Two gaps closed: (1) every versioned URL is
content-addressed (`?h=<that file's raw-byte hash>` per pack file — `?v=<pack
version>` only for a manifest entry with no hash — `?v=<build id>` for the wasm,
hashed filenames for JS/CSS), so an update ADDED an entry beside the old one and
nothing ever removed the old — three data updates meant three whole ~12 MB packs
stranded on the device. `cache::pruneStale` now sweeps, at idle after the shell
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
core pack only** (2026-07-25, TODO #28): the analysis-stage artifacts
stream in after first paint —
`loadRndPack` → `plumbline_engine_load_rnd_data` —
at idle / on the first-run machine choice / on the Settings toggle, with
`studyEpoch` refreshing any open panel; until they land, the morphology gloss is
simply absent, exactly like an Android install (which never bundles
it). Phones defer the tier out of the BOOT path only — **not out of the
session** (revised 2026-07-27 from the 2026-07-26 defer-until-asked rule): it
loads itself once first paint is behind us, so the reader is never asked twice.
The explicit "Load analysis" offer survives for exactly one case, a device on
Data Saver that hasn't got the pack yet; when the pack is already cached the
load costs no network at all and asking about a download that will not
happen is theatre. **The stage shrank on 2026-07-30**, from 4.0 MB gzipped to
1.3 MB, when the concept embedding left with the last features that read it; what
remains is `morphology.morphb` and the never-read `text-witness.json`, so any
copy quoting the old size is stale. A study waiting on it says so ONCE — the
pack's own progress line, with the generic slow-first-read note suppressed
underneath it. The morphology sidecar ships **packed**, because the browser cannot
keep a parsed artifact between launches and so repeated the whole parse on
every start (2026-07-27):

| artifact | packed as | why | wasm parse |
|---|---|---|---|
| `morphology.jsonl` | `.morphb` — interned string table + fixed-width records | 31,091 serde calls, 355,603 entries over only 13,990 Strong's / 2,840 codes / 6 homographs | 82ms → 44ms |

`plumbline-hydrate morphb` writes it; `morph::load_morph` prefers it and falls
back to the text for any home that
lacks one (an older pack, a hand-built home, an unreadable packed file), so the
text form stays valid. `.morphb` is also ~230 KB smaller over the wire than the
JSONL, so there is no trade. (`concept-vectors.vec` was packed the same way, to
`.vecb`; that row and the `vecb` subcommand behind it went on 2026-07-30 with the
artifact.) **Still owed:** morphology's remaining
cost is allocation, not parsing — 355,603 entries × three owned `String`s — so
lazy per-verse decoding off the packed bytes would take most of the rest;
`entries()` has exactly one caller (`plumbline_engine_morph_json`), which wants
a single token, so the change is contained. Remaining web-side deltas: the
analytical popups keep light paper while Android's follow the theme; user data
lives per-browser (export/import is the portability story); Present's "In context"
fade is Android-only (`Present.kt`'s Hide/In context button has no web twin).
Hosting decided 2026-07-25: GitHub
Pages at <https://plumblinebible.org/> (custom domain, same day; the old
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

> **Audited 2026-07-29.** Eight "not yet in a Compose shell" blocks used to close
> this section — memorization, the rendering lens, the launcher icon, authority
> tiers, the view-model consolidation, the panel content-model + link router, the
> popup view-models, and Tier 0. **All eight were false**; every one of those
> features had shipped, some of them weeks earlier, and several of the blocks were
> written before the Compose shell existed and never revisited. They are deleted
> rather than corrected, because the sections above already describe what both
> shells do. What survives below is (a) Android-first product work, (b) the build
> gate, and (c) the **live** Android deltas, each one grepped.
>
> The live Android deltas, all of them, in one place:
> **no canon strip** (§Canon strip) · **no ambient connectors and no weave gutter
> dot** (§Ambient weave connectors) · **no hover gloss** (§Hover gloss) · **no
> keyboard map, shortcuts sheet, or per-pane back/forward** (§Keyboard + wheel,
> Tier 0 #2) · **one pane, or two on an opened fold** (§Multi-pane) · **no
> machine-tier artifacts** (below) · **four unhandled link verbs** (below) ·
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
  (An earlier note here said the bridge data was unreachable because
  `OpenFromBytes` has no home; the shell opens from `filesDir` and copies the
  bridge assets in, so that has not been true for some time.)

  This delta used to be four consequences wide. `concept-vectors.vec` was the
  other missing artifact, and the three features it fed — SIMILAR CONCEPTS,
  "verses like this", and the concept map's embedding spokes — were all empty or
  thinned on Android. All three were removed on 2026-07-30, so the gap they
  described is gone with them.

- **On-device feedback round 3 (2026-07-24/25, v0.4.0–v0.5.0).** Landed
  Android-first from on-device street-use feedback; the web has since matched all
  of it except where marked:
  - **Present mode** (`ui/Present.kt`, #1 priority): a thread as a fullscreen,
    high-contrast ("sunlight") large-type presentation for showing someone in
    person. **The thread picker follows the app theme** (2026-07-26 — it's the
    owner's screen; only the presentation itself stays fixed-light), and a
    destination tap always dismisses the fullscreen maps (chord map ate the
    Explore button before). **Re-warmed 2026-07-26 (both shells):** the sunlight palette now
    sits on the app's warm paper (#FCF9F4, warm rules, gold accents, picker
    cards, a ✦ on the end card) instead of stark white — still fixed-light
    and daylight-readable. EB Garamond now actually ships in the APK
    (`assets/fonts/`, the web's variable-weight files; the reader's bold is a
    `wght 700` instance) — Present had been silently falling back to Roboto.
    Present also accepts a **preselected thread** (`presentThreadName` /
    `presentThread`) so first-run "Sharing the gospel" opens the Romans Road
    directly. Original description follows: a thread as a fullscreen
    presentation for showing someone in
    person — scrollable overview (bounce anywhere), tap-to-focus a passage
    huge, "In context" fades surrounding verses in, end card with plain-text
    Share. The share's closing line carries the hosted PWA link and the end
    card shows its QR (2026-07-25, both shells; the BibleGateway link was
    dropped the same day — the verse text is inlined, and the take-home hands
    the recipient the app, not just the text). **Delta:** the "In context" fade
    is Android's alone; the web's Present has no equivalent.
  - **Sharing a passage is a QR, not the share sheet** (2026-07-27, both
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
  - **Embedded study maps** — **REMOVED 2026-07-30, both shells** (`ui/StudyMaps.kt`
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
    · Present · Memorize, one-handed reach; `ui/NavIcons.kt` — **ported to the
    web's narrow layout 2026-07-28**, same icon paths, so it is a phone idiom in
    both shells rather than an Android one), the **passage
    navigator** (`ui/BookNav.kt`: OT/NT → book → chapter tap grids, replacing
    the book dropdown — **both shells**, and there is no verse stage since
    2026-07-26: sizing that grid meant probing the engine for the chapter's
    verse count, so every chapter tap waited on a round trip. Verse targeting
    still arrives via links, cross-references and search, and ReaderPane
    scrolls the target verse into view), the near-fullscreen-expandable study
    sheet, and the reader
    whitespace fix (manifest MARGIN/MAX_COLUMN are logical units — density-
    scaled on Android).
- **What the Compose shell paints (all of it, as of 2026-07-29).** The reader
  (`ui/ReaderPane.kt`) + word study + search + the fold layouts, plus:
  **memorization** (review drill · coverage · activity — `ui/Memorize.kt`); the
  **constellation / chord** maps as pinch-zoom/pan canvases
  (`ui/Maps.kt`); the **whole panel
  content-model** — every `*_blocks_json` / `*_blocks2_json` payload walked by
  `ui/StudyPane.kt` into `AnnotatedString` runs with the palette colour-role map,
  which is how the **RENDERINGS tier and the authority-tier marks (✝ † ≈ ⚗) and
  legend** arrive without Android owning any tier code; **link routing** through
  `plumbline_route_link_json` for 15 of the 19 verbs; the **verse-action sheet**
  (`ui/VerseActions.kt`: copy · copy chapter · share · tag · note · thread ·
  memorize · mark-chapter-read, plus the tag and thread pickers and
  `PassageEndPicker`); **Explore** (notes · threads · tags · weaves+suggested ·
  constellation · chord); **Present** (`ui/Present.kt`); the **passage navigator**
  (`ui/BookNav.kt`) tinted by the reading map; **first run** (`ui/FirstRun.kt`) and
  **church** (`ui/Church.kt`); **backup/restore** (`ui/Backup.kt`); the four themes
  off `plumbline_theme_palette_json` (`ui/Palette.kt`); the two analysis gates in
  Settings; `WarmIndexes` on a coroutine at startup.

  **Live authoring gap:** `editThreadNotes` / `editWeaveNotes` / `editEntryNote` /
  `untag` are the four verbs `StudyScreen.onLink` does not handle — they need an
  index→name lookup, and the comment saying so is in the code beside the `when`.
  The web routes all nineteen (`study/links.ts`). Also still untested on hardware:
  posture-driven fold-mode switching.
- The Kotlin/JNA binding (`crates/ffi/bindings/kotlin/Plumbline.kt`, package
  `dev.plumbline.core`) is the low-level `PlumblineNative` interface +
  JNA types (`PlumblineLayoutConfig`, `MeasureCallback`) **only**; the single
  PascalCase wrapper is `app/.../StudyEngine.kt`. It covers the whole C ABI, and
  it cannot fall behind: `plumbline-bindgen`'s `verify_surface` compares it symbol
  for symbol against the generated header and CI fails on any difference. (The C#
  sibling `bindings/csharp/PureStudy.cs` went with the WinUI shell —
  `crates/ffi/bindings/` holds `c/` and `kotlin/` now.) The native lib
  cross-builds with cargo-ndk into `jniLibs/{arm64-v8a,x86_64}/libplumbline_ffi.so`
  (NDK r29, `--platform 26`), verified independently of the emulator/SDK.
- Build gate: Android NDK + `cargo-ndk` for the `.so` per ABI; the Rust and
  the JSON contract are identical.
- Measure callback: back it with `android.graphics.Paint.measureText` (or
  Compose's TextMeasurer); the core does the rest.
- **v1 phone shell (2026-07-24) — form-factor UX from on-device
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
  format (2026-07-29), and the same three sliders in `SettingsDialog.svelte`.
  Requires the `material-icons-core` dependency.
- **v1 phone shell, round 2 (2026-07-24).** Overflow menu cut to
  five entries — Memorize / Explore / History / Guide & About / Settings — so it
  never scrolls; the fold's second-pane flip is a top-bar icon, not a menu item.
  **Memorize** is a hub (a list of every card from `MemoryCoverageJson`, canon-
  sorted, + Review due / Coverage / Activity buttons). **Explore** is a described
  card list (Notes, Threads, Tags, Weaves, Constellation, Chord) so the tools
  aren't cryptic; **Weaves** is one screen with an All/Suggested filter (was two
  items — the web still lists Suggested separately).
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

- **Passage memorization + the church on Android (2026-07-27, both shells).**
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
  **Church parity — the Compose shell caught up:** `ui/Church.kt` is the twin of
  `shell/church.ts` (same readable `?church=…&churchInfo=…&churchUrl=…` link),
  Settings gained **Your church** + "Present shares as a new believer", first run
  gained the **Curious about the Bible** path and asks for the church on the two
  paths that hand the app on, the welcome a reader was given is remembered
  (`intro`) and re-readable, and Present's take-home QR + share text carry the
  reader's link instead of a bare URL. The QR is generated at render time
  (zxing-core, UTF-8 byte mode) — it was a build-time constant matrix for one
  fixed URL, which cannot carry a church. **Delta (Android):** a reader SETS and
  SHARES a church but never RECEIVES one — a plumblinebible.org link opens the
  PWA, so `App.svelte`'s incoming-church capture has no Compose counterpart; and
  Church/Welcome are overflow (⋮) items rather than top-bar buttons, because the
  phone top bar is deliberately tight.
- **Proper nouns are not concepts (2026-07-27, engine-wide).** The concept card's
  collocate/community lists drop proper
  nouns (`strongs::is_proper_noun`) — "faith" sitting next to Ephraim, Jerusalem
  and Shechem read as noise. Names stay fully reachable in word
  study, concordance and search; `CONCEPT_KEEP_NAMES` keeps the divine name and
  Christ, which in this corpus are concepts rather than incidental names.
  Candidates are over-fetched before filtering so a list never comes back short.
  (This rule also governed the concept map's neighbour rings, which went on
  2026-07-30.)
- **A cold read explains itself (2026-07-27, both shells).** The first
  definition of a session builds the occurrence index and the first analytical
  map sweeps the corpus; a bare flash of "loading" (or blank paper) for seconds
  reads as a hang. Once a read outlasts ~600 ms, both shells add a still,
  non-pulsing note that the wait is **one-time** and the rest are instant
  (`StudyPanel.svelte`/`MapFrame.svelte`; `StudyPane.kt`/`Maps.kt`). Timed
  rather than flagged: whatever index is cold, the wait itself is the signal.
