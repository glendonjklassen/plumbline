# Per-pane text language

**Status (2026-08-17, v0.55.0): SHIPPED ON THE WEB. Android is the remaining
half.** The engine seam, the config field, the worker's per-language engines and
the pane control are all in, with `apps/web/e2e/pane-language.spec.ts` holding
them. What follows is the design as built, then what Android still needs.

## What it is

A reading pane picks its own TEXT language: German beside English for John 3,
without the UI language moving. The maintainer's ask, and the two halves of it
that were settled up front:

- **Full study per pane.** Tapping a word in the German pane gives German word
  study, from that language's own Strong's dictionary. Not a read-only parallel
  column — a pane whose words cannot be tapped is a screenshot.
- **The reader's own data is SHARED.** Notes, tags, weaves, threads and reading
  progress belong to the reader, not to a text. A tag made while reading Luther
  is on the verse, and the English pane shows it.
- **UI language is a separate setting** and never moves when a pane's text does.

## Why the shared data is free

Every text this app ships sits at the KJV's own verse addresses — each source
was mapped to KJV numbering offline (manifest §The other Bibles). A `refKey`
therefore means ONE verse in all of them, and there is no mapping layer to
write, no migration, and no chance of a note landing on the wrong verse.
This is the fact the whole feature rests on; if a text ever ships at its own
numbering, `core::versification` is where that conversation starts.

## The architecture: a second ENGINE, not a second corpus inside one

`plumbline_engine_open_lang(home, lang)` (crates/ffi, shipped and tested) opens
a second engine on another language's text from the SAME home.

Considered and rejected: one engine holding a map of alt corpora, with
`_in(lang, …)` twins of every text endpoint. That needs a parallel
`PanelSource` implementation and a new ABI endpoint per feature — new surface
for word study, layout, verse text, chapter counts, each with its own drift
risk — to avoid a duplicate study store that costs little in practice.

The second engine reuses EVERY existing endpoint unchanged: the shell picks
which handle to call and the ABI does not grow per feature. Two consequences to
hold on to:

- **The alt handle's study view is a snapshot.** It reads the same files, so it
  opens with the reader's data — but an authoring write goes through the
  primary handle, and the alt does not hear about it. The shell must call
  `plumbline_engine_load_core_data` on every alt handle after a write, exactly
  as it re-fetches panels today.
- **Authoring stays on the primary handle**, always. Two writers over one home
  is a corruption story the atomic store should never be asked to survive.

`plumbline_engine_open_lang` deliberately does NOT fall back to English when
the text is missing (`plumbline_engine_open` does, because a reader is owed a
Bible). A pane labelled Deutsch must not quietly paint the KJV; the error is
the shell's cue to offer the download.

## How the web does it (built)

- `engine.worker.ts` keeps `altEngines`, a `StudyEngine` per language code, and
  a `callIn` op routes a read to one of them. **The method name is unchanged**,
  so every existing read works on any handle and per-pane language costs no
  per-feature RPC. Authoring never routes — it is the primary's alone.
- **The language is part of the turn-cache key.** Without it a German pane at
  the same width serves the English pane's cached display list: the right
  geometry for the wrong Bible, and the mutation that proves the test.
- **No reload.** The settings language switch reloads the app (the TOC is pinned
  for the session); a pane switch must not, because the pane beside it is being
  read. Progress shows *in the pane*.
- `openPaneLang` downloads through the same pack path the settings switch uses,
  then opens. Opening reads the **idxcache**, not the JSONL — ~150 ms locally,
  so it does not starve the one worker thread with a canon-wide parse.
- The panel views whose content is scripture (`wordStudy`, `codeStudy`,
  `concordance`, `renderingConcordance`) carry the language, and the link router
  propagates it: a German word study's "other occurrences" lists German verses.
- **Release and repair.** A language no pane reads is freed
  (`releaseLangs`), and the home's copy of its files is evicted after the engine
  has parsed them (~33 MB each). That combination means a language re-opened
  later in the session finds its file gone — so the open RETRIES through the
  supply path, and the depot answers without the network. The cold path is the
  repair, exactly as it is for the core pack.

### What it costs, measured

`e2e/pane-language.spec.ts` prints this on every run rather than asserting a
number, so a regression is visible without a brittle budget:

| Texts open | wasm heap |
|---|---|
| 1 (English only) | ~104 MB |
| 3 (English + Luther + Reina-Valera) | ~226 MB |

About **61 MB per extra Bible** — its idxcache, retained by the open corpus,
plus the chapters actually read. Three at once is the ceiling (the web caps at
three panes) and it works; on a low-end phone it is the number to watch, which
is why unused languages are released rather than kept.

## What is left: Android

Every corpus is already bundled in the APK, so there is no download to arrange
— a second engine (`plumbline_engine_open_lang`, already in the Kotlin binding)
and the pane's own handle. One pane on a phone, two on a fold opened flat
(`FoldMode.kt`), so the control belongs on the pane header there too. The web's
shape ports directly: language in the layout key, study routed to the pane's
engine, release what no pane reads.

**Both**: the study panel must say which TEXT a word study came from when it is
not the reader's own language, or a German definition under an English pane is
unexplained.

## Tests

Shipped: `crates/ffi` `a_second_engine_opens_a_named_language_and_never_substitutes_english`,
and `apps/web/e2e/pane-language.spec.ts` — German beside English with the UI
unmoved, study following the pane's text, the memory measurement, and release +
re-open.

Two traps this feature walked into, recorded because they will recur on Android:

1. **A helper that does not wait proves nothing.** `setPaneBible` first returned
   on the menu click, so the test tapped a word before the German text had
   replaced the English — and the assertions passed against unswitched text.
   It now polls the pane's own `lang`.
2. **Comparing two studies before and after a switch is not a language test.**
   Changing the text moves the words, so the same coordinate lands on a
   different token and the two answers differ even when the language is dropped
   entirely. The test now pins the two links separately: the tap carries the
   language into the panel view, and the same verse+token rendered with and
   without it paints differently.

Still worth writing: a tag authored on the primary appearing in the alt pane's
word study (the `refreshAlts` contract), and a boot-responsiveness check that
opening a second corpus does not starve queued layout/tap RPCs.
