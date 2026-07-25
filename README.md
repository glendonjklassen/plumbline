# pure-study

A KJV-only Bible-study tool: a clean parallel-passage reader with an optional
"Full study" tier of Strong's, morphology, cross-references, and corpus
analytics. Built in Rust; successor to *overlay*. Everything runs locally and
offline — the repo ships the complete data pack, so a clone is a working app.

![Genesis 15 and Romans 4 side by side, joined by the "Abraham believed God"
weave's connector lines](assets/readme/reader-weaves.png)

## Install & run

### Web (any device)

The app is a PWA — a hosted link is coming; until then, run it locally:

```sh
git clone https://github.com/glendonjklassen/pure-study.git
cd pure-study/apps/web
npm install && npm run pack:data
rustup target add wasm32-wasip1
cargo build -p pure-ffi --release --target wasm32-wasip1 && npm run pack:wasm
npm run build && npm run preview   # → http://localhost:4173
```

Everything runs in your browser — the engine is the same Rust core compiled
to WebAssembly, your study data lives in browser storage, and the app works
offline after the first visit (installable as an app from the address bar).

### Android

Download the APK from the
[Releases page](https://github.com/glendonjklassen/pure-study/releases)
(arm64-v8a + x86_64, signed; no Play Store, no Google services required).

To run the app from anywhere (not just the checkout), seed a per-user home
once — `~/.local/share/pure-study` on Linux — and the binary will find it:

```sh
cargo run --release -p pure-hydrate -- copy --from . --to ~/.local/share/pure-study
```

## Getting started (60 seconds)

First launch asks which **analysis layers** you want beside the text — the
scholars' tier (renderings, word grammar, cross-references) and the machine
tier (similar concepts, verses-like-this, concept maps). Both are on by
default and switchable any time in **Settings**; the text and your own notes,
tags, and threads are always on.

Then:

1. **Read.** Tap the passage button (`John 3 ▾`) for the book → chapter →
   verse navigator, swipe (or `←`/`→`) to step chapters. The reader reopens
   exactly where you left off — mid-chapter included.
2. **Tap a word** (double-click on desktop) — the study pane opens: your
   note first, then the dictionary entry and whichever analysis tiers you
   keep on. Every claim is marked with its provenance (✝ the text ·
   † scholarship · ≈ machine).
3. **Long-press a verse** (right-click on desktop) — copy, share, note,
   highlight, **tag**, add to a thread, or memorize it.
4. **Tag as you go, weave later.** Tag passages by topic ("Rapture") over
   weeks; open the tag and hit **⇔ make weave** to chain them through the
   canon. Point two panes at linked passages and the connectors draw
   themselves.
5. **≡ menu** holds the rest: Memorize (spaced repetition), Explore (all the
   study tools, described), History, Present (hand-the-phone-across mode),
   the guide, and Settings — including **backup to a zip** that restores on
   any device.

See **[docs/GUIDE.md](docs/GUIDE.md)** for the full tour — search syntax, the
study panel tier by tier, weaves, the constellation, threads and tags.

## Shortcuts

The reading pane holds focus (click it if a dropdown steals it):

| Key | Action |
|-----|--------|
| `Up` / `Down` | scroll a few lines |
| `PageUp` / `PageDown` / `Space` | scroll nearly a page |
| `Home` / `End` | chapter start / end |
| `Left` / `Right` (or `[` / `]`) | step chapters, rolling across book boundaries |
| **`Shift`** + wheel / `Up` / `Down` / `PageUp` / `PageDown` / `Space` | **lock every pane together** (parallel reading) |
| `Ctrl` + wheel, `Ctrl` `+` / `-` | zoom the body text · `Ctrl 0` resets |
| `Ctrl`+click a word (or double-click) | open its Strong's study panel |
| `Esc` | close the study panel / any popup (clicking outside a popup also closes it) |

## Weaves — parallel passages

A **weave** ties parallel passages together (a Gospel harmony, a prophecy and
its fulfillment, an OT verse and the NT that quotes it). Links are **ambient**:
point two panes at parallel passages and any weave connecting them draws its
connector lines across the gap — no mode to enter. A verse scrolled out of view
leaves its connector pinned at the pane edge as a hint.

- **Map** — book-to-book weave density as chord ribbons.
- **Constellation** — the whole weave library, one weave per labelled lane on
  the canon backbone; page with `‹ ›` (or `Left`/`Right`), **pin** a lane
  (click its `▪`) to hold it while paging others past it, click a node to jump
  there, an edge to open the weave.

![The constellation: weave lanes over the canon backbone](assets/readme/constellation.png)

> [!NOTE]
> The weaves shipped in this repo began life as **AI-generated study aids**.
> Each records an `approved` flag, surfaced in the reader; approving one (from
> its compare card) is how a parallel graduates from a study prompt to
> something you've checked against the text yourself.

## Your data

Everything lives under one **data home** — the first of: `$PURE_STUDY_HOME` /
`$OVERLAY_HOME`, a directory tree containing `data/kjv.jsonl` (this checkout
counts), the executable's directory, or the per-user data dir
(`~/.local/share/pure-study` on Linux, `%APPDATA%\pure-study` on Windows).

Yours to back up: `weaves/`, `threads/`, `tags/`, `notes/`, `memory/`, and
the config. **Settings → Back up (.zip)** exports exactly that from either
app, and **Restore from backup…** loads it on any device — the archive layout
is shared, so a phone backup restores in the browser and vice versa. All
writes are atomic (temp → fsync → rename), on every platform.

## Data provenance

The KJV text (public domain) comes via eBible.org's SWORD module; Strong's via
Open Scriptures (CC-BY-SA); morphology from OSHB (CC-BY 4.0) and Robinson's
public-domain Textus Receptus tagging; cross-references from the TSK via
openbible.info. Full credits and licenses: **[BIBLIOGRAPHY.md](BIBLIOGRAPHY.md)**.
Scripture renders in EB Garamond (OFL, bundled).

## For developers

| Crate | What it is |
|-------|------------|
| `crates/core` | Pure domain: corpus, Strong's, search, weaves, tags, config, atomic store |
| `crates/layout` | Greedy line-breaker + hit regions (measures via callback) |
| `crates/rnd` | Feature-gated analytics: bridge, embeddings, morphology, keyness, witness, concept |
| `crates/ffi` | The single flat C ABI for native shells (cdylib) — see [crates/ffi/README.md](crates/ffi/README.md) |
| `crates/hydrate` | `pure-hydrate` CLI: copy/verify the data pack into a home |
| `apps/android` | The Compose shell (Android) — the UX gold standard |
| `apps/web` | The PWA shell (Svelte + the core compiled to wasm32-wasip1) |

```sh
cargo test -p pure-core -p pure-layout -p pure-rnd -p pure-ffi -p pure-hydrate
cargo test -p pure-rnd --features "bridge embeddings morphology concept"
```

The five portable crates are dependency-light pure Rust and build anywhere,
including `wasm32-wasip1` (the web shell) and the Android NDK targets. CI runs
the portable tests, the R&D-feature tests, an FFI binding-drift guard, and
cross-builds of the C ABI on every push. The offline pipeline that produced
the data pack is documented in [data-prep/README.md](data-prep/README.md); the
porting history (from the Haskell *overlay*, 2026-07) lives in the git log.

### Architecture

Decisions locked 2026-07-08, still in force:

| # | Decision | Choice |
|---|----------|--------|
| 1 | UI strategy | **Native shell per platform** over a shared Rust core. Today: Jetpack Compose (Android, the UX gold standard) + a PWA (web) covering every desktop. The GTK/WinUI desktop shells were built first and retired 2026-07-25. |
| 2 | Build order | Desktop first (GTK4) → Windows → Android → web; the desktops then retired in favour of the PWA. |
| 3 | Data delivery | **Bundle core, download R&D** — KJV + Strong's ship in-app; heavy analytics artifacts are optional packs. |
| 4 | R&D default | **Guided first-run** — first launch picks the analysis tiers (scholars' / machine) with examples; the text and the reader's own data are always on (revised 2026-07-25 from the original Simple/Full split). |
| — | Patches / signed rules | Dropped — overlay's Ed25519 point-patch/rule layer was not ported. |
| — | Future | A paid cross-device **sync SaaS**; the data model must not block it (stable ids, no host-local assumptions). |

```
Rust core (pure, headless, fully testable)
  ├─ crates/core     domain: canon, references, corpus, Strong's, search, weaves, threads
  ├─ crates/rnd      OPTIONAL, feature-gated analytics
  ├─ crates/layout   text layout + per-word HIT-TESTING → a display list
  └─ crates/ffi      one C ABI surface → Kotlin/Android JNA + the wasm web binding

Thin native shells (paint the display list, forward input coords back to core)
  ├─ apps/android    Jetpack Compose — the UX gold standard
  └─ apps/web        Svelte PWA over the core compiled to wasm32-wasip1
```

The load-bearing idea: **layout and hit-testing live in the core.** Given a
chapter + width + font metrics (via an injected measure callback — android.graphics.Paint
on Android, canvas measureText on the web), the core produces a *display list*: positioned glyph
runs plus a table of tappable word rectangles, each carrying its verse ref,
token index, and Strong's refs. A shell only paints that list and sends tap /
hover `(x, y)` back for the core to hit-test. Word-level study features are
written once, and shells stay genuinely thin.

### Data formats (frozen — carried verbatim from overlay)

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

The **tokenizer** (SWORD `mod2imp` → `overlay-import`) stays an offline
data-prep step — the runtime only *consumes* `kjv.jsonl`, carrying the version
stamp check but not the tokenizer itself.
