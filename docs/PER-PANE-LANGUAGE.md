# Per-pane text language

**Status (2026-08-17, v0.54.0): the engine seam is in and tested; neither shell
uses it yet.** This file is the decision and the remaining work, so the next
session starts from a plan rather than a survey.

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

## What is left, per shell

**Web** (`apps/web`) — the larger half, because the engine lives in one worker:

1. `engine.worker.ts` holds `StudyEngine` singly. It needs a map keyed by
   language, opened on demand, and the RPC surface needs an optional language on
   the text-facing calls (layout, verse, chapter counts, word study). Keep the
   authoring calls unqualified — they are the primary's alone.
2. The alt corpus is an optional pack file (`corpus:<code>` role, `stage:
   "optional"`), already fetched on demand for a language SWITCH. A per-pane
   language wants the same download on a per-pane pick, with the depot
   read-through doing the work (`engine/depot.ts`) and boot never blocked on it.
3. `PaneState` gains `lang`; `openPanes` in the config gains it too — additive,
   absent meaning "the reader's language", per the frozen-wire rule.
4. A per-pane control. The pane header is where it belongs (the ✚/✕ pane
   controls already live there); a chip showing the corpus label from the
   language row (`KJV` / `Luther` / `Reina-Valera`) rather than a flag.
5. **The one-worker rule still applies**: opening a corpus is a long synchronous
   engine call, so it must be chunked or it starves every layout and tap RPC
   queued behind it (CLAUDE.md §UI testing). Boot-responsiveness regression test
   territory.

**Android** (`apps/android`): every corpus is already bundled in the APK, so
there is no download to arrange — a second engine and the pane's own handle.
One pane on a phone, two on a fold opened flat (`FoldMode.kt`), so the control
belongs on the pane header there too.

**Both**: the study panel must say which TEXT a word study came from when it is
not the reader's own language, or a German definition under an English pane is
unexplained.

## Tests worth writing first

- Two engines live at once answer independently (shipped:
  `a_second_engine_opens_a_named_language_and_never_substitutes_english`).
- A tag authored on the primary appears in the alt pane's word study after the
  shell's reload — the staleness contract, stated as a test.
- A pane whose language has no downloaded text shows the offer, never English.
- The boot-responsiveness budget survives opening a second corpus (derived from
  the machine's own measured chunk cost, never a fixed millisecond ceiling).
