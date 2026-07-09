# pure-study — Rust rebuild plan

A ground-up Rust rebuild of the Haskell `overlay` app (a 1769 KJV reader +
study tool). This document is the map: the decisions made, the architecture,
how it maps onto the old code, and what is done vs. pending.

> Reference source of truth: `../overlay` (Haskell). **Read-only** — we port
> from it, never modify it.

## Decisions (locked with Glendon, 2026-07-08)

| # | Decision | Choice |
|---|----------|--------|
| 1 | UI strategy | **Native per platform** over a shared Rust core (Jetpack Compose on Android, WinUI on Windows, **GTK4 on Linux first**). |
| 2 | Build order | **Desktop first** — GTK4 on Arch Linux for a fast native dev loop; Windows/Android follow over the same core. |
| 3 | Data delivery | **Bundle core, download R&D** — KJV + Strong's ship in-app (offline immediately); heavy R&D artifacts (embeddings, morphology, cross-refs, trust) are optional downloadable "packs". |
| 4 | R&D default | **Off + guided first-run** — first launch asks *Simple reader* vs *Full study*; casual users never see the complexity. |
| — | Patches / signed rules | **Dropped.** Not ported. (The Ed25519 point-patch/rule overlay from overlay is gone.) |
| — | Future | A paid cross-device **sync SaaS**; the pack/download server is its seed. Data model must not block it (stable ids, no host-local assumptions). |

## Architecture

```
Rust core (pure, headless, fully testable on Linux)
  ├─ crates/core     domain: canon, references, corpus/text, Strong's, search, weaves, threads
  ├─ crates/rnd      OPTIONAL, feature-gated: embeddings, morphology, keyness, trust/witness
  ├─ crates/layout   text shaping + line layout + per-word HIT-TESTING → a display list
  └─ crates/ffi      one C ABI surface → UniFFI (Kotlin/Android) + csbindgen (C#/WinUI)

Thin native UIs (paint the display list, forward input coords back to core)
  ├─ apps/desktop    GTK4 + libadwaita (Linux) — the first shell
  ├─ apps/windows    WinUI 3 (C#)      — later
  └─ apps/android    Jetpack Compose   — later
```

### The load-bearing idea: layout + hit-testing live in the core

overlay's reader is a custom Monomer widget (`ReaderView.hs`) that lays out and
hit-tests **every word individually** — Ctrl+click Strong's, hover cards, and
the cross-pane weave connectors all ride on that per-word layout. Reimplementing
that in Kotlin *and* C# *and* GTK would be three copies of the hardest code.

Instead the **core owns layout**: given a chapter + width + font metrics, it
produces a *display list* — positioned glyph runs plus a table of tappable word
rectangles (each carrying its `VRef` + token index + Strong's refs). Each native
UI then only:
1. paints the display list (rectangles, glyph runs, rule/underline marks), and
2. sends tap/hover `(x, y)` back; the core hit-tests and returns what was hit.

This keeps the native shells genuinely *thin* and the study logic in one place.
Shaping uses `cosmic-text` (+ `rustybuzz`/`swash`), which also gives us RTL
Hebrew/Greek for the (optional) morphology layer for free.

## Data formats (frozen — carried verbatim from overlay)

- **`kjv.jsonl`** — line 1 is a header `{format, tokenization, source, verses}`;
  every subsequent line is a verse `{"b":OSIS,"c":ch,"v":vs,"t":[token,...]}`.
  A **token** is a positional array `[pre, word, post, [strongs], flags]`.
  `flags` is a bitfield: `1` added (KJV italics), `2` divine name, `4` title
  (psalm superscription), `8` paragraph mark (¶) precedes the word.
- **`strongs.json`** — one minified object, `"H7225" → {lemma?, xlit?, pron?,
  derivation?, strongs_def?, kjv_def?}` (14,197 entries).
- **`kjv-notes.jsonl`** — `{"b","c","v","note"}` (1769 translators' margin notes).
- **weave** — `{format:"overlay-weave-v2", name, kind, tokenization, notes,
  notesSource, created, approved, links:[{a:"Gen 1:7", b:..., label?, approved?,
  spanA?, spanB?}]}`. A weave is an undirected graph of verse↔verse links.
- **`refKey`** — the frozen compact ref string, `"Gen 1:7"` (OSIS book id).
- **tokenization version** — `kjv1769-tok2`; loaders refuse a version mismatch.

The **tokenizer** (SWORD `mod2imp` → `overlay-import`) stays an **offline
data-prep step** — the Rust runtime only *consumes* `kjv.jsonl`, so we carry
the version stamp check but not the tokenizer itself (that lives in `data-prep/`
when we port the pipeline).

## Module mapping (overlay Haskell → pure-study Rust)

| overlay | pure-study | status |
|---------|-----------|--------|
| `Canon.hs` | `core::canon` | ✅ |
| `Corpus.hs` (VRef, Token, Verse, Corpus, refKey) | `core::reference`, `core::corpus` | ✅ |
| `Strongs.hs` | `core::strongs` | ✅ |
| `Search.hs` | `core::search` | ✅ (text/phrase/ref/bare-Strong's; morphology form-preds gated to `rnd`) |
| `Weave.hs` | `core::weave` | ✅ |
| `Thread.hs` | `core::thread` | ✅ (read side + serde) |
| `Tag.hs` | `core::tag` | ✅ (read side + membership) |
| `Refs.hs` (display names, canon segments) | `core::reference` | ✅ (segments) |
| `ReaderView.hs` (custom layout widget) | `layout` | ✅ (algorithm + hit-test, unit-tested) |
| — (new: the one C ABI over core+layout) | `ffi` | ✅ (opaque handles, callback layout, JSON; C/C#/Kotlin bindings) |
| `CrossRef.hs` (TSK topical tier) | `core::crossref` | ✅ (pure parser; no ML) |
| `Bridge.hs` (OT↔NT etymology) | `rnd::bridge` (feature) | ✅ (etymology layer; rendering/trust deferred) |
| `Embed.hs` (concept vectors, SIF) | `rnd::embed` (feature) | ✅ loader + neighbours + cross + SIF "verses like this" |
| `Morph.hs` (OSHM/Robinson parse) | `rnd::morph` (feature) | ✅ consuming side (parse + render + sidecar); offline projection stays Python |
| `Concept*`, `Witness`, `Burst`, `Quotation` | `rnd` (optional) | ⏳ later (fused-source/quotation tiers; need more hydrated inputs) |
| `Patch.hs`, `Rule.hs` | — | ❌ dropped by decision |
| `UI/Panels/Events/Home/Startup` (Monomer) | `apps/desktop` (GTK), + WinUI/Compose over `ffi` | 🔨 |
| `ml/*.py`, `pipelines/*.py` | `data-prep/` (offline Python; pack documented) | ✅ documented ([data-prep/README.md](data-prep/README.md)); generators stay in the overlay checkout |

## Overnight scope (this session)

Foundational core + a first visible desktop render. See git log on branch
`rust-rewrite` and `PROGRESS.md` for exactly where it stopped.
