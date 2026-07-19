# Shell feature manifest — the parity contract

The canonical inventory of everything a pure-study shell does, written so a new
shell (WinUI, Compose/Android) can be built **without re-surveying the repo**.
The GTK shell (`apps/desktop/src/main.rs`, one file) is the reference
implementation; line refs below point there (as of branch `windows-arm64`,
2026-07-14). Non-GTK shells reach everything through the C ABI (`crates/ffi`);
the *Data* line under each feature names the endpoint(s).

Conventions used everywhere:

- **refKey** `"Gen 1:7"` (OSIS id + `ch:v`) is the wire form of a verse ref;
  `display` is the human form. `reading_key()` orders refs canonically.
- Token flag bits: `1` added (italic), `2` divine name, `4` title, `8` ¶.
- Timestamps are UTC `%Y-%m-%dT%H:%M:%SZ` (`now_stamp`, M:2258).
- Authoring endpoints return **null on success, else an error string**; after
  any write the engine reloads study data from disk — re-fetch, never mutate
  shell-side state.

## Constants + styling (M:41–54, 3807–3823)

| name | value | meaning |
|---|---|---|
| MAX_COLUMN | 720 px | text column cap; centre in wider panes |
| MARGIN | 28 px | text margin, all sides |
| MIN/MAX/DEFAULT_FONT | 12 / 48 / 18 pt | zoom range, 1-pt steps |
| MAX_PANES | 3 | reading columns |
| PANEL_WIDTH | 380 px | study sidebar (fixed width) |
| OCC_SHOWN | 300 | concordance cap |
| XREF_SHOWN | 40 | xref/link list caps |
| GLOSS_SAMPLE | 80 | verses sampled for the english gloss |
| LINK_INSET / YINSET | 14 / 5 px | connector gutter inset / clamp margin |

Palette: paper `#fcf9f4`; body `rgb(0.13,0.12,0.10)`; gold accent
`rgb(0.62,0.49,0.22)` ≈ `#9e7d38`; added-word gray `#6b6862`; divine
`rgb(0.30,0.20,0.15)`; popup paper `rgb(0.949,0.933,0.902)`; pane-nav bg
`#efeae1`; section-header gold `#a0894a`. Font: bundled EB Garamond
(`apps/desktop/assets/fonts/`), forced light theme. Window default 1100×780.

## App / window icon

The woven-cross icon (`apps/desktop/assets/icons/pure-study.svg` + PNGs, shared
by both desktop shells). Each shell wires it to the window/taskbar:
- **GTK** — `install_app_icon` (M:4078, called after `install_css`) adds
  `assets/icons` to the display's `IconTheme` search path and calls
  `Window::set_default_icon_name(APP_ID)`. The icon is installed under the app
  id as a scalable SVG: `assets/icons/hicolor/scalable/apps/
  dev.purestudy.app.svg`. Compile-time manifest path (like the bundled fonts)
  → CI-validated, not run on the ARM64 box.
- **WinUI** — the multi-res `pure-study.ico` (window + taskbar).
- **Compose** — pending (see Android notes).

## Reader core

- **Layout is in the Rust core** — the shell provides a text-measure callback
  and paints the returned display list (`verseNumber` + `word` items with
  x/y/w/h, flags, strongs). Config the GTK shell passes (M:2660–2815):
  `width = min(paneW−2·28, 720)`, `line_height = (ascent+descent)·1.35`,
  `space_width` measured, `verse_num_gap = space·1.4`,
  `para_indent = line_height·0.9`, `para_spacing = line_height·0.45`.
- Paint: verse numbers **bold gold**; FLAG_ADDED italic gray; FLAG_DIVINE /
  FLAG_TITLE colors above; Strong's-tagged words underlined gold α0.30 width 1
  at baseline+2.5. Hit-testing: `hit_test(x − margin_x, y − MARGIN)`.
- **Highlight band** (search/goto target): gold α0.12 rect over the verse's
  lines, x from `margin−6`, width `col+12`; persists until that pane next
  navigates (M:2720–2740).
- *Data*: `pure_engine_layout_chapter` (+ `pure_layout_*`), `pure_engine_toc_json`.

## Multi-pane (M:1649–2113)

1–3 columns; each has a nav strip: book dropdown, chapter spin (1..count),
prev/next, **+** (only when n<3; inserts a copy of this pane after itself,
becomes active), **✕** (only when n>1). **Active pane** = last touched (canvas
click or nav interaction); gets a 2-px gold top border *only when >1 pane*;
window subtitle "`{book} {ch}` · 1769 KJV". Search go-to, panel links, canon
strip, chord/constellation clicks all target the active pane. Navigation with
a verse polls (~8 ms × 120) until the fresh layout paints, then scrolls the
verse to y−8. `step_pane` clamps chapter into 1..count (no cross-book step;
header ‹› in WinUI added cross-book — GTK does not).

## Ambient weave connectors (M:2821–2934)

Transparent overlay above the pane row, input-transparent, redrawn on scroll /
navigate / zoom / rebuild (60 ms delay) / authoring. The deduped canonical
link pairs come from the **core view-model** `pure_engine_link_pairs_json`
(each endpoint located + a `resolved` flag) — no shell re-derives the dedup:
GTK calls `pure_core::weave::link_pairs` directly; WinUI parses the endpoint
(filtering `resolved`), Compose consumes the same JSON. For each pair: map both
endpoints' `(book, chapter)` to showing panes
(later pane wins duplicates); skip unless both shown in *different* panes.
Endpoint y = the verse-number item's `MARGIN + y + h/2` in pane space →
overlay coords; endpoint x = left pane's right edge − 14 / right pane's left
edge + 14 (connectors ride the gutter). Clamp y into the pane's visible band
±5 so an off-screen end lingers as an edge dot. Draw: cubic Bézier, ctrl pts
`(x1+dx·0.4, y1)`/`(x2−dx·0.4, y2)`, stroke gold α0.35 width 1.5; dots r2
gold α0.7. **In-pane gutter dots**: any verse with weave partners gets a gold
dot α0.75 r2.3 at `x=margin−9`, next to the verse number (M:2712).
*Data*: `pure_engine_weaves_json` (client folds into pair/xref indexes).

## Hover gloss (M:1744, 3582)

Native tooltip timing; hit-test under pointer; only when the word has Strong's
refs. Per code: bold code, lemma, italic xlit, then `kjv` (fallback `def`)
trimmed to 80 chars. *Data*: `pure_layout_hit_test_json` + `pure_engine_strongs_json`.

## Word study panel (double-click **or Ctrl+click**; M:3168–3515)

Sidebar 380 px, on-demand; Esc hides; clearing search hides. Content order —
(F) = Full mode only.

**Structure — one core producer, thin per-block renderers (P0.1).** The whole
panel is now a **typed block list** built once in `pure_core::panel` and served
over `pure_engine_word_study_blocks_json` (+ the sibling `*_blocks_json`
endpoints below). A block is `Section{title, mark?}` / `Para{runs, indent,
topGap}` / `Rule`; a run carries text + a **semantic colour role** + a logical
point size + bold/italic + an optional `uri`. The producer owns *everything*
below — tier order, caps, (F)-gating, `humanize`, `RenderKey`, gloss/lemma
formatting, the reverse-lens line, the provenance marks, snippet windowing, and
the pre-baked link URIs. Each shell has a *small* per-block painter
(`RenderBlocks` / `blocks_to_markup`); it derives nothing. The `word` (the
surface that led here — highlights its rendering, keys the reverse line) is a
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
     `pure_engine_renderings_json` + `pure_engine_word_codes_json`.
   - **SAME ROOT ACROSS TESTAMENTS** *(per-chip marks)* — bridge partners (≤6)
     as gloss chips → concordance links; sources humanized (`bridge::source_label`
     / WinUI `Humanize`: `lxx`→Septuagint, `quotation`→NT quotation, `abbott-smith`→
     Abbott-Smith (1922), …); then this chip's provenance marks from the union of
     its sources' tiers (✝/†/≈, + ⚗ if any source is research-grade); "· disputed
     by usage" in `#b04a3a` when the text-witness disbelieves (shipped data never
     grades, so silent).
   - **SIMILAR CONCEPTS** *(Machine ≈)* — embedding neighbours (6); "across the
     testaments —" cross (6).
   - **APPEARS ALONGSIDE** *(Machine ≈)* — concept community (8).
   - **WHERE IT CONCENTRATES** *(Machine ≈)* — top books (5) "Book ×N · …" + "(OT x · NT y)".
   - **LEITWORT** *(Machine ≈)* — "{winCount} of its {n} uses cluster in {label} (p ≈ 10^−{score})".
   - "▸ open concept map" link.
4. (F) Author actions: `＋ tag verse`, `＋ add to thread`.
5. **cross-references (N)** — weave partners (≤40), each + weave-name link to
   its compare card.
6. (F) **study cross-references (N) — TSK** *(Human †)* (≤40; ranges "a–b").
7. (F) **verses like this** *(Machine ≈)* — SIF in-testament (6); cross (4).
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
The `PanelSource` trait (implemented by both the GTK `State` and the FFI
`PureEngine`) is the producer's only input — a thin set of projected accessors
(`strongs`/`occurrences`/`renderings`/`bridge_partners`/`concept`/
`similar_verses`/`verse_xrefs`/`verse_notes`/…), so the same producer runs
Rust→Rust for GTK and behind the endpoints for WinUI/Compose.

## Authority tiers — provenance marks on evidence

Ported from overlay `Bridge.hs` `Tier` + `Panels.hs` `provIcon`/`tierMarks`.
Every piece of study evidence shows where it comes from, so the reader always
knows its provenance. The model is `pure_rnd::bridge` (`crates/rnd/src/bridge.rs`):

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
  Machine `≈` gray `#999`; research-grade `⚗` red `#b04a3a`. A GtkLabel can't
  embed images (overlay draws PNGs via Monomer), so GTK uses overlay's
  *glyph-fallback* set styled with `<span foreground>`; WinUI uses colored
  `Run`s. **Per-chip** on SAME ROOT partners (real per-source provenance);
  **per-section** on the headers above; a **legend** at the foot. Human-baseline
  blocks (the dictionary entry) and user data (weaves, tags) are unmarked.
- **Wire**: `pure_engine_bridge_partners_json` gained additive `tiers`
  (`["god","human","machine"]`) + `researchGrade` per partner, so non-Rust
  shells consume the classification instead of reimplementing it. GTK, being
  Rust, calls `bridge::tiers_of`/`research_grade` directly. Fixed-by-block
  sections (SIMILAR CONCEPTS = Machine, TSK = Human, …) are marked shell-side.

## Link routing — one verb vocabulary (P1.4)

All panel interactivity funnels through one URI dispatcher, and the verb
vocabulary is **parsed once in the core**: `pure_core::panel::parse_link(uri) ->
PanelLink` — co-located with the producers that *emit* the URIs, so a verb can't
drift between what the panel bakes and what a shell handles. GTK matches on
`PanelLink` directly; WinUI/Compose route through `pure_route_link_json(uri)`
(`{verb, …}`, tagged) — neither re-splits the string. The 16 verbs:
`go:Book:ch[:v]` · `occ:CODE` · `rend:CODE:rendering` · `code:CODE[:word]` ·
`thread:i` · `tag:i` · `weave:i` · `conceptmap:CODE` · `addtag:refkey` ·
`addthread:refkey` · `untag:i:refkey` · `approve:i` · `reject:i` ·
`editthreadnotes:i` · `editentrynote:ti:ei` · `editweavenotes:i`.

Navigation + native prompts + the write choreography (author endpoint →
reload → refetch) stay shell-side. `parse_link` handles multi-word books
("1 John") and colon-bearing refkeys; `code:CODE[:word]` keeps its `word`
(a surface token) and opens the standalone code-study card — distinct from
`occ:CODE` (verse list) and `rend:CODE:rendering` (filtered list).

## Concordance (`occ:`; M:3783)

Code + lemma large + count; verse links capped at 300, "… N more".
*Data*: `pure_engine_strongs_occurrences_json` (cap 500 engine-side).

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
reject deletes the file. Ordinals shift after every write — always re-fetch.
*Data*: `suggested_weaves_json`, `weave_approve/reject(index)`.

## Weave authoring (M:2137–2236)

Single left-click a word → pin `PinSpan{verse, anchor, lo, hi}` in that pane:
same-verse clicks re-span `lo=min(anchor,tok), hi=max(anchor,tok)`; a
different verse resets. Pinned span painted blue α0.22 per word rect. Header
`＋ link` enabled when ≥2 panes have pins; takes the first two, prompts a
weave name, writes a Quotation-kind link with both spans (canon-ordered), then
clears pins + redraws connectors. **Compare card** (`weave:i`): name + kind +
"(suggested)"; "N link(s)" + (F) `✎ note`; per link ≤40: label `"…"` gold,
each side verse link + verse text small with **span words bold** and added
words italic gray. *Data*: `pure_engine_weave_add_link_spans`, `weaves_json`,
`verse_json` (tokens for span rendering).

## Canon strip (M:2938–2989)

30-px strip under the panes. 8 sections (Law 0–4, History 5–16, Wisdom 17–21,
Prophets 22–38, Gospels 39–42, Acts 43, Letters 44–64, Revelation 65), odd
sections shaded black α0.04, centred 11-px labels when they fit; OT/NT divide
line at index 39. Pin per pane at `x=(order+0.5)/66·w` (active gold, others
gray). Click: `idx = x/w·66` → active pane to that book ch 1.

The segments + divide are the **single source** `core::reference::CANON_SEGMENTS`
/ `OT_NT_DIVIDE`, served over the wire by `pure_engine_canon_segments_json`.
GTK reads the const directly; WinUI loads the endpoint once into a shared
`Canon` holder that both the strip and the map popups (chord / constellation)
read — no shell hardcodes the bands anymore (the WinUI copies had drifted).

## Search (M:660, 3739)

Live per keystroke; empty query closes the panel. `goto` answer → big "go to"
link (navigates active pane; verse target gets the band). `hits` → "N
result(s)" + tier phrase small; per hit: verse link, gray `why`, "※ note"
marker for margin-note matches; "… N more" past cap. *Data*: `search_json`.

## Keyboard + wheel (M:1806–1875)

Up/Down ±line (`font·3` px); PageUp / PageDown|Space ±85% page (**Shift** =
all panes lockstep); Home/End; Right|`]` / Left|`[` next/prev chapter (this
pane); Ctrl+0 zoom reset, Ctrl+± zoom ±1 pt; Esc hides panel / closes popups.
Wheel scrolls hovered pane; Ctrl+wheel zooms; Shift+wheel scrolls all panes.
Zoom **persists config on every change** (M:2117).

## Config / session (`core::config`)

`%APPDATA%\pure-study\config.json` (XDG / App Support elsewhere):
`{"studyMode":"simple"|"full","bodySize":18.0,"openPanes":[{"book","chapter"}],
"activePane":0}`. `first_run` only when the file is absent; corrupt file →
defaults, no re-prompt. Restore panes (≤3; default John 3) + active + zoom at
startup; persist on close, mode toggle, first-run pick, every zoom. Scroll
position intentionally transient. *Data*: `pure_config_load_json` /
`pure_config_save_json` (shared file with GTK — keep the shape).

## Simple/Full mode (M:462–488, 1564–1644)

First run: modal "Welcome to pure-study" with two cards — "Simple reader"
("Just the text…") / "Full study" ("Everything…"); closing without choosing
keeps Simple. Header toggle button shows the *current* mode; leaving Full
collapses the panel. Simple hides the header study tools (Threads, Tags,
Suggested, Map, Constellation, ＋link) and every (F) item above; Simple keeps
reading, search, hover gloss, basic word study (Strong's + occurrences +
weave xrefs + margin notes), canon strip, connectors, zoom.

## Chord/arc "Map" popup (M:887–935, 2994–3087)

1000×360, Esc/click-outside closes. **The book-pair fold lives in the core
view-model** `pure_engine_chord_map_json` → `{pairs:[{a,b,count}] (canon book
indices, a≤b), max, otNtDivide, bookCount}` (GTK calls `pure_core::weave::chord_pairs`
directly). The shell only paints: canon axis with section bands + labels (from
the `Canon` holder), gold baseline, OT/NT seam; ribbons heaviest-first, alpha
`0.12+0.30·(cnt/max)`, foot width `2+8·(cnt/max)`; colours OT `(0.82,0.70,0.43)`
/ NT `(0.50,0.70,0.90)` / cross `(0.78,0.59,0.86)` (+0.08 α, cap 0.5); apex
`min(0.42·h, 22+0.26·h·|dx|/w)`; self-pair = small loop. Click: x→book →
navigate active pane + close.
*Parity fix:* the map counts every deduped pair (resolved or not); WinUI
previously folded only the resolved connector links — now unified to the GTK
reference.

## Constellation popup (M:937–1529)

1200×640; ‹prev/next› + caption; Esc/click-outside closes; Left/Right page.
**The whole layout is the core view-model** `pure_engine_constellation_json(page,
pins_json)` (pins = a JSON array of weave indices) → lanes of nodes + edges as
**fractions** (`x` a canon fraction, `laneFrac` 0..1 within a lane, `size` a
0..1 witness degree) plus `nPins/freeTotal/page/maxPage/caption/laneCapacity`;
GTK calls `pure_core::weave::constellation` directly. Usable = weaves with ≥1
resolvable link, largest-first; `laneCapacity` (18) lanes, pinned (by weave
**index**) first. The shell maps fractions to pixels + paints: `laneH =
(h−topPad−10)/laneCapacity`, node `(plotLeft + x·(w−plotLeft), topPad +
(lane+laneFrac)·laneH)` with plotLeft 162 / topPad 18 / gutter 150; 7-colour
cycle ×0.72; node square half-size `1.4+2.4·size`; pin gutter x<150 (filled gold
8×8 pinned / hollow gray); lane name ≤22 chars; canon ruler + OT/NT seam; hover
tooltip "verse · weave". Hit priority **node > edge > pin-gutter**; node →
navigate (stays open); edge → compare card (closes); gutter → toggle pin. The
caption comes from the model.
*Parity fixes* folded into the one model: node size normalises by the **global**
max degree (GTK was per-page), and both shells now share one lane-height metric
and one caption (they had drifted on all three).

## Concept map popup (`conceptmap:`; M:724–883)

720×560: radial graph + 40-px dispersion strip. **The whole popup is the core
view-model** `pure_engine_concept_map_json(code)` → `{code, centerLabel,
spokes:[{code, label, semantic}], byBook (canon-ordered counts), otNtDivide,
bookCount}`. The spoke union (embedding-near ∪ community, deduped, 6 each) lives
in `pure_rnd::concept::radial_spokes` (GTK calls it directly); labels
("gloss\nlemma") are baked by the endpoint. Paint only: radius `min(w,h)/2−95`;
semantic spokes gold, community green; centre node gold; dispersion cells gold α
`0.15+0.75·(cnt/max)` at `bi/bookCount`, OT/NT seam. No shell book-order table.

## C ABI surface (crates/ffi) — endpoint ↔ feature map

Pre-existing: `open`/`open_from_bytes`/`free`, `toc_json`, `chapter_count`,
`verse_json`, `token_json`, `layout_chapter` + `layout_*` + `hit_test`,
`strongs_json`, `strongs_occurrences_json`, `search_json`, `threads_json`,
`tags_json`, `verse_xrefs_json`, `suggested_weaves_json`, authoring
(`thread_add`, `tag_add`, `tag_remove`, `weave_add_link`, `weave_approve`,
`weave_reject`, `thread_set_notes`, `thread_entry_set_note`,
`weave_set_notes`), R&D (`concept_neighbours_json`, `bridge_partners_json`,
`morph_json`, `similar_verses_json`).

Added for shell parity (2026-07-14):

| endpoint | returns | for |
|---|---|---|
| `pure_engine_verse_notes_json(ref)` | `{verse, notes[]}` or null | margin notes |
| `pure_engine_study_xrefs_json(ref)` | `{verse, refs:[{to, toDisplay, end?, votes}]}` | TSK tier |
| `pure_engine_weaves_json()` | full library: weaves + links incl. `approved`, `spanA/B`, `resolved`, `suggested` | compare card, weaves list, panel xrefs (chord map + constellation now have their own view-model endpoints) |
| `pure_engine_concept_json(code)` | `{total, ot, nt, topBooks, byBook, collocates, community, leitwort?}` | ALONGSIDE / CONCENTRATES / LEITWORT / dispersion |
| `pure_engine_gloss(code)` | plain english gloss or null | concept chips |
| `pure_engine_weave_add_link_spans(name, a, b, aLo, aHi, bLo, bHi, added)` | null/error (negative span = none) | word-span links |
| `pure_config_load_json()` / `pure_config_save_json(json)` | config wire above (+`firstRun` on load) | session/mode/zoom |

Added for the rendering lens (2026-07-16):

| endpoint | returns | for |
|---|---|---|
| `pure_engine_renderings_json(code)` | `{code, renderings:[{rendering, total, capped, refs:[{verse, display, span:[s,e]}]}]}` (refs cap 500) | RENDERINGS tier + filtered concordance |
| `pure_engine_word_codes_json(word)` | `{word, codes:[{code, count}]}` | "also translates" reverse line |

Extended for authority tiers (2026-07-16): `pure_engine_bridge_partners_json`
partners gained **additive** fields `tiers` (`["god"\|"human"\|"machine"]`,
deduped, ordered God→Human→Machine) and `researchGrade` (bool). Existing
`code`/`sources`/`prior` unchanged; a consumer that ignores the new fields sees
the pre-tier behaviour. No extern-surface change → bindings unchanged.

Added for the view-model consolidation (2026-07-16, architecture-review P0.3 —
the warm-up that moves shared derivation out of the shells into the core):

| endpoint | returns | for |
|---|---|---|
| `pure_engine_link_pairs_json()` | `{pairs:[{a, aBook, aChapter, aVerse, b, bBook, bChapter, bVerse, resolved}]}` | ambient connectors + chord map (retires the shell dedup) |
| `pure_engine_canon_segments_json()` | `{segments:[{label, first, last}], otNtDivide}` | canon strip + map ruler bands (retires the WinUI hardcode) |

Both are thin wrappers over the one core source: `link_pairs` wraps
`pure_core::weave::link_pairs`; `canon_segments` wraps
`core::reference::CANON_SEGMENTS` / `OT_NT_DIVIDE`. GTK (being Rust) calls those
directly rather than round-tripping JSON.

Added for the popup view-models (2026-07-18, architecture-review P0.2 — the
three map popups' derivation moved into the core; positions cross the wire as
**fractions/logical units**, never pixels/colours):

| endpoint | returns | for |
|---|---|---|
| `pure_engine_chord_map_json()` | `{pairs:[{a, b, count}] (canon book indices, a≤b), max, otNtDivide, bookCount}` | chord/arc "Weave map" (retires the shell fold + max) |
| `pure_engine_concept_map_json(code)` | `{code, centerLabel, spokes:[{code, label, semantic}], byBook[] (canon order), otNtDivide, bookCount}` | concept map (retires the spoke assembly + gloss/lemma lookups + book table) |
| `pure_engine_constellation_json(page, pins_json)` | `{lanes:[{weaveIndex, name, pinned, nodes:[{x, laneFrac, size, refKey, book, chapter, verse, display}], edges:[{aX, aLaneFrac, bX, bLaneFrac}]}], nPins, freeTotal, page, maxPage, caption, laneCapacity}` (pins = JSON array of weave indices) | constellation (retires the usable/degree/jitter/paging/pin derivation) |

Producers: `chord_map` wraps `pure_core::weave::chord_pairs`; `constellation`
wraps `pure_core::weave::constellation`; `concept_map` bakes labels over
`pure_rnd::concept::radial_spokes` + `concept.stat`. GTK calls the core fns
directly; the non-Rust shells consume the JSON and map fractions → pixels.

Added for the panel content-model + link router (2026-07-18, P0.1 + P1.4 — the
whole study panel and its verb vocabulary move into the core). Every block
endpoint returns `{blocks:[Section|Para|Rule]}` (runs carry a semantic colour
role + logical size + optional uri); `full` gates the R&D tiers + author actions.

| endpoint | returns / for |
|---|---|
| `pure_engine_word_study_blocks_json(ref, tok, full)` | the tapped word's dictionary + Full tiers + this verse's xrefs/notes |
| `pure_engine_code_study_blocks_json(code, word?, full)` | the standalone `code:` card |
| `pure_engine_concordance_blocks_json(code)` / `pure_engine_rendering_concordance_blocks_json(code, rendering)` | full / rendering-filtered concordance |
| `pure_engine_threads_blocks_json()` / `pure_engine_thread_blocks_json(i)` | threads list / detail |
| `pure_engine_tags_blocks_json()` / `pure_engine_tag_blocks_json(i)` | tags list / detail |
| `pure_engine_weaves_blocks_json()` / `pure_engine_suggested_blocks_json()` | weaves list / review queue |
| `pure_engine_compare_blocks_json(i, full)` | weave compare card |
| `pure_engine_search_blocks_json(query)` | search results (goto link or ranked hits + snippets); null on blank |
| `pure_route_link_json(uri)` | parse a panel link into `{verb, …}` (engine-independent) |

One producer (`pure_core::panel`) over the `PanelSource` trait feeds all of
these; GTK implements the trait on `State` and calls the producer directly, the
FFI implements it on `PureEngine`. **Golden coverage (P2.6):** `panel_blocks_via_abi`
and `route_link_via_abi` exercise the block payloads + parser over the ABI, and
the producer itself has 15 unit tests over a fake source; the block kinds are a
Rust enum (a shell that meets an unknown kind renders nothing — forward-compat).

Not ported into any shell (by decision / data): signed patches + rules;
text-witness grading (shipped data never passes, so the "disputed" marker
stays silent); quotation detection (awaits hydrated inputs).

## Android notes

- The Kotlin/JNA wrapper (`crates/ffi/bindings/kotlin/PureStudy.kt`, package
  `dev.purestudy.core`) predates the parity endpoints — extend it like
  `bindings/csharp/PureStudy.cs` (which is current).
- **Rendering lens (2026-07-16) — Compose delta:** the two endpoints
  (`renderingsJson` / `wordCodesJson`) are already in the Kotlin binding, but
  the RENDERINGS tier + the `rend:` and `code:` routes are **not yet in a
  Compose shell**. Build them from the GTK/WinUI reference when the Compose
  shell lands, including the reusable code-study view (`code_study_markup` /
  `CodeStudy`) that both the per-code block and the `code:` verb share.
- **App/window icon — Compose delta:** GTK (`install_app_icon`, scalable SVG at
  `apps/desktop/assets/icons/hicolor/scalable/apps/dev.purestudy.app.svg`) and
  WinUI (`.ico`) both wire the woven cross; Android needs its own launcher
  icon (adaptive-icon / mipmap) generated from `pure-study.svg` — not the
  hicolor tree.
- **Authority tiers (2026-07-16) — Compose delta:** the classification lives in
  Rust (`bridge::source_tiers`/`research_grade`/`tiers_of`) and rides the wire
  (`tiers`/`researchGrade` on bridge partners), so a Compose shell reads it for
  the SAME ROOT marks and hardcodes the fixed-by-block tiers (SIMILAR CONCEPTS =
  Machine, TSK = Human, …) like GTK/WinUI. The mark glyphs (✝ † ≈ ⚗), their
  colors, and the legend are **not yet in a Compose shell** — build from the
  GTK/WinUI reference (see **Authority tiers** above). The Kotlin binding must
  also gain `bridgePartnersJson` (its `PureFfi` interface omits the R&D tier).
- **View-model consolidation (2026-07-16, P0.3) — Compose delta:** the two new
  endpoints (`pure_engine_link_pairs_json` / `pure_engine_canon_segments_json`)
  are **not yet in the Kotlin `PureFfi` interface** (kept minimal like the other
  study-tier endpoints). A Compose shell adds the two JNA decls + wrappers, then
  consumes them for its connectors and canon strip instead of re-deriving —
  exactly what this phase removed from GTK/WinUI.
- **Panel content-model + link router (2026-07-18, P0.1 + P1.4) — Compose
  delta:** the whole study panel is a typed block list from the
  `pure_engine_*_blocks_json` family, and links parse via `pure_route_link_json`
  — none are in the Kotlin interface yet. A Compose shell adds those JNA decls +
  wrappers, then walks the blocks with a small per-block composable
  (`Section`/`Para`/`Rule`, runs → `AnnotatedString` with the colour-role map +
  clickable link URIs) and dispatches clicks on the parsed `{verb}`. It writes
  **no** panel derivation — the producer owns tier order, caps, gloss/lemma,
  snippets, and the verb vocabulary. Colour roles map identically to GTK/WinUI.
- **Popup view-models (2026-07-18, P0.2) — Compose delta:** the three map
  popups now come from `pure_engine_chord_map_json` / `pure_engine_concept_map_json`
  / `pure_engine_constellation_json` (all **not yet in the Kotlin interface**).
  A Compose shell adds the three JNA decls + wrappers and paints the returned
  fractions — it never re-derives the fold / spoke assembly / lane layout.
  Positions are fractions/logical units, so the Compose mapping is the SAME
  `plotLeft + x·(w−plotLeft)` / `topPad + (lane+laneFrac)·laneH` the GTK/WinUI
  reference uses; keep `laneCapacity`, plotLeft 162, topPad 18, gutter 150, and
  the `1.4+2.4·size` radius identical so all three shells place a node alike.
- Build gate: Android NDK + `cargo-ndk` for the `.so` per ABI; the Rust and
  the JSON contract are identical.
- Measure callback: back it with `android.graphics.Paint.measureText` (or
  Compose's TextMeasurer); the core does the rest.
