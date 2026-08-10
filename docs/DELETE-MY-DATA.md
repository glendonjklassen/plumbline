# Erase my data — spec

**Status: specified 2026-07-29; implementation is an [opus] TODO item (§D).
Both shells, same change set (parity rule).**

The audit's finding: Settings offers Back up and Restore but no way to erase,
which is a conspicuous omission for an app handed to strangers on shared
devices. This spec exists because deletion is the one feature where an
imprecise implementation is itself a data-loss bug — the scope below is
exhaustive and closed, and the implementation must not improvise beyond it.

## What dies, exactly

| | Web (PWA) | Android |
|---|---|---|
| Authored files | IndexedDB `user` store — **every key** (tags/, threads/, weaves/, notes/, memory/, reading/, plans/, .config/) | `tags/ threads/ weaves/ notes/ memory/ reading/ plans/` under the app home, recursively |
| Settings & first-run state | (inside `.config/` above) | `.config/plumbline/` |
| Seed-once decisions | IndexedDB `cache` store: `meta:stockSeeded` and `meta:bundled`, **deleted BY KEY** — never `clear()` on that store (home.ts's rule; the store may hold other rebuildables) | the `.stock-seeded` marker file |
| Shell-local traces | every `localStorage` key with the `plumbline:` prefix | — |

## What survives, exactly

- **The depot** (Cache API: pack, wasm, shell) and Android's extracted `data/`
  + its `.data-v2` marker. These are the Bible text, not the reader's data —
  erasing must not break the offline promise or force a re-download/re-extract.
- The installed app itself, and any backup zips the reader exported.

Net effect: the app boots as a fresh install — guided first-run, stock set
re-seeded — without touching the network.

## Flow (both shells, same copy)

Settings gains a final section, visually separated, below Back up / Restore:

> **Erase my data**
> Removes every note, tag, thread, weave, memory card, reading record and
> setting from this device. The Bible text stays. This cannot be undone.
> [ Erase my data… ]

Tapping opens a confirm dialog that:

1. States the counts, so the reader confirms against reality, not a
   hypothetical: "This erases N notes, N tags, N threads, N weaves, N memory
   cards and your reading history."
2. Offers **Back up first (.zip)** inline — the same action as Settings.
3. Requires a deliberate second act: a checkbox — "I understand this cannot be
   undone" — that enables the destructive button **Erase everything**.
   (Type-to-confirm is hostile on phones; checkbox + enabled-button is the
   pattern both shells can share.)

## Mechanics

**Web** — reuse the restore choreography (`SettingsDialog.svelte`), which
already solved "nothing may write after the decision":

1. `s.restoring = true; await s.rpc.freeze()` — the debounced authoring
   persist and the dwell persist are now no-ops.
2. One readwrite transaction on `user`: `objectStore.clear()` (clearing ALL of
   `user` is the intent here, unlike the cache store).
3. One readwrite transaction on `cache`: delete `meta:stockSeeded`,
   `meta:bundled` by key.
4. Remove `localStorage` keys with the `plumbline:` prefix.
5. `location.reload()` — boots first-run, re-seeds stock from the pack.

If any step throws: reload anyway and report what happened on the other side —
a half-erased home must not keep accepting writes into a frozen session (the
same failure the restore path has, TODO §A).

**Android:**

1. Quiesce the engine (no authoring surface open; take the engine lock, or
   free the engine first — the erase happens from Settings, so nothing else
   is mid-call).
2. Delete the six user dirs recursively, then `.config/plumbline/`, then the
   `.stock-seeded` marker — content first, markers/config last (see ordering).
3. `activity.recreate()` — MainActivity's existing open path re-seeds stock
   and shows first-run, exactly as a fresh install.

**Ordering rule (both shells):** user content first, config + seed markers
last. A crash mid-erase must look like "some of my data is missing" (honest,
re-runnable), never like a fresh install with ghost files behind it — deleting
config first would boot the first-run flow over a home that still has notes.

**Atomicity honesty:** the web's step 2 is atomic per store; Android's is
per-file. That's acceptable for a user-initiated erase because re-running it
completes it, and the confirm dialog remains reachable after a partial run.

**Privacy honesty (About/dialog copy, one line):** this resets the app on this
device; it is not forensic erasure — the OS/browser may retain storage-level
artifacts (journals, snapshots) the app cannot reach, and copies you exported
or shared are yours to manage.

## Test requirements (ship with the implementation)

- **Web e2e:** author a note + tag + memory card + read a chapter; erase;
  assert the first-run chooser owns the screen, the `user` store is empty, the
  two meta keys are gone, `plumbline:` localStorage keys are gone; complete
  first-run and assert stock re-seeded (weaves > 20) and the authored note is
  gone — **with the network offline from the erase onward**, which pins "the
  depot survives" as behaviour rather than intention.
- **Android unit test:** the function that enumerates deletion targets never
  yields `data/`, `.data-v2`, or anything outside the home; and yields exactly
  the table above given a populated home.
- Mutation-test both, per working rules.
