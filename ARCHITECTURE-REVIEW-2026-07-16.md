# Architecture review — 2026-07-16

Scope: structural/architectural improvements, with particular attention to the
question **"is GTK/WinUI too heavy, and what could be pulled back into Rust?"**
No code was changed; this is a report. Findings are prioritized, with concrete
file/line evidence and a recommended plan.

Context that shaped the priorities (confirmed with Glendon):

- The **Compose/Android shell is next** — so the headline is *consolidate shared
  logic into Rust before a third copy is written*, not after.
- **Growing the FFI surface is fine** (view-model endpoints are additive, so no
  wire-contract break).
- Pain already felt: **parity drift** between GTK and WinUI, and the
  **4127-line `apps/desktop/src/main.rs`**.

---

## TL;DR

The bones are good. The crate split (`core` / `layout` / `rnd` / `ffi` /
`hydrate`) is clean, the feature-gating is disciplined, and the load-bearing
idea — *"layout lives in Rust, shells paint a display list and forward input"* —
**genuinely works for the reader**. Keep all of that.

The problem is that the thin-shell contract **stops at the reader**. For
everything else — the study panel, the three popups, the connectors, the canon
strip, the link routing — a large amount of *non-painting* logic (data
derivation, geometry, content-model assembly, interaction routing) has leaked
into each shell and is **hand-duplicated between GTK (Rust) and WinUI (C#)**,
kept in sync only by a 300-line prose contract ([docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md)).
Roughly **half of each shell is logic, not painting** — and Android is about to
make it a third copy.

**Direct answer to "is GTK/WinUI too heavy?"** The *native* parts (fonts, canvas
drawing, scroll physics, dialogs) are appropriately thin and **should stay
native** — that's the whole point of native-per-platform. But too much *logic*
has crossed the reader boundary into the shells. The fix is not to change UI
toolkits; it's to **extend the display-list idea to the rest of the app**: the
core emits normalized *view-models*, the shells only paint and route input.

---

## What's working (don't touch)

- **Crate boundaries.** `core` (domain), `layout` (pure algorithm over an
  injected `Measure`), `rnd` (feature-gated analytics, off by default), `ffi`
  (one flat C ABI), `hydrate` (CLI). Clear, dependency-light, and the workspace
  keeps GTK out of `default-members` so `cargo test -p pure-core` stays fast
  ([Cargo.toml:12-21](Cargo.toml#L12-L21)).
- **The reader's thin-shell contract.** [crates/layout/src/lib.rs](crates/layout/src/lib.rs)
  owns line-breaking + per-word hit regions; both shells feed a native
  `Measure` callback and paint the returned list
  ([ReaderView.cs:292-334](apps/windows/PureStudyWin/ReaderView.cs#L292-L334)).
  Hit regions are always consistent with what was painted because the same
  engine measured and drew them. **This is the template for everything below.**
- **FFI safety + contract discipline.** `catch_unwind` firewalls on every entry
  point, opaque handles, camelCase-JSON that evolves additively, a
  `PURE_WIRE_VERSION`, golden wire samples in tests, and a CI binding-drift
  guard ([crates/ffi/src/lib.rs:24-93](crates/ffi/src/lib.rs#L24-L93)). Any new
  view-model endpoints should slot into exactly this discipline.

---

## The core finding: logic has leaked into the shells

Below, "duplicated" means the same derivation exists in **GTK Rust** and **WinUI
C#** today and will need a **third Kotlin copy** for Compose. Painting
(Béziers, colors, fonts) legitimately differs per shell and is *not* the
concern — the concern is the data/geometry/content decisions feeding it.

| # | Duplicated logic | GTK (Rust) | WinUI (C#) | Nature |
|---|---|---|---|---|
| 1 | **Weave link dedup** → canonical pairs | `build_links` [main.rs:190](apps/desktop/src/main.rs#L190) | LinkPair build + shell index | pure data |
| 2 | **Chord map** book-pair fold + max | `chord_arcs` [main.rs:3047](apps/desktop/src/main.rs#L3047) | [Popups.cs:73-81](apps/windows/PureStudyWin/Popups.cs#L73-L81) | pure data |
| 3 | **Constellation** usable-filter, per-verse degree, lane/paging/pin, node fractional-x, jitter, node size | `draw_constellation` [main.rs:1216](apps/desktop/src/main.rs#L1216) | [Popups.cs:232-471](apps/windows/PureStudyWin/Popups.cs#L232-L471) | data + geometry |
| 4 | **Concept map** spoke assembly (near ∪ community, dedup), labels, dispersion norm | `draw_concept_radial` [main.rs:798](apps/desktop/src/main.rs#L798) | [Popups.cs:506-620](apps/windows/PureStudyWin/Popups.cs#L506-L620) | data + geometry |
| 5 | **Canon segments** (8 bands, OT/NT=39, pin fraction) | reads `core::reference::CANON_SEGMENTS` | **re-hardcoded** [CanonStrip.cs:15-20](apps/windows/PureStudyWin/CanonStrip.cs#L15-L20) | drift risk |
| 6 | **Link router** — the whole URI verb table + write→reload→refetch choreography | `handle_link` [main.rs:2530](apps/desktop/src/main.rs#L2530) | `StudyPanel.Link` [StudyPanel.cs:65-182](apps/windows/PureStudyWin/StudyPanel.cs#L65-L182) | interaction model |
| 7 | **Word-study panel content model** — tier order, caps, (F)-gating, `humanize`, `RenderKey`, chip gloss/lemma, "also translates" line | GTK panel builder | [StudyPanel.cs:308-548](apps/windows/PureStudyWin/StudyPanel.cs#L308-L548) | content model |
| 8 | **Search snippet** windowing | (shell) | `Snippet` [StudyPanel.cs:812-850](apps/windows/PureStudyWin/StudyPanel.cs#L812-L850) | presentation |

Item 5 is the clearest illustration of the risk: [CanonStrip.cs](apps/windows/PureStudyWin/CanonStrip.cs)
*comments* that the segments are "frozen in `core::reference::CANON_SEGMENTS`"
and then hardcodes them anyway. GTK reads the core; WinUI has a copy that can
silently drift. Multiply that pattern across items 1–8.

### Rough size of the leak

Hand-written WinUI shell ≈ **3,100 lines** (excluding `obj/`). Of that, the
portion that is *derivation / content-model / routing* rather than painting or
native plumbing is roughly:

- `StudyPanel.cs` — ~700 of 891 (content model + router)
- `Popups.cs` — ~500 of 632 (derivation + geometry; window plumbing is the rest)
- `ConnectorLayer.cs` / `CanonStrip.cs` — ~90 combined (dedup + segments)
- `MainWindow.cs` — the link-pair/xref index folding

≈ **1,300–1,500 lines (~45%) of the WinUI shell is logic a third shell must
re-implement.** That is the quantified "too heavy." The GTK shell carries the
same mass inside `main.rs`.

### Why it's already biting

- **Parity is enforced by prose + human review, not the compiler.** The manifest
  literally instructs "replicate it" for the router and lists behavioral deltas
  (e.g. WinUI added cross-book header stepping; GTK's `step_pane` clamps
  in-book — [FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md) §Multi-pane). There
  is **no cross-shell golden test**.
- **Every feature costs N× implementations.** The rendering-lens TODO spells it
  out: "GTK + WinUI in the same change set; log the Compose delta"
  ([TODO.md:48](TODO.md#L48)). Android makes every future feature 3×.

---

## Recommended architecture: view-models in the core

Extend the reader's proven pattern to the whole app. The core already emits a
**display list** the reader paints; have it emit **view-models** for everything
else. The rule that keeps this clean:

> **The core owns *what* and *where-proportionally*; the shell owns *pixels* and
> *paint*.** View-models carry data + positions as **fractions/logical units**,
> never device pixels, colors, or fonts. The shell maps `(fraction) → (w,h)`,
> picks palette/typeface, and does all drawing, hit-*feel*, animation, dialogs,
> and focus.

This is deliberately *not* "move the UI into Rust." Fonts, measurement, canvas
drawing, scroll physics, hover timing, and native prompts **stay in the shell**
— that's why native-per-platform was chosen and it's the right call. The goal
is only: **no business logic and no content model in the shell.**

### Concrete endpoints (all additive)

- `pure_engine_link_pairs_json()` → deduped canonical pairs. Retires item 1.
- `pure_engine_canon_segments_json()` → the segments already in
  `core::reference`. Delete the C# hardcode (item 5).
- `pure_engine_chord_map_json()` → `{books, pairs:[{a,b,count}], max}` (item 2).
- `pure_engine_constellation_json()` → laid-out lanes with node positions **as
  fractions**, degrees, pin/paging state (item 3). Shell scales + paints.
- `pure_engine_concept_map_json(code)` → spokes (deduped, `semantic` flag),
  labels, dispersion cells (item 4).
- **The big one — a typed content-model for the panel.** Return an ordered list
  of typed blocks the shell walks and renders:

  ```
  [ {kind:"heading", text, style},
    {kind:"para",   runs:[{text, style, uri?}]},
    {kind:"chips",  items:[{label, sublabel?, uri}]},
    {kind:"rule"} , … ]
  ```

  One Rust producer owns tier order, caps (≤6 bridge / ≤8 community / 5 books /
  ≤40 xref / 300–500 occ), (F)-gating, `humanize`, `RenderKey`, gloss/lemma
  formatting, and pre-baked `uri`s. Each shell has a *small* per-block renderer
  (GTK label/Pango markup, WinUI `Inlines`, Compose `AnnotatedString` — all three
  support styled runs with inline links). This collapses items 6, 7, 8 (word
  study, search results, concordance, threads/tags detail, compare card,
  suggested queue) from ~550 C# lines + the GTK twin into **one producer + three
  thin renderers**.

- **Link routing:** define the verb table once. Minimum: generate the URI
  verb constants + parser from one source so `go/occ/rend/thread/tag/addtag/…`
  can't drift. Stronger (recommended given "grow FFI freely"): a core
  `pure_engine_activate(uri)` that performs the write-verbs and returns "what
  changed" so the **write→reload→refetch** choreography lives once; the shell
  keeps only navigation + native prompts.

### Trade-offs (stated honestly)

- **Coupling core to view concerns.** Mitigated by the fractions-not-pixels rule
  and by keeping the block vocabulary small and generic. If a view-model ever
  needs a pixel or a hex color, it's in the wrong layer.
- **A new frozen contract.** The block model must be additive like the rest of
  the wire: unknown block kinds render as nothing (forward-compatible), so the
  core can add kinds without breaking older shells.
- **Don't over-pull.** Scroll physics, hover debounce, dialog UX, active-pane
  feel, and the actual canvas are shell-owned and should stay that way.

---

## Prioritized plan

### P0 — before the Compose shell (highest leverage)

1. **Panel content-model endpoint(s)** (items 6–8). Biggest duplication, biggest
   parity risk, and the thing you'd otherwise write a third time in Kotlin.
   Land it, then retire the GTK + WinUI panel builders onto it in the same
   change. *Effort: high. Risk: medium (new contract) — de-risk by shipping the
   block model behind the existing endpoints first, then cutting shells over.*
2. **The three popup view-models** — chord / constellation / concept map (items
   2–4). Return derived graph + normalized coords; shells keep only paint.
   *Effort: medium. Risk: low.*
3. **`link_pairs` + `canon_segments`** (items 1, 5). Small, deletes real
   drift risk. *Effort: low. Risk: low.* **Do this first as the warm-up** — it
   proves the pattern cheaply.

### P1

4. **Link-routing single source of truth** (item 6 router). Removes the
   "replicate `handle_link`" clause from the manifest. *Effort: medium.*
5. **Split `apps/desktop/src/main.rs`** (4127 lines → modules). It violates the
   CLAUDE.md "no 3k-line files" rule, and the split falls naturally along the
   seams the extraction creates: `reader`, `panes`, `popups`, `panel`, `links`,
   `chrome`. Doing it *after* P0–P1 means you're splitting a smaller, logic-light
   file. *Effort: medium. Risk: low (mechanical).*

### P2 — make parity a machine guarantee, not prose

6. **Cross-shell contract tests.** Extend the existing golden-wire samples in
   [crates/ffi/src/tests.rs](crates/ffi/src/tests.rs) to cover every new
   view-model; add a compile-checked enum of block kinds so a shell that forgets
   to handle one fails to build rather than silently dropping content.
7. **Shrink the manifest's job.** Once view-models own the content model,
   [docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md) becomes *"what the
   endpoints mean"* (a few pages) instead of *"how to re-implement each feature"*
   (the current 300 lines). The parity contract moves from English into the
   type system.

---

## Smaller notes

- **`crates/ffi/src/lib.rs` (1761 lines) + `wire.rs` (862)** are large but under
  the 3k rule and are *the ABI* — fine. The content-model work will grow
  `wire.rs`; that's acceptable (it's declarative schema).
- **`RwLock<StudyData>` + reload-after-write** ([lib.rs:117](crates/ffi/src/lib.rs#L117))
  is a sound choice for concurrent read + off-thread authoring. No change needed.
- **Verify `VersePerLine` parity.** WinUI wires `verse_break` to the core layout
  ([ReaderView.cs:191-206](apps/windows/PureStudyWin/ReaderView.cs#L191-L206));
  confirm GTK exposes the same toggle or log it as a delta. (Symptomatic of the
  prose-only parity contract — exactly what P2 fixes.)
- **Sequencing with in-flight work.** The rendering-lens feature is mid-flight on
  this branch (`git status` shows modified `StudyPanel.cs` / `Wire.cs` /
  `main.rs`). Land that feature first, *then* start P0 with the warm-up (P0.3) so
  the extraction is done against a stable feature set.

---

## One-paragraph summary for future-me

The architecture is right and the reader proves it: put the hard, shared logic
in Rust and let native shells paint. The regression is that everything past the
reader — panel, popups, connectors, canon strip, routing — quietly kept its
logic in the shells and duplicated it GTK↔WinUI, guarded only by prose. Android
is the forcing function: consolidate now by emitting **view-models** (data +
fractional geometry + a typed content-block model) from the core so the third
shell, and every future feature, is *paint + route* instead of *re-derive*.
Start with the cheap `link_pairs`/`canon_segments` extraction to prove the
pattern, then the popup view-models, then the panel content model; split
`main.rs` along the seams that opens up; and replace the prose parity contract
with golden view-model tests and a compile-checked block enum.
