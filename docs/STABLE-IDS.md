# Stable ids for threads, tags and weaves

**Decided 2026-07-29: yes — ids ship as additive fields, once the
unknown-field-preservation work has landed.**

**Implemented 2026-07-30 (v0.36.0).** Both fields are live on `Thread`, `Tag` and
`Weave`; the flatten work they wait on landed in the same unreleased tag, which
is what the ordering below actually required. Step 3 (id-carrying wire links) is
still future work — see "Sequencing".

## Why this is decided now

The architecture table in CLAUDE.md already made this call when the sync SaaS
was cancelled: *"Keep the data-model discipline it imposed anyway (stable ids,
no host-local assumptions, exportable single-file JSON)."* The 2026-07-29 audit
found the discipline did not survive in three of the four user formats:

- `Thread`, `Tag` and `Weave` are keyed by **name**; the filename is
  `slug(name)`. A rename is a new file and a lost identity.
- No container carries an `updated` stamp (only `created`), so "which copy is
  newer" is unanswerable across two devices or a backup import.
- The wire addresses all three by **ordinal into a name-sorted list**
  (`thread:2`), which shifts under any add/rename/delete and is meaningless
  outside the session that produced it.

Only `notes/` got it right (refKey as a natural key, plus `updated`). Memory
cards and reading files also have natural keys (refKey, book) and need nothing.

The timing is the point: sideloaded v1.0 APKs never auto-update, so whatever
v1.0 does to fields it doesn't understand is frozen the day it ships. The
sequencing below exists to survive that.

## The design

**New fields, both additive, on Thread, Tag and Weave:**

- `id` — 32 lowercase hex chars (128 random bits). Generated once at creation;
  a file without one (any v1.0 artifact) is assigned one lazily on its first
  save by an id-aware build. Never derived from the name.
- `updated` — same wire timestamp format as `created`
  (`YYYY-MM-DDThh:mm:ssZ`, UTC), bumped on every mutating save.

**Identity semantics.** The id is the identity; the name is a label. Rename
keeps the id (the slug filename may change). If a loader ever sees two files
carrying the same id — the rename-artifact case — it keeps the one with the
newer `updated` in memory and removes the stale *file* only on the next
explicit save of that object, through the atomic store. Load never deletes;
that discipline (`thread.rs`'s refuse-to-clobber guard) extends to this.

**Inner identity needs no ids.** Thread entries are `(refKey, added)`; weave
links are the undirected `(a, b)` pair. Both are natural keys already.

**Wire evolution — additive only.** Wire structs gain `id` alongside the
existing ordinal `index`; panel links gain id-carrying forms. Shells prefer the
id when present. The ordinals stay on the wire until the wire-codegen work
(TODO §H) exists to retire them safely behind a `PLUMBLINE_WIRE_VERSION` bump.

**What this buys, and what it doesn't.** Per-object last-writer-wins by
`updated` becomes possible across backup imports and (if it ever exists)
multi-device sync. No sync protocol is being built; this keeps the door from
rusting shut while the formats are young.

## Sequencing — order matters

The constraint is the ORDER, not any particular version number: this was written
when the next tag was expected to be 1.0.0, and it is 0.36.0. "The first shipped
release" is what "v1.0" meant throughout.

1. **Before the tag — DONE:** the `#[serde(flatten)]` unknown-field preservation
   lands in every user-format Repr (TODO §B). Without it, a build round-tripping
   a *later* build's file would strip `id` and `updated` — the fields would be
   unreliable for ever after. This is the only hard prerequisite.
2. **DONE, same tag (0.36.0):** `id` + `updated` on the three containers, lazy
   assignment on save, duplicate-id resolution as above. Landing it alongside
   step 1 rather than an increment later is safe *because* step 1 is in the same
   unreleased tag — no shipped build has ever seen a file carrying these fields,
   so there is none to strip them.
3. **Post-codegen — NOT DONE:** id-carrying wire links; ordinals retired. The
   wire still addresses all three by ordinal, and shells still use it.

## Test requirements — all shipped 2026-07-30

- Round-trip: load → save preserves `id` and everything else, on all three
  types, including through a build that predates any *later* additive field
  (the flatten test's job, extended to `id`).
- A file with no `id` gains one on first save and loses nothing —
  `a_tag_from_before_ids_gains_one_on_first_save_and_loses_nothing`.
- Rename keeps the id — `a_tag_written_under_a_new_name_keeps_its_id`. Note
  **there is no rename endpoint yet**, on either shell; the test pins the
  mechanism a rename will use. "The old slug's file is cleaned up only via save"
  is therefore not implemented: nothing can create the artifact. Load deleting
  nothing is what matters today, and that is asserted.
- Duplicate ids: newest `updated` wins in memory; load deletes nothing —
  `duplicate_ids_keep_the_newer_and_load_deletes_nothing`, on all three types,
  with a pair whose newer copy sorts first AND a pair whose newer copy sorts
  last (one pair alone cannot tell "newest" from "first").
- Where `updated` comes from: the shells send a stamp with the mutations that
  create something, and the engine reads its own clock for the ones that don't
  (`now_stamp`, the only clock in the product's Rust — the core stays pure). In
  the browser that clock is the WASI shim's `clock_time_get`, and a shim without
  it would stamp 1970 SILENTLY, so it is proven in a real browser:
  `apps/web/e2e/stable-ids.spec.ts`.

## Out of scope

Sync itself. Ordinal removal (waits on codegen). Ids for notes, memory cards,
reading files or config — their keys are natural and already stable.
