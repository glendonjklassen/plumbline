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
navigate / zoom / rebuild (60 ms delay) / authoring. For each deduped
canonical link pair (client builds from the weave library, as GTK
`build_links` M:185): map both endpoints' `(book, chapter)` to showing panes
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

## Word study panel (double-click **or Ctrl+click**; M:3106–3377)

Sidebar 380 px, on-demand; Esc hides; clearing search hides. Content order —
(F) = Full mode only:

1. Verse ref bold; the word xx-large.
2. (F) Morphology gloss line, small, `#6a5a2a`.
3. Per Strong's code (else "*no Strong's tag on this word*"):
   code bold + "**N occurrence(s) ▸**" → concordance; lemma x-large; xlit
   italic; pron `#888`; definition; `KJV: …` small; then (F) tiers with
   small-caps gold headers:
   - **RENDERINGS** — the other English words this code is translated as
     (corpus-derived, not R&D), most frequent first; the tapped word's own
     rendering is **bold**. Each chip shows `×count` and links
     `rend:CODE:rendering` → a concordance filtered to that rendering (cap
     OCC_SHOWN). When the reverse lens maps the tapped surface word to >1 code,
     a "“word” also translates …" line (`#6b6862`) links the other codes. New
     feature — no overlay antecedent. *Data*: `pure_engine_renderings_json` +
     `pure_engine_word_codes_json`.
   - **SAME ROOT ACROSS TESTAMENTS** — bridge partners (≤6) as gloss chips →
     concordance links; sources humanized (`lxx`→Septuagint, `quotation`→NT
     quotation); "· disputed by usage" in `#b04a3a` when the text-witness
     disbelieves (shipped data never grades, so silent).
   - **SIMILAR CONCEPTS** — embedding neighbours (6); "across the testaments —"
     cross (6).
   - **APPEARS ALONGSIDE** — concept community (8).
   - **WHERE IT CONCENTRATES** — top books (5) "Book ×N · …" + "(OT x · NT y)".
   - **LEITWORT** — "{winCount} of its {n} uses cluster in {label} (p ≈ 10^−{score})".
   - "▸ open concept map" link.
4. (F) Author actions: `＋ tag verse`, `＋ add to thread`.
5. **cross-references (N)** — weave partners (≤40), each + weave-name link to
   its compare card.
6. (F) **study cross-references (N) — TSK** (≤40; ranges "a–b").
7. (F) **verses like this** — SIF in-testament (6); cross (4).
8. (F) **tags** — tags holding this verse; each is a link + `✕` untag.
9. **margin notes** — the verse's 1769 notes, small.

Concept chips render english-first: "**gloss** *lemma*" joined by "  ·  ";
the gloss is the modal KJV rendering across ≤80 occurrences (skip FLAG_ADDED
tokens, strip edge punctuation, ties lexicographic; fallback: distilled
def/kjv clause ≤30 chars). *Data*: `pure_engine_gloss` computes this
engine-side; plus `strongs/occurrences/morph/bridge/concept-neighbours/
concept/similar-verses/verse-xrefs/study-xrefs/tags/verse-notes` endpoints.

## Link routing (GTK `handle_link`, M:2486–2564)

All panel interactivity funnels through one URI dispatcher — replicate it:
`go:Book:ch[:v]` · `occ:CODE` · `rend:CODE:rendering` · `thread:i` · `tag:i` · `addtag:refkey` ·
`addthread:refkey` · `untag:i:refkey` · `approve:i` · `reject:i` ·
`editthreadnotes:i` · `editentrynote:ti:ei` · `editweavenotes:i` · `weave:i` ·
`conceptmap:CODE`.

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

1000×360, Esc/click-outside closes. Fold deduped link pairs into canon-ordered
book-pair counts. Canon axis with section bands + labels, gold baseline, OT/NT
seam. Ribbons heaviest-first: alpha `0.12+0.30·(cnt/max)`, foot width
`2+8·(cnt/max)`; colors OT `(0.82,0.70,0.43)` / NT `(0.50,0.70,0.90)` / cross
`(0.78,0.59,0.86)` (+0.08 α, cap 0.5); apex `min(0.42·h, 22+0.26·h·|dx|/w)`;
self-pair = small loop. Click: x→book → navigate active pane + close.

## Constellation popup (M:937–1529)

1200×640; ‹prev/next› + caption; Esc/click-outside closes; Left/Right page.
Usable weaves = ≥1 link with both ends resolving in the corpus, sorted
largest-first. 18 lanes; pinned lanes (by weave file identity) first and
stay; the rest page. x = `162 + ((order + (ch−1)/chapters)/66)·(w−162)`; y =
lane centre + jitter `((ch·3+v)%7−3)·laneH·0.12`; 7-colour cycle ×0.72; node =
square, half-size `1.4+2.4·(deg/maxDeg)` (degree across the whole library).
Pin gutter at x<150: filled gold 8×8 pinned / hollow gray otherwise; lane name
≤22 chars; canon ruler on top; OT/NT seam. Hover tooltip "verse · weave" in a
dark box. Hit priority **node > edge > pin-gutter**; node → navigate (stays
open); edge → compare card (closes); gutter → toggle pin. Caption: "{N pinned
· }weaves X–Y of Z · largest first · click the ▪ to pin a lane".
*Data*: `weaves_json` (has `resolved` per link) + toc for chapter counts.

## Concept map popup (`conceptmap:`; M:724–883)

720×560: radial graph + 40-px dispersion strip. Neighbours = embedding
nearest (6, gold spokes) + community (6, green spokes), deduped; radius
`min(w,h)/2−60`; labels "gloss\nlemma"; centre node gold. Dispersion: per-book
cells gold α `0.15+0.75·(cnt/max)`, OT/NT seam.
*Data*: `concept_neighbours_json` + `concept_json` (byBook, community) +
`gloss`.

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
| `pure_engine_weaves_json()` | full library: weaves + links incl. `approved`, `spanA/B`, `resolved`, `suggested` | connectors, gutter dots, compare card, chord map, constellation |
| `pure_engine_concept_json(code)` | `{total, ot, nt, topBooks, byBook, collocates, community, leitwort?}` | ALONGSIDE / CONCENTRATES / LEITWORT / dispersion |
| `pure_engine_gloss(code)` | plain english gloss or null | concept chips |
| `pure_engine_weave_add_link_spans(name, a, b, aLo, aHi, bLo, bHi, added)` | null/error (negative span = none) | word-span links |
| `pure_config_load_json()` / `pure_config_save_json(json)` | config wire above (+`firstRun` on load) | session/mode/zoom |

Added for the rendering lens (2026-07-16):

| endpoint | returns | for |
|---|---|---|
| `pure_engine_renderings_json(code)` | `{code, renderings:[{rendering, total, capped, refs:[{verse, display, span:[s,e]}]}]}` (refs cap 500) | RENDERINGS tier + filtered concordance |
| `pure_engine_word_codes_json(word)` | `{word, codes:[{code, count}]}` | "also translates" reverse line |

Not ported into any shell (by decision / data): signed patches + rules;
text-witness grading (shipped data never passes, so the "disputed" marker
stays silent); quotation detection (awaits hydrated inputs).

## Android notes

- The Kotlin/JNA wrapper (`crates/ffi/bindings/kotlin/PureStudy.kt`, package
  `dev.purestudy.core`) predates the parity endpoints — extend it like
  `bindings/csharp/PureStudy.cs` (which is current).
- **Rendering lens (2026-07-16) — Compose delta:** the two endpoints
  (`renderingsJson` / `wordCodesJson`) are already in the Kotlin binding, but
  the RENDERINGS tier + `rend:` route are **not yet in a Compose shell**. Build
  them from the GTK/WinUI reference when the Compose shell lands.
- Build gate: Android NDK + `cargo-ndk` for the `.so` per ABI; the Rust and
  the JSON contract are identical.
- Measure callback: back it with `android.graphics.Paint.measureText` (or
  Compose's TextMeasurer); the core does the rest.
