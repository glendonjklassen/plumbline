# Plumbline TODO — what is still open

Started as the v1.0.0 audit punch list (2026-07-29, an eight-agent
pre-release audit). Everything that shipped or was dropped has been cleared
out of here — git history and the release commits are the record. What is
left is only what is still open, and it is all **[FABLE]**: frontier
judgment on frozen-contract evolution, concurrency design, and cross-cutting
architecture where a wrong call is silent and permanent. Mostly short
design/decision sessions, not long ones.

File:line refs date from 2026-07-29 — re-verify before editing.

**Versioning, decided 2026-07-29 by Glendon:** everything ships under
incremental tags. When 1.0.0 happens is his call and his alone, so read any
"1.0.x" below as "a later increment", and **1.0.0 must not appear anywhere a
reader could read it as this release** (README's sideload block is guarded by
a test).

## E. Release mechanics — before `git tag v1.0.0`

- [ ] **[FABLE]** Hand-write the v1.0.0 release notes (`gh release create
  --notes-file`) — auto-generated notes are PR-title soup and the repo is the
  download page.
## F. Performance — web

- [ ] **[FABLE]** Dependency-aware invalidation design: the `authored` event should
  carry what changed; per-key/method epochs instead of one global `cacheEpoch`
  (today one write refetches the world and every fill re-runs every derived).
- [ ] **[FABLE]** Display-list transport: dropping `verseDisplay`/string-verse is a
  ~45% payload cut but bends the additive-only wire rule (engine+shell
  version-lock makes it arguable — decide); the structural fix is a
  typed-array/zero-copy list, which also obsoletes the proxy item above.
- [ ] **[FABLE]** Per-book pack split — quantified 2026-07-29: stage 1 is 99.3% one
  file (3.40 MB gz / 37.24 MB raw); split ⇒ first paint needs ~30 KB directory +
  one book, and it kills the ~150 MB transient boot peak. Engine-side per-chapter
  lazy decode already exists; touches data-prep, hydrate, manifest, `load_cache`,
  pin. Cross-cutting enough to design first.
## G. Performance — Android

- [ ] **[FABLE]** Replace the coarse `synchronized(engine)` monitor (49 sites) — the
  core is documented thread-safe for reads; a tap during a cold study build
  currently blocks the main thread for seconds (ANR class). Read-write lock or
  lock-free reads; concurrency semantics need care.
## H. Architecture consolidation (post-1.0 track)

- [ ] **[FABLE]** Wire-type codegen: emit Kotlin + TS from `wire.rs` via the existing
  `plumbline-bindgen` bin; make both decoders strict; CI drift gate on shapes, not
  just function names. Highest-leverage single change in the audit — would have
  caught all three live wire bugs.
- [ ] **[FABLE]** Config ownership: core-owned write API + clamps + history cap +
  church caps (three layers currently disagree on ranges; both shells rewrite the
  whole object wholesale).
- [ ] **[FABLE]** First-run prose → a core panel producer (the guide/about pattern) —
  the evangelistic copy is byte-identical in two languages today. Extends the
  closed block vocabulary with interactive blocks, so design first. Prerequisite
  work for German localization.
- [ ] **[FABLE]** wasm-only surface → negotiated capabilities: `defer_builds` (a
  correctness-critical engine mode) and `warm_step` into the C ABI instead of
  `cfg(target_arch)`; make `PLUMBLINE_WIRE_VERSION` a live handshake (it's
  currently emitted and read by nothing).

## Future (not this release)

- **Other-language support, starting with German.** Blockers mapped by the
  2026-07-29 architecture audit: `TOKENIZATION_VERSION` is one global const
  conflating translation+tokenizer; `data/kjv.jsonl` is the data-home marker;
  `export::EDITION` compiled in; fixed 66-book canon table; and — biggest — user
  data (tags/notes/cards) carries no translation identifier. The serde-flatten,
  stable-id, wire-codegen, and first-run-into-core items above all directly reduce
  the cost of this. Multi-language support includes hymnal as much as possible.
