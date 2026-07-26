# Shell feature manifest — the parity contract

> **2026-07-25 — the shells are Android (Compose, the UX gold standard) and
> the web PWA.** The GTK and WinUI desktop shells were retired and REMOVED
> from the tree (git history has them). Sections below that cite GTK
> `M:<line>` refs or name GTK/WinUI behaviours are kept as the historical
> spec of *what* each feature does — the line refs no longer resolve, and
> "deltas owed to GTK/WinUI" are void. Treat the Android shell as the living
> reference implementation.

The canonical inventory of everything a Plumbline shell does, written so a
shell can be built **without re-surveying the repo**. Historically the GTK
shell was the reference implementation; line refs below (`M:<line>`) pointed
at its `main.rs`. Non-Rust shells reach everything through the C ABI
(`crates/ffi`); the *Data* line under each feature names the endpoint(s).

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
| PANEL_WIDTH | 380 px | study sidebar (web: × the text-size setting — the reader zoom scales the whole study surface, width and type; 2026-07-25) |
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

The woven-cross icon (`apps/desktop/assets/icons/plumbline.svg` + PNGs, shared
by both desktop shells). Each shell wires it to the window/taskbar:
- **GTK** — `install_app_icon` (M:4078, called after `install_css`) adds
  `assets/icons` to the display's `IconTheme` search path and calls
  `Window::set_default_icon_name(APP_ID)`. The icon is installed under the app
  id as a scalable SVG: `assets/icons/hicolor/scalable/apps/
  dev.plumbline.app.svg`. Compile-time manifest path (like the bundled fonts)
  → CI-validated, never exercised on the dev machine.
- **WinUI** — the multi-res `plumbline.ico` (window + taskbar).
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
- *Data*: `plumbline_engine_layout_chapter` (+ `plumbline_layout_*`), `plumbline_engine_toc_json`.

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
link pairs come from the **core view-model** `plumbline_engine_link_pairs_json`
(each endpoint located + a `resolved` flag) — no shell re-derives the dedup:
GTK calls `plumbline_core::weave::link_pairs` directly; WinUI parses the endpoint
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
*Data*: `plumbline_engine_weaves_json` (client folds into pair/xref indexes).

## Hover gloss (M:1744, 3582)

Native tooltip timing; hit-test under pointer; only when the word has Strong's
refs. Per code: bold code, lemma, italic xlit, then `kjv` (fallback `def`)
trimmed to 80 chars. *Data*: `plumbline_layout_hit_test_json` + `plumbline_engine_strongs_json`.

## Word study panel (click a word — 2026-07-25, was double-click; M:3168–3515)

Sidebar 380 px (web: scaled by the text-size setting), on-demand; Esc hides;
clearing search hides. Content order — (F) = Full mode only.

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
     `plumbline_engine_renderings_json` + `plumbline_engine_word_codes_json`.
   - **SAME ROOT ACROSS TESTAMENTS** *(per-chip marks)* — bridge partners (≤6)
     as gloss chips → concordance links; sources humanized (`bridge::source_label`
     / WinUI `Humanize`: `lxx`→Septuagint, `quotation`→NT quotation, `abbott-smith`→
     Abbott-Smith (1922), …); then this chip's provenance marks from the union of
     its sources' tiers (✝/†/≈, + ⚗ if any source is research-grade); "· disputed
     by usage" in `#b04a3a` when the text-witness disbelieves (shipped data never
     grades, so silent).
   - **SIMILAR CONCEPTS** *(Machine ≈)* — embedding neighbours (6); "across the
     testaments —" cross (6). Grammatical function words (articles,
     conjunctions, prepositions, pronouns, be-verbs — `rnd::stopwords`) never
     appear as neighbours (2026-07-25: *believe* was offering *because*);
     they remain directly studyable.
   - **APPEARS ALONGSIDE** *(Machine ≈)* — concept community (8), same
     function-word filter.
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
`PlumblineEngine`) is the producer's only input — a thin set of projected accessors
(`strongs`/`occurrences`/`renderings`/`bridge_partners`/`concept`/
`similar_verses`/`verse_xrefs`/`verse_notes`/…), so the same producer runs
Rust→Rust for GTK and behind the endpoints for WinUI/Compose.

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
  Machine `≈` gray `#999`; research-grade `⚗` red `#b04a3a`. A GtkLabel can't
  embed images (overlay draws PNGs via Monomer), so GTK uses overlay's
  *glyph-fallback* set styled with `<span foreground>`; WinUI uses colored
  `Run`s. **Per-chip** on SAME ROOT partners (real per-source provenance);
  **per-section** on the headers above; a **legend** at the foot. Human-baseline
  blocks (the dictionary entry) and user data (weaves, tags) are unmarked.
- **Wire**: `plumbline_engine_bridge_partners_json` gained additive `tiers`
  (`["god","human","machine"]`) + `researchGrade` per partner, so non-Rust
  shells consume the classification instead of reimplementing it. GTK, being
  Rust, calls `bridge::tiers_of`/`research_grade` directly. Fixed-by-block
  sections (SIMILAR CONCEPTS = Machine, TSK = Human, …) are marked shell-side.

## Link routing — one verb vocabulary (P1.4)

All panel interactivity funnels through one URI dispatcher, and the verb
vocabulary is **parsed once in the core**: `plumbline_core::panel::parse_link(uri) ->
PanelLink` — co-located with the producers that *emit* the URIs, so a verb can't
drift between what the panel bakes and what a shell handles. GTK matches on
`PanelLink` directly; WinUI/Compose route through `plumbline_route_link_json(uri)`
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
*Data*: `plumbline_engine_strongs_occurrences_json` (cap 500 engine-side).

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

## Canon strip (M:2938–2989)

30-px strip under the panes. 8 sections (Law 0–4, History 5–16, Wisdom 17–21,
Prophets 22–38, Gospels 39–42, Acts 43, Letters 44–64, Revelation 65), odd
sections shaded black α0.04, centred 11-px labels when they fit; OT/NT divide
line at index 39. Pin per pane at `x=(order+0.5)/66·w` (active gold, others
gray). Click: `idx = x/w·66` → active pane to that book ch 1.

The segments + divide are the **single source** `core::reference::CANON_SEGMENTS`
/ `OT_NT_DIVIDE`, served over the wire by `plumbline_engine_canon_segments_json`.
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

`%APPDATA%\plumbline\config.json` (XDG / App Support elsewhere):
`{"studyMode":"simple"|"full","bodySize":18.0,"openPanes":[{"book","chapter"}],
"activePane":0,"versePerLine":false,"theme":"system","copyStyle":"verseRef",
"sideMargin":28.0,"lineSpacing":1.35,"history":[{"book":"John","chapter":3}]}`.
All additive (default on absence); a save must round-trip fields it doesn't
expose (each shell carries them forward). `copyStyle`
(`verse`|`verseRef`|`verseMarkdown`) is the one-tap copy shape; `sideMargin`
(px, 0–160) + `lineSpacing` (×text-height, 1–3) are reader spacing. `history`
is recent (book, chapter), most-recent-first, deduped, core-capped at 50
(`config::HISTORY_CAP`) — powers "start where I left off" + a history list.
`first_run` only when the file is absent; corrupt file →
defaults, no re-prompt. Restore panes (≤3; default John 3) + active + zoom at
startup; persist on close, mode toggle, first-run pick, every zoom. Scroll
position intentionally transient. *Data*: `plumbline_config_load_json` /
`plumbline_config_save_json` (shared file with GTK — keep the shape).

## First run — who is opening the Book? (2026-07-26, both shells)

The first launch asks who's here (`FirstRun.svelte` / `FirstRun.kt` — keep
the copy in sync):

- **New in the faith** — a welcome from the maintainer (next steps: read —
  Ps 12:6–7; find a church — Heb 10:24–25; assurance — Rom 5:8, John 3:16,
  1 John 5:13, John 10:28–29, Phil 1:6, 1 John 1:9, 2 Tim 3:16–17). Every
  reference is tappable and opens **beside John**: web — second pane; fold —
  second pane; phone — the passage opens with John 1 as the saved start.
  "Open the book of John" lands in John 1 with **both analysis tiers off** —
  just the text.
- **Sharing the gospel** — straight into Present with the Romans Road
  (default tiers; the picker shows if the stock thread was removed).
- **Established believer** — the analysis-tier picker (scholars' / machine,
  with examples). The text is always on; tiers change any time in Settings.
  Dismissing without choosing (click-away / system back) keeps the defaults.

The old Simple/Full first-run modal is gone; `studyMode` still round-trips in
the config for older readers of the shared file.

## Primary menu (≡)

**Reworked 2026-07-26 (web): destinations vs utilities.** The web header now
mirrors the Compose IA — Read is the base layer; **Explore · Present ·
Memorize** are first-class header buttons (narrow screens fold them into ≡
above a divider), plus search, a first-class **Share** button, and a ≡ menu
holding utilities only (History · Guide & about · Keyboard shortcuts ·
Settings). Threads/Tags/Weaves live inside Explore, as on Android. The
subtitle is just the passage ("John 3" — no edition suffix; the e2e boot
signal matches `/\w+ \d+/`). **Share the app** (2026-07-25, both shells)
opens the hosted PWA's QR code (pre-generated matrix, no QR dependency —
`QrCode.svelte` / `QrShare.kt`) + the link via system share / copy; the same
QR closes Present's end card. Historical menu notes: **Weave views** (Suggested, Weave map, Constellation — disabled outside
Full study), **Reading** (Simple/Full radio + Verse-per-line toggle), **Theme**
(light/dark/night/follow-system radio), and **Guide / Keyboard shortcuts /
About**. GTK: a `gtk::MenuButton` + `gio::Menu` backed by `win.*` `SimpleAction`s
(string-stateful radios, boolean toggle). WinUI: a `DropDownButton` + `MenuFlyout`
with `RadioMenuFlyoutItem` / `ToggleMenuFlyoutItem`.

## Chord/arc "Map" popup (M:887–935, 2994–3087)

1000×360, Esc or the close button closes. **The book-pair fold lives in the core
view-model** `plumbline_engine_chord_map_json` → `{pairs:[{a,b,count}] (canon book
indices, a≤b), max, otNtDivide, bookCount}` (GTK calls `plumbline_core::weave::chord_pairs`
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

1200×640; ‹prev/next› + caption; Esc or the close button closes; Left/Right page.
**The whole layout is the core view-model** `plumbline_engine_constellation_json(page,
pins_json)` (pins = a JSON array of weave indices) → lanes of nodes + edges as
**fractions** (`x` a canon fraction, `laneFrac` 0..1 within a lane, `size` a
0..1 witness degree) plus `nPins/freeTotal/page/maxPage/caption/laneCapacity`;
GTK calls `plumbline_core::weave::constellation` directly. Usable = weaves with ≥1
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
view-model** `plumbline_engine_concept_map_json(code)` → `{code, centerLabel,
spokes:[{code, label, semantic}], byBook (canon-ordered counts), otNtDivide,
bookCount}`. The spoke union (embedding-near ∪ community, deduped, 6 each) lives
in `plumbline_rnd::concept::radial_spokes` (GTK calls it directly); labels
("gloss\nlemma") are baked by the endpoint. Paint only: radius `min(w,h)/2−95`;
semantic spokes gold, community green; centre node gold; dispersion cells gold α
`0.15+0.75·(cnt/max)` at `bi/bookCount`, OT/NT seam. No shell book-order table.

**Cross-testament bridge row.** `concept_map_json` also carries an optional
`bridge:{partners:[{code,label,prior}], byBook}` — the strongest other-testament
equivalents of `code` (top `concept::BRIDGE_ROW_PARTNERS`=6 from the already-fused
`FusedBridge`, i.e. etymology + `bridge/*.json` witnesses like Abbott-Smith) and
their **unioned** per-book dispersion (`concept::union_by_book`), canon-ordered
like `byBook`. Additive + `skip_serializing_if=None`, so the ABI/bindings are
unchanged (same fn, richer JSON) and a partnerless code omits it. This is what
makes an OT word light up its NT match: viewing *Christ* (G5547) fills the OT half
via *Messiah* (H4899, prior 0.93). **GTK** (`draw_dispersion`) and **WinUI**
(`Popups.ConceptMap`) both paint it as a second indigo row beneath the gold one
(strip 52-px, alpha `0.18+0.72·(cnt/max)` on the row's own max) and name the
partners in a caption. **Android** (`Maps.ConceptMap`) paints the same banded
row on its zoomable concept-map canvas — all three shells now show it.

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
| `plumbline_engine_verse_notes_json(ref)` | `{verse, notes[]}` or null | margin notes |
| `plumbline_engine_study_xrefs_json(ref)` | `{verse, refs:[{to, toDisplay, end?, votes}]}` | TSK tier |
| `plumbline_engine_weaves_json()` | full library: weaves + links incl. `approved`, `spanA/B`, `resolved`, `suggested` | compare card, weaves list, panel xrefs (chord map + constellation now have their own view-model endpoints) |
| `plumbline_engine_concept_json(code)` | `{total, ot, nt, topBooks, byBook, collocates, community, leitwort?}` | ALONGSIDE / CONCENTRATES / LEITWORT / dispersion |
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
| `plumbline_engine_link_pairs_json()` | `{pairs:[{a, aBook, aChapter, aVerse, b, bBook, bChapter, bVerse, resolved}]}` | ambient connectors + chord map (retires the shell dedup) |
| `plumbline_engine_canon_segments_json()` | `{segments:[{label, first, last}], otNtDivide}` | canon strip + map ruler bands (retires the WinUI hardcode) |

Both are thin wrappers over the one core source: `link_pairs` wraps
`plumbline_core::weave::link_pairs`; `canon_segments` wraps
`core::reference::CANON_SEGMENTS` / `OT_NT_DIVIDE`. GTK (being Rust) calls those
directly rather than round-tripping JSON.

Added for the popup view-models (2026-07-18, architecture-review P0.2 — the
three map popups' derivation moved into the core; positions cross the wire as
**fractions/logical units**, never pixels/colours):

| endpoint | returns | for |
|---|---|---|
| `plumbline_engine_chord_map_json()` | `{pairs:[{a, b, count}] (canon book indices, a≤b), max, otNtDivide, bookCount}` | chord/arc "Weave map" (retires the shell fold + max) |
| `plumbline_engine_concept_map_json(code)` | `{code, centerLabel, spokes:[{code, label, semantic}], byBook[] (canon order), otNtDivide, bookCount}` | concept map (retires the spoke assembly + gloss/lemma lookups + book table) |
| `plumbline_engine_constellation_json(page, pins_json)` | `{lanes:[{weaveIndex, name, pinned, nodes:[{x, laneFrac, size, refKey, book, chapter, verse, display}], edges:[{aX, aLaneFrac, bX, bLaneFrac}]}], nPins, freeTotal, page, maxPage, caption, laneCapacity}` (pins = JSON array of weave indices) | constellation (retires the usable/degree/jitter/paging/pin derivation) |

Producers: `chord_map` wraps `plumbline_core::weave::chord_pairs`; `constellation`
wraps `plumbline_core::weave::constellation`; `concept_map` bakes labels over
`plumbline_rnd::concept::radial_spokes` + `concept.stat`. GTK calls the core fns
directly; the non-Rust shells consume the JSON and map fractions → pixels.

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
these; GTK implements the trait on `State` and calls the producer directly, the
FFI implements it on `PlumblineEngine`. **Golden coverage (P2.6):** `panel_blocks_via_abi`
and `route_link_via_abi` exercise the block payloads + parser over the ABI, and
the producer itself has 15 unit tests over a fake source; the block kinds are a
Rust enum (a shell that meets an unknown kind renders nothing — forward-compat).

Not ported into any shell (by decision / data): signed patches + rules;
text-witness grading (shipped data never passes, so the "disputed" marker
stays silent); quotation detection (awaits hydrated inputs).

## Tier 0 daily-driver features (2026-07-19)

The eight small, additive daily-driver features from [TODO.md](../TODO.md) Tier
0. Shared logic lives in `plumbline-core`; GTK calls it directly, WinUI/Compose
through new FFI endpoints. New endpoints (all additive; bindings regenerated):
`plumbline_engine_copy_text`, `plumbline_engine_user_note_json` / `_notes_json` / `_set`,
`plumbline_engine_tag_set_color`, `plumbline_engine_chapter_highlights_json`,
`plumbline_theme_palette_json`, `plumbline_theme_highlight_tones_json`,
`plumbline_engine_warm_indexes`, `plumbline_panel_guide_blocks_json` / `_about_blocks_json`.
New panel-link verbs: `editnote:REF`, `guide`, `about` (parse + wire in both).

- **1. Copy & context menu.** Formatting is `plumbline_core::export::copy_text`
  (verse / verse+ref / markdown / chapter). Right-click a verse → menu: copy
  shapes, Note…, highlight tones, and (Full) Tag… / Add to thread… (the last
  three route through the panel dispatcher). GTK: a `gtk::Popover` of buttons +
  `area.clipboard().set_text`; WinUI: a `MenuFlyout` + `Clipboard.SetContent`.
  Verse-under-point = hit word's verse, else nearest verse-number by y.
- **2. Back/forward history.** Per-pane `(book, chapter)` stack + cursor, seeded
  with the opening chapter; navigation pushes (unless it *is* a history move),
  forward entries drop on a new jump. Alt+←/→ and mouse buttons 4/5 (GTK
  buttons 8/9; WinUI `XButton1/2`). Lives in the pane (GTK `Pane`, WinUI
  `ReaderView`), fed by `navigate_pane` / `ShowChapter`.
- **3. Personal margin notes.** `plumbline_core::usernote`: one JSON file per verse
  under `home/notes/`, refKey-keyed, atomic store; empty text deletes. A new
  `PanelSource::user_note` surfaces the "your note" block (both modes) via the
  content model; the `editnote:` verb prompts (multi-line). A square gutter mark
  sits left of the weave dot. GTK reads `State.usernotes`; WinUI via
  `user_notes_json` (gutter set) + `user_note_json` (prefill).
- **4. Highlighting.** Reuses the existing tag `color` field: a colour-bearing
  tag washes its verses. `tag::set_color` + `tag::verse_color`; a fixed 6-tone
  palette (`theme::HIGHLIGHT_TONES`). The context menu's "Highlight — <tone>"
  adds the verse to that tone's tag (created coloured); "Remove highlight"
  clears every colour-tag holding it. Whole-verse washes paint at the band site
  (GTK `band` closure; WinUI the highlight-band loop) under the search band.

  **Word-precise cross-verse ranges (drag).** Click-drag in the reader lays down
  a highlight from one word to another, spanning verses. Model: an additive
  `highlights` array on the tag file — `{startRef,startTok,endRef,endTok,color?}`,
  reusing the frozen refKey + `kjv1769-tok2` token offsets; an old reader ignores
  it and still shows whole-verse member washes (no new `TargetRepr` variant, which
  would break its parse). `tag::add_highlight` / `remove_highlight`;
  `tag::verse_highlight_runs` decomposes a range into per-verse `[lo,hi]` runs
  (partial first/last verse, whole interior). FFI (all additive):
  `plumbline_engine_highlight_add` / `_remove` / `_clear_verse`, and
  `plumbline_engine_chapter_highlights_json` gains a `runs` array beside `verses`.
  Both shells paint the runs as per-word rects (like the pinned-span band) and
  preview the live drag in the default tone; a press still pins the start word,
  a drag past a 6px threshold supersedes the pin, and endpoints are canonicalised
  (a backwards drag stores the same range). "Remove highlight" also drops any
  range covering the verse (GTK removes inline; WinUI via `_clear_verse`). GTK
  uses a `gtk::GestureDrag`; WinUI a pointer-capture drag on the `CanvasControl`.
- **5. Dark + night themes.** `plumbline_core::theme::Palette` is the one source
  (`palette(theme)`), served as `plumbline_theme_palette_json`; light values are the
  shipped ones (no regression), dark (candlelight-warm) + night (true-black) are
  new. Config gains `theme` (`system`/`light`/`dark`/`night`, additive). The
  reader canvas + chrome paint from the palette; the ≡ menu's Theme radio
  (light/dark/night/follow-system) sets the choice + persists. GTK drives the CSS provider + `AdwStyleManager` scheme from the
  palette; the study panel's accent hexes come from the palette via a
  thread-local (`MARKUP_PALETTE`, set at load + on switch) while its base ink
  inherits the `ForceDark`-themed label. WinUI re-applies the `Palette` static +
  `ElementTheme` and rebuilds captured brushes; its panel "ink" runs inherit the
  element theme (accents are palette-driven via `ColorOf`). **Delta (both shells): the analytical popups (chord / constellation /
  concept map) stay on their light popup paper in dark/night** — reading + panel
  + chrome are themed; the transient map overlays are not (a follow-up).
- **6. Kill the first-study-click pause.** `plumbline_engine_warm_indexes` forces the
  lazy analytics (concept / leitwort / SIF) to build. **Delta:** WinUI warms on
  a background thread (`Task.Run`) at startup in Full mode; GTK, whose engine
  state is single-threaded `Rc<RefCell>`, warms on the main loop via a
  `glib::timeout_add_local_once` just after first paint (proactive, not
  off-thread). Both move the stall off the first click.
- **7. In-app guide, shortcuts, About.** `panel::guide_blocks` / `about_blocks`
  are shared block lists (served engine-free); a Help button opens the guide in
  the panel, the guide links to About and vice-versa. The shortcuts overlay is
  shell-native (keybindings differ): `?`/F1 → a modal list (GTK `gtk::Window`,
  WinUI `ContentDialog`).
- **8. Small unifications.** Cross-book stepping via `canon::adjacent_book` —
  past a book's last chapter enters the next, before ch.1 the previous (was
  clamped in **both** shells, not just GTK as an earlier note said). All search
  hits band in any visible chapter (a hit set on the reader, painted at the band
  site). A Shift/Ctrl-click on a `go:` link opens the other pane (WinUI reads
  the modifier in the link handler; GTK captures it with a capture-phase click
  gesture on the study label just before the link activates).

## Per-tier analysis gates + tag→weave (2026-07-25, product round 4)

Street-use feedback retired two ideas at once: the all-or-nothing
Simple/Full switch ("weirdly selective") and highlight-tones-as-annotation.
**Tags are the primary annotation** (topic study accumulates over time); the
**weave comes later** from the tag. Landed core-first; Android consumes it now,
the other shells owe the UI (deltas below).

- **Gates.** `plumbline_core::panel::Gates { human, machine }` replaces the
  producer-level `full: bool`: *human* gates curated scholarship (RENDERINGS +
  reverse lens, morphology gloss, SAME ROOT, TSK), *machine* gates the
  learned/statistical tiers (SIMILAR CONCEPTS, ALONGSIDE, CONCENTRATES,
  LEITWORT, verses-like-this, the concept-map link). The text and the
  reader's own data — author actions (`＋ tag verse` / `＋ add to thread`),
  the verse's tags + `untag`, weave xrefs, margin + personal notes, the
  compare card's `✎ note` — are **never gated**. Legacy `full:bool` fns
  remain as exact wrappers (Full = all on), so GTK compiles unchanged.
- **Note-first panel.** The reader's own note block moved to the **top** of
  the word study (right under the tapped word), in every mode.
- **ABI (additive).** `plumbline_engine_word_study_blocks2_json(ref, tok, gates)`
  and `plumbline_engine_code_study_blocks2_json(code, word?, gates)` — `gates`
  bitmask: bit 0 human, bit 1 machine. `plumbline_engine_weave_from_tag(tag,
  refsJson|null=all verse members, weaveName|null=tag name, added)` chains
  the tag's passages canon-ordered (`weave::add_chain`: sorted, deduped,
  consecutive pairs; find-or-create + link-dedup make re-runs additive).
  Surface is now **90 fns**; bindings regenerated.
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
- **Deltas owed (GTK/WinUI):** the two gate switches in place of the
  Simple/Full UI (their menus still show the radio; producers already accept
  both via the legacy wrappers), reader-level Tag… action, tag→weave UI
  (`makeweave:` routing + subset picker), personal-note gutter mark parity
  (GTK already has one; WinUI check), scroll-verse restore, and consuming the
  `*_blocks2` endpoints. **Web shell (branch `web-shell`):** same list —
  adopt before merge.

## Backup / restore (2026-07-25, both shells)

Settings exports the authored home — `tags/ threads/ weaves/ notes/ memory/`
+ the config as `.config/plumbline/config.json` + a `plumbline-backup.json`
marker — as a **zip with a shared layout**, so one archive restores across
devices (phone ↔ browser). Restore is merge-by-overwrite (same-name items
replaced), path-filtered to the authored dirs (no traversal), then the engine
re-opens over the restored home. Web: dependency-free zip (store-only write;
store+deflate read) in `apps/web/src/engine/zip.ts`, IndexedDB write with ALL
persistence frozen until the reload (three clobber paths guarded, covered by
the Playwright round-trip test). Android: `ui/Backup.kt` over SAF
Create/OpenDocument + java.util.zip; restore recreates the activity.

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

**GTK** drives `plumbline_core::memory` directly: `≡` → Memorize (Review due / Coverage
map / Activity); context menu "Memorize this verse". The review window steps the
due queue with a first-letter / blank-out-slider / typed-recall drill + the four
grade buttons; the coverage map reuses the canon-strip dispersion language shaded
by mastery; activity is reviews-per-day columns. **WinUI** mirrors via the C ABI
(StudyEngine.Memory* + Wire memory records). Decks are sourced one verse at a
time for v1; **delta:** a "memorize this tag/thread" bulk-enqueue and printable
flashcards (needs #14) are follow-ups.

## Web shell (apps/web, branch web-shell — 2026-07-25)

The fourth shell: Svelte 5 + TS over the **same C ABI**, compiled unchanged to
`wasm32-wasip1` and run in the browser under `@bjorn3/browser_wasi_shim` with
an in-memory home (data pack fetched + gunzipped into it; authored files
mirrored to IndexedDB after every write; the corpus idxcache persisted for
fast reopens). `apps/web/src/engine/StudyEngine.ts` is the method-for-method
TS sibling of StudyEngine.kt / Plumbline.cs. Build:
`npm run pack:data && cargo build -p plumbline-ffi --release --target
wasm32-wasip1 && npm run pack:wasm && npm run build` (in apps/web). Two
wasm-only ABI shims live in `crates/ffi/src/wasm.rs` (`plumbline_web_alloc/free`,
the `plumbline.plumbline_js_measure` import surfaced as a `PlumblineMeasureFn`);
plumbline-bindgen excludes them from the native bindings by name.

Feature state (per this manifest): reader core (canvas painter, measure via
canvas `measureText`, all flags/bands/washes/runs/gutter marks), multi-pane
(≤3) + canon strip + ambient connectors, the whole panel content-model +
link router (incl. `makeweave:`), live search, hover gloss (native tooltip),
keyboard map + wheel + touch (pan, long-press menu, horizontal chapter
swipe), context menu (copy shapes / note / tones / tag / thread / memorize),
tag picker + tag→weave sheets, drag highlights, the
three map popups from the core view-models (pinch-zoom), memorization (hub /
drill / coverage / activity), Present mode (sunlight, share + the hosted
PWA link + its QR on the end card), notes browser, history, first-run,
guide/about/shortcuts,
light/dark/night/system themes from the core palette, per-tier gates,
config round-trip incl. scroll-verse restore (flushed on tab hide — the
ON_PAUSE twin), PWA (installable, offline after first visit; pack cached
`?v=<content-hash>`).

Web deltas: engine runs on the main thread (GTK-style; a worker is the
escape hatch if jank shows — TODO #28's remaining stage). **Boot ships the
core pack only** (2026-07-25, TODO #28): the `rnd`-marked artifacts
(morphology, concept vectors) stream in after first paint —
`loadRndPack` → `plumbline_engine_load_rnd_data` → a re-warm builds the SIF —
at idle / on the first-run machine choice / on the Settings toggle, with
`studyEpoch` refreshing any open panel; until they land, the machine tiers
are simply absent, exactly like an Android install (which never bundles
them). Analytical popups keep light paper (shared delta); user data lives
per-browser (export/import is the portability story);
Present "In context" fade not built. Hosting decided 2026-07-25: GitHub
Pages at <https://plumblinebible.org/> (custom domain, same day; the old
github.io URL 301s there), deployed by the release workflow on every `v*`
tag (base "./", so any host or subpath works without a rebuild; the
scripture font's @font-face lives in `public/fonts.css` to keep its URLs
base-relative).

## Android notes

- **On-device feedback round 3 (2026-07-24/25, v0.4.0–v0.5.0).** Landed
  Android-first from on-device street-use feedback; the product features among
  them are **GTK/WinUI deltas** owed to the desktop shells:
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
    the recipient the app, not just the text). **Delta (GTK/WinUI):** a projection-friendly
    presentation window (fullscreen, large type, step keys) from the same
    thread data.
  - **Embedded study maps** (`ui/StudyMaps.kt`): the concept map + canon
    dispersion heatmap as scaled-down, first-class cards inside the word-study
    panel (before the first titled section), tapping through to the fullscreen
    map / a book jump. **Delta (GTK/WinUI):** same embed in their study panels
    (both already have the fullscreen popups).
  - **Notes browser** (`ui/Notes.kt`, Explore ▸ Notes): every personal note,
    browsable; tap → passage, Edit in place. **Delta (GTK/WinUI):** desktop
    only paints note gutter marks; a notes library view is owed.
  - **Tag picker** (`ui/VerseActions.kt` TagPickerSheet): `addtag:` offers
    existing tags first (plain before coloured tone tags), freetext "New tag…"
    secondary; tags stay colourless unless explicitly coloured. **Delta
    (GTK/WinUI):** both still open a bare text prompt.
  - **Memorize hub layout**: coverage is an inline strip above the verse list
    (not a screen); Activity is a half/half calendar-heatmap + history-log
    split. **Delta (GTK/WinUI):** both still present coverage/activity as
    separate popups with the bar-chart activity.
  - Phone-idiom (no desktop port intended): **bottom nav bar** (Read · Explore
    · Present · Memorize, one-handed reach; `ui/NavIcons.kt`), the **passage
    navigator** (`ui/BookNav.kt`: OT/NT → book → chapter → verse tap grids,
    replacing the book dropdown; ReaderPane scrolls the target verse into
    view), the near-fullscreen-expandable study sheet, and the reader
    whitespace fix (manifest MARGIN/MAX_COLUMN are logical units — density-
    scaled on Android).
- **Compose parity (passes 1–3, 2026-07-24).** The Compose shell reached
  near-parity with GTK/WinUI. Beyond the v0 reader + word study + search + fold
  layouts it now has: **memorization** (review drill · coverage · activity —
  `ui/Memorize.kt`); the **concept / constellation / chord** maps as
  pinch-zoom/pan canvases (`ui/Maps.kt`, incl. the cross-testament bridge row);
  **Tier-0 verse actions** via a long-press sheet (copy/share · note ·
  verse-then-trim highlight · memorize — `ui/VerseActions.kt`); **study
  routing** for every panel verb (occurrences / rendering / codeStudy / thread /
  tag / weave / guide / about → the block pane); the **≡ study libraries**
  (threads / tags / weaves / suggested / guide / about) + a **Full-study**
  toggle (surfacing the morphology / similar / bridge-partner + authority-tier
  blocks); a **word-study bottom sheet** on narrow screens; and **authoring**
  (add tag/thread, edit note, approve/reject suggested weaves). Form-factor
  calls (product decisions, 2026-07-24): zoomable canvases, study bottom
  sheet, verse-then-trim highlighting. So the per-feature "Compose delta" notes
  below are **resolved** except: `editThreadNotes` / `editWeaveNotes` /
  `editEntryNote` / `untag` (need an index→name lookup); the cross-testament
  **bridge data isn't loaded on Android yet** — `OpenFromBytes` has no home dir,
  so `bridge/*.json` isn't read; the bridge row is wired but empty until asset
  or extract-to-home wiring lands; and posture-driven fold-mode switching is
  untested on hardware.
- The Kotlin/JNA binding (`crates/ffi/bindings/kotlin/Plumbline.kt`, package
  `dev.plumbline.core`) is now current with the 87-fn C ABI (incl. the 9
  `plumbline_engine_memory_*`). It is the low-level `PlumblineNative` interface +
  JNA types (`PlumblineLayoutConfig`, `MeasureCallback`) **only** — the earlier
  duplicate camelCase wrapper was removed (and the interface renamed from
  `PlumblineFfi`); the single PascalCase wrapper is `app/.../StudyEngine.kt`,
  method-for-method with `bindings/csharp/PureStudy.cs`. The native lib
  cross-builds with cargo-ndk into `jniLibs/{arm64-v8a,x86_64}/libplumbline_ffi.so`
  (NDK r29, `--platform 26`), verified independently of the emulator/SDK.
- **Memorization (Tier 2 #15) — Compose delta:** the binding + `StudyEngine`
  (`Memory*`) + `Wire.kt` records are in place, but the Memorize UI (review
  drill · coverage map · activity) is **not yet in the Compose shell** — build
  it from the GTK/WinUI reference when the shell lands.
- **Rendering lens (2026-07-16) — Compose delta:** the two endpoints
  (`renderingsJson` / `wordCodesJson`) are already in the Kotlin binding, but
  the RENDERINGS tier + the `rend:` and `code:` routes are **not yet in a
  Compose shell**. Build them from the GTK/WinUI reference when the Compose
  shell lands, including the reusable code-study view (`code_study_markup` /
  `CodeStudy`) that both the per-code block and the `code:` verb share.
- **App/window icon — Compose delta:** GTK (`install_app_icon`, scalable SVG at
  `apps/desktop/assets/icons/hicolor/scalable/apps/dev.plumbline.app.svg`) and
  WinUI (`.ico`) both wire the woven cross; Android needs its own launcher
  icon (adaptive-icon / mipmap) generated from `plumbline.svg` — not the
  hicolor tree.
- **Authority tiers (2026-07-16) — Compose delta:** the classification lives in
  Rust (`bridge::source_tiers`/`research_grade`/`tiers_of`) and rides the wire
  (`tiers`/`researchGrade` on bridge partners), so a Compose shell reads it for
  the SAME ROOT marks and hardcodes the fixed-by-block tiers (SIMILAR CONCEPTS =
  Machine, TSK = Human, …) like GTK/WinUI. The mark glyphs (✝ † ≈ ⚗), their
  colors, and the legend are **not yet in a Compose shell** — build from the
  GTK/WinUI reference (see **Authority tiers** above). The Kotlin binding must
  also gain `bridgePartnersJson` (its `PlumblineFfi` interface omits the R&D tier).
- **View-model consolidation (2026-07-16, P0.3) — Compose delta:** the two new
  endpoints (`plumbline_engine_link_pairs_json` / `plumbline_engine_canon_segments_json`)
  are **not yet in the Kotlin `PlumblineFfi` interface** (kept minimal like the other
  study-tier endpoints). A Compose shell adds the two JNA decls + wrappers, then
  consumes them for its connectors and canon strip instead of re-deriving —
  exactly what this phase removed from GTK/WinUI.
- **Panel content-model + link router (2026-07-18, P0.1 + P1.4) — Compose
  delta:** the whole study panel is a typed block list from the
  `plumbline_engine_*_blocks_json` family, and links parse via `plumbline_route_link_json`
  — none are in the Kotlin interface yet. A Compose shell adds those JNA decls +
  wrappers, then walks the blocks with a small per-block composable
  (`Section`/`Para`/`Rule`, runs → `AnnotatedString` with the colour-role map +
  clickable link URIs) and dispatches clicks on the parsed `{verb}`. It writes
  **no** panel derivation — the producer owns tier order, caps, gloss/lemma,
  snippets, and the verb vocabulary. Colour roles map identically to GTK/WinUI.
- **Popup view-models (2026-07-18, P0.2) — Compose delta:** the three map
  popups now come from `plumbline_engine_chord_map_json` / `plumbline_engine_concept_map_json`
  / `plumbline_engine_constellation_json` (all **not yet in the Kotlin interface**).
  A Compose shell adds the three JNA decls + wrappers and paints the returned
  fractions — it never re-derives the fold / spoke assembly / lane layout.
  Positions are fractions/logical units, so the Compose mapping is the SAME
  `plotLeft + x·(w−plotLeft)` / `topPad + (lane+laneFrac)·laneH` the GTK/WinUI
  reference uses; keep `laneCapacity`, plotLeft 162, topPad 18, gutter 150, and
  the `1.4+2.4·size` radius identical so all three shells place a node alike.
- **Tier 0 (2026-07-19) — Compose delta:** the new endpoints (copy text,
  user-note read/write, chapter highlights, palette, highlight tones, warm,
  guide/about blocks) are **not yet in the Kotlin `PlumblineFfi` interface**. A
  Compose shell adds the JNA decls + wrappers, then: a long-press/overflow
  context menu (copy via `ClipboardManager`, note, highlight tones, tag/thread);
  per-pane history (system back + gesture); a note gutter mark + the "your note"
  block (already in the content model); highlight washes from
  `chapterHighlightsJson`; theme from `paletteJson` (Compose is well-suited —
  a `MaterialTheme`/`Color` map, follow-system via `isSystemInDarkTheme()`);
  `warmIndexes` on a coroutine at startup; the guide/about blocks + a shortcuts
  sheet; cross-book stepping + all-hits banding + modifier-open-other-pane.
- Build gate: Android NDK + `cargo-ndk` for the `.so` per ABI; the Rust and
  the JSON contract are identical.
- Measure callback: back it with `android.graphics.Paint.measureText` (or
  Compose's TextMeasurer); the core does the rest.
- **v1 phone shell (2026-07-24) — Compose delta (form-factor UX, on-device
  feedback).** The phone drops the always-split layout + the Split/Single and
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
  **Copy** action. Desktop keeps its right-click copy variants + fixed layout;
  the desktop UI for `copyStyle`/`sideMargin`/`lineSpacing` is a pending follow-up
  (the fields round-trip on both desktop shells regardless). Requires the
  `material-icons-core` dependency.
- **v1 phone shell, round 2 (2026-07-24) — Compose delta.** Overflow menu cut to
  five entries — Memorize / Explore / History / Guide & About / Settings — so it
  never scrolls; the fold's second-pane flip is a top-bar icon, not a menu item.
  **Memorize** is a hub (a list of every card from `MemoryCoverageJson`, canon-
  sorted, + Review due / Coverage / Activity buttons). **Explore** is a described
  card list (Threads, Tags, Weaves, Constellation, Chord) so the tools aren't
  cryptic; **Weaves** is one screen with an All/Suggested filter (was two items).
  **History** is a bottom sheet over the new `history` config field; the reader
  restores the last-viewed passage from `openPanes` and persists it + history on
  every chapter change (off-thread). **Settings** folds Full study + text size /
  margin / line-spacing + copy format + bundled set into one dialog. Guide &
  About are combined in the core (`guide_blocks` inlines `about_body`; the
  standalone About card stays for the `about` link verb). Word study / library /
  search run off the main thread and the analytics index warms at startup
  (`WarmIndexes`) to kill the first-tap stall. Map pan is bounded (the shared
  `zoomable` clamps the offset so a map can't be flung off-screen; pinned at 1×).
  A memorize add shows a Toast. Desktop keeps its menu/right-click layout; these
  are phone form-factor deltas.
