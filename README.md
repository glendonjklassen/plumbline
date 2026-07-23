# pure-study

A KJV-only Bible-study tool: a clean parallel-passage reader with an optional
"Full study" tier of Strong's, morphology, cross-references, and corpus
analytics. Built in Rust; successor to *overlay*. Everything runs locally and
offline — the repo ships the complete data pack, so a clone is a working app.

![Genesis 15 and Romans 4 side by side, joined by the "Abraham believed God"
weave's connector lines](assets/readme/reader-weaves.png)

## Install & run

### Windows

Download the zip for your architecture (arm64 / x64 / x86) from the
[Releases page](https://github.com/glendonjklassen/pure-study/releases) —
self-contained, data pack bundled. Unzip anywhere and run `PureStudyWin.exe`;
no installer, no runtime to install. (The build is unsigned for now, so
SmartScreen may ask once — "More info → Run anyway".)

### Linux (from source)

The GUI is GTK4 + libadwaita. You need the Rust toolchain
([rustup](https://rustup.rs)) and the GTK development packages:

```sh
# Arch:            sudo pacman -S gtk4 libadwaita
# Debian/Ubuntu:   sudo apt install libgtk-4-dev libadwaita-1-dev build-essential
# Fedora:          sudo dnf install gtk4-devel libadwaita-devel

git clone https://github.com/glendonjklassen/pure-study.git
cd pure-study
cargo run --release -p pure-desktop
```

That's it — the checkout itself is a hydrated data home (KJV text, Strong's,
weaves, the full analytics pack), so everything lights up on first launch.
There is no Linux package yet (AppImage/flatpak/AUR are planned); on Linux,
building from source is currently the only path.

To run the app from anywhere (not just the checkout), seed a per-user home
once — `~/.local/share/pure-study` on Linux — and the binary will find it:

```sh
cargo run --release -p pure-hydrate -- copy --from . --to ~/.local/share/pure-study
```

## First run

You'll be asked **Simple reader** or **Full study**:

- **Simple** is just the text: panes, navigation, search, margin notes.
- **Full study** adds the whole study surface — Strong's word study, the
  analytics tiers, weave authoring, threads, tags.

The choice is saved and can be flipped any time with the button in the header.
The reader reopens wherever you left off. See **[docs/GUIDE.md](docs/GUIDE.md)**
for the full tour — search syntax, the study panel explained tier by tier,
weaves, the constellation, threads and tags.

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

Yours to back up: `weaves/`, `threads/`, `tags/`, `patches/` in the data home,
plus the config (`~/.config/pure-study/config.json`). The rest (`data/`,
`bridge/`, `*.idxcache`) is the shipped/regenerable pack. All writes are
atomic (temp → fsync → rename), on every platform.

## Limitations, honestly

- **KJV-only, by design.** The analytics ride the 1769 tokenization end to end.
- **Linux and Windows today.** The GTK (Linux) and WinUI (Windows) shells are
  at feature parity over the same Rust core
  ([docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md) is the parity
  contract); the Android (Compose) shell is next, macOS much later — see
  [TODO.md](TODO.md).
- **No sync.** One machine, one home; copy the authored dirs to move.
- **Grammar search** (`tense:aorist`-style form predicates) is a placeholder —
  word/phrase/reference/Strong's-code search all work (see the guide).
- Cross-testament **quotation detection** is not ported (its curated output
  ships in the bridge data); the suggested-parallels *generator* is offline
  tooling — the reader consumes its results.

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
| `apps/desktop` | The GTK4 + libadwaita shell (Linux) |
| `apps/windows` | The WinUI 3 + Win2D shell (Windows) — see [its README](apps/windows/PureStudyWin/README.md) |

```sh
cargo test -p pure-core -p pure-layout -p pure-rnd -p pure-ffi -p pure-hydrate
cargo test -p pure-rnd --features "bridge embeddings morphology concept"
```

The five portable crates are dependency-light pure Rust and build on Linux,
macOS, and Windows including ARM64 (`aarch64-pc-windows-msvc` — on the ARM box:
VS Build Tools with the **C++ ARM64/ARM64EC build tools** component, then
`cargo build --release -p pure-ffi` → `pure_ffi.dll` + the committed C header /
C# P/Invoke shim; without that component rustc silently falls back to whatever
`link.exe` is on PATH and fails cryptically). CI runs the portable tests, the
R&D-feature tests, an FFI binding-drift guard, and Windows x86_64 + ARM64
cross-builds of the C ABI on every push. The offline pipeline that produced
the data pack is documented in [data-prep/README.md](data-prep/README.md); the
porting history (from the Haskell *overlay*, 2026-07) lives in the git log.

### Architecture

Decisions locked 2026-07-08, still in force:

| # | Decision | Choice |
|---|----------|--------|
| 1 | UI strategy | **Native shell per platform** over a shared Rust core — GTK4 (Linux), WinUI 3 (Windows), Jetpack Compose (Android); macOS later. |
| 2 | Build order | Desktop first (GTK4), then Windows, then Android over the same core. |
| 3 | Data delivery | **Bundle core, download R&D** — KJV + Strong's ship in-app; heavy analytics artifacts are optional packs. |
| 4 | R&D default | **Off + guided first-run** — first launch asks *Simple reader* vs *Full study*; casual users never see the complexity. |
| — | Patches / signed rules | Dropped — overlay's Ed25519 point-patch/rule layer was not ported. |
| — | Future | A paid cross-device **sync SaaS**; the data model must not block it (stable ids, no host-local assumptions). |

```
Rust core (pure, headless, fully testable)
  ├─ crates/core     domain: canon, references, corpus, Strong's, search, weaves, threads
  ├─ crates/rnd      OPTIONAL, feature-gated analytics
  ├─ crates/layout   text layout + per-word HIT-TESTING → a display list
  └─ crates/ffi      one C ABI surface → C#/WinUI + Kotlin/Android bindings

Thin native shells (paint the display list, forward input coords back to core)
  ├─ apps/desktop    GTK4 + libadwaita (Linux)
  ├─ apps/windows    WinUI 3 (C#)
  └─ apps/android    Jetpack Compose — next up
```

The load-bearing idea: **layout and hit-testing live in the core.** Given a
chapter + width + font metrics (via an injected measure callback — Pango on
GTK, Win2D on Windows), the core produces a *display list*: positioned glyph
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
