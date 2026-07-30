# Plumbline TODO — v1.0.0 audit punch list (2026-07-29)

Source: the eight-agent pre-release audit (5 product/release passes + 3
architecture/perf passes). Every item carries the model it needs:

- **[FABLE]** — needs frontier judgment: frozen-contract evolution, concurrency
  design, destructive-path specs, or cross-cutting architecture where a wrong
  call is silent and permanent. These are mostly *short* design/decision
  sessions, not long ones.
- **[opus]** — the audit fully specified the fix; an Opus ultracode session can
  land it. Opus sessions: follow CLAUDE.md's rules (mutation-test every
  regression test; rebuild the wasm before the web suite when a crate changed).

File:line refs were verified 2026-07-29 against `69f137f` with a tidying thread
in flight — re-verify lines before editing.

**Versioning, decided 2026-07-29 by Glendon:** this whole catalogue ships under
the next INCREMENTAL tag (v0.36.0), not as 1.0.0. When 1.0.0 happens is his call
and his alone. So read "before the tag" below as "before the next tag" — the work
is still ordered the same way, and §A–E still come first — and read any "1.0.x"
as "a later increment". F–I can ride later increments (the PWA auto-updates).

## A. Data safety — before the tag

- [x] **[opus]** Android restore is a destructive in-place partial write: unzip to
  `.restore-tmp`, verify every entry, then temp+rename each file in; all-or-nothing
  (`ui/Backup.kt:78-100`). Never stream over the live home.
- [x] **[opus]** Web save failure is silent: `void persistUserData()` swallows
  QuotaExceeded while the UI reports success (`engine.worker.ts:620-629`,
  `engine/home.ts:230-241`). Propagate failure → `persistFailed` message → sticky
  toast + retry with backoff. Add an `{op:"flush"}` RPC called on
  pagehide/visibilitychange-hidden to close the 50 ms debounce window.
- [x] **[FABLE]** Multi-tab: whole-subtree last-writer-wins destroys the other tab's
  edits and resurrects deletions. DONE — design chosen: per-file content
  fingerprints in `engine/home.ts` (a tab writes only what it changed, deletes
  only what it removed; IndexedDB's per-store transaction serialisation makes a
  cross-tab lock unnecessary). Same-file edits stay last-writer-wins by design.
  Regression test: `e2e/multitab.spec.ts`.
- [x] **[opus]** `reading.rs:263-269` and `memory.rs:358` treat unparseable/future-format
  files as empty and then overwrite them. Copy the refuse-to-clobber guard from
  `thread.rs:196-199` into both.
- [x] **[opus]** Engine worker death hangs everything silently: add
  `onerror`/`onmessageerror` rejecting all `#waiting`, plus a ~60 s boot watchdog
  surfacing the existing error UI (`engine/worker-client.ts:82-98`).
- [x] **[opus]** Web zip restore: bounds-check `dataAt + csize` and verify CRC-32
  against the central directory in `zipRead` (`engine/zip.ts:104-124`).
- [x] **[opus]** A failed restore leaves the session frozen forever: on `idbApply`
  rejection, reload anyway (or un-freeze + clear `restoring`) with a blocking error
  (`SettingsDialog.svelte:62-70`).
- [x] **[opus]** Android note-save discards the engine's error and closes the sheet:
  `ui/VerseActions.kt:231-238`, `ui/Notes.kt:146`, `ui/Memorize.kt:387`. Surface the
  returned error, don't close on failure.
- [x] **[opus]** Android stock re-seed overwrites user-edited stock items (web
  preserves): `seedStock` must skip existing destinations, and `copyAsset` must
  write temp+rename (`MainActivity.kt:163-189`).
- [x] **[opus]** Web `backup()` is unguarded while `restore()` right below has
  try/catch + toast (`SettingsDialog.svelte:31-45`). Same shape.
- [x] **[opus]** Damaged config is replaced with defaults on next save (loses history/
  panes/church): rename to `config.json.bad` first (`config.rs:386-390`).
- [x] **[opus]** Stranded `.tmp` files in user dirs get persisted and shipped in backup
  zips: filter dot/`.tmp` names in `collectFiles` and `zipWrite`
  (`engine/home.ts:102-108`, `store.rs:55-65`).
- [x] **[opus]** Delete the dead duplicate `onAuthored` persistence handler
  (`engine/boot.ts:123-131`) — it never runs and traps whoever tunes the debounce.
- [ ] **[opus]** Stock toggle OFF deletes the reader's *edits* to stock-named items
  (both shells): hash-compare against pristine assets and keep anything that
  differs, or warn in the toggle copy (`engine/home.ts:222-225`,
  `MainActivity.kt:155-159`).

## B. Forward compatibility — before the tag (cannot retrofit; sideloaded APKs never auto-update)

- [x] **[opus]** "Additive evolution only" is not implemented: add
  `#[serde(flatten)] extra: Map<String, Value>` to every user-format Repr
  (usernote, tag, thread, weave, memory, reading, config) and round-trip it, with
  tests. Without this, any v1.1 field is stripped the first time a v1.0 build
  rewrites the file.
- [x] **[FABLE]** Stable-id decision: DECIDED 2026-07-29 — ids ship additively in
  the first 1.0.x, after the flatten item lands in v1.0. Full design (id/updated
  fields, identity semantics, wire evolution, sequencing, required tests) in
  [docs/STABLE-IDS.md](docs/STABLE-IDS.md).
- [ ] **[opus]** Implement stable ids per [docs/STABLE-IDS.md](docs/STABLE-IDS.md)
  (first 1.0.x, NOT before the flatten item ships in v1.0): `id` + `updated` on
  Thread/Tag/Weave, lazy assignment on save, duplicate-id resolution, the four
  required tests.

## C. Live wire-drift bugs

- [x] **[opus]** `akjvOverlay` is written by both shells and dropped by the core on
  every config save — the preference never survives a restart. Add the field to
  `Config` + `WireConfigState` (`wire.rs:1239-1287`, `config.rs:89-137`).
- [x] **[opus]** Android never renders tier marks / paragraph gaps: serde `rename_all`
  on `WireBlock` renames variants, not fields, so JSON carries `mark_glyph`/`top_gap`
  while `Wire.kt:584-589` expects camelCase. Add `rename_all_fields = "camelCase"`,
  update web `BlockList.svelte:16-20` to camelCase, bump `PLUMBLINE_WIRE_VERSION`,
  add golden key-set tests so this class can't recur silently.
- [x] **[opus]** ~~Every numbered book (1 John, 2 Chronicles…) dead-clicks in three web
  paths~~ — **FALSE POSITIVE**, closed 2026-07-29 (`51123f5`). Every OSIS book id is
  one word (`1John`, `2Chr`), so `.replace(" ", ":")` never missed; the spaced form is
  `display_name` and never feeds a refKey. The three sites were hardened to core's
  last-space rule anyway (a shell disagreeing with the frozen parser fails silently),
  with `e2e/numbered-books.spec.ts` as the guard. Related REAL bug found and left:
  `church.ts`'s `sharedAtRef` regex rejects a hand-typed `?at=1 John 3:16` outright.

## D. First impression (PWA) — before sharing the link

- [x] **[opus]** Social metadata: meta description + full OG/Twitter block + static
  1200×630 og-image + `apple-touch-icon-180` in `index.html`; register new assets in
  `vite.config.ts` publicFiles.
- [ ] **[opus]** **NEW 2026-07-29, found by the WebKit project (`I-01`).** `sw.js`'s
  `mayCache()` recognises "the shell document asked for as data" by comparing
  `url.href`, so ANY query string walks past it: `index.html?x`, `/?x` and
  `manifest.webmanifest?x` are all written to the cache by a non-navigation fetch.
  That is the exact white-screen vector the comment above it claims to have closed —
  a newer document cached while that build's `/assets/*` are absent. Chromium caches
  it too (`cache.keys()` lists the entry); `app.spec.ts:821` passes there ONLY because
  the page's `cache.match` runs before the SW's fire-and-forget `cache.put` lands, and
  WebKit wins that race and fails. Two-part fix: compare `url.pathname` in
  `isShellDoc`, and make the test POLL for the entry instead of racing it. Latent today
  (`engine/update.ts` fetches the manifest `no-store`, which the first rule refuses),
  but the guard is weaker than its own comment. Then add
  `/checking for an update cannot poison the cached shell/` to `OFFLINE_ON_WEBKIT`.
- [ ] **[opus]** Icons/manifest: generate 192/512 + maskable from `public/icon.svg`;
  add manifest `id`, `lang`, `orientation`, `categories`, `screenshots`; dark
  `theme-color` meta pair.
- [ ] **[opus]** `<noscript>` block; `404.html` redirect preserving search+hash;
  optional `public/CNAME` (belt-and-braces — Pages setting is verified working).
- [x] **[opus]** Canvas reader exposes zero accessible text (screen readers, Ctrl+F,
  translate see nothing): hidden text mirror rebuilt from the display list +
  `role`/`aria-label` on the wrapper (`ReaderPane.svelte:438-451`). Also
  role/label/keyboard path for `CanonStrip.svelte:85-87`.
- [ ] **[opus]** URL routing: mirror pane 0 into `location.hash` (`#/John/3`),
  `pushState` when a transient surface opens, `popstate` → `dismissTransient()` so
  Back closes overlays instead of exiting the PWA. Nothing is bookmarkable today.
- [ ] **[opus]** "Share link" verse action in `ContextMenu` →
  `shareUrl(PWA_URL, s.church, {at: refKey})` — the `?at=` plumbing exists, only
  Present's QR uses it.
- [x] **[opus]** Light-theme contrast fails WCAG AA: darken `faded` → ~#6e6862 and
  `gold` → ~#846327 in `theme.rs:200-212` (fixes both shells); fix Present
  `.linkbtn` (~1.5:1!) and `.stepbar`; restate literal light values inside the
  white `.share-dialog`; dim MapFrame paper in night.
- [ ] **[opus]** A stray tap outside the first-run card permanently loses onboarding:
  make the choose stage non-dismissible or always write `config.intro`
  (`FirstRun.svelte:207-216`). **HELD BACK 2026-07-29 — written and working, NOT
  committed.** The fix (choose/tiers/church non-dismissible; welcome/curious dismiss
  via `startInJohn()` so the intro is recorded) passes its own 2 tests and 3 mutations,
  and Android's identical `BackHandler` hole was fixed with it. But with it in the tree
  `e2e/network.spec.ts` takes 4.3 min with one test hanging to the 240 s timeout;
  without it, 27 s and 3/3 — reproduced 3 clean runs vs 2 hangs, and the full suite went
  11.6 min / 2 failed → 3.6 min / 107 passed on removing it alone. No mechanism found:
  `.backdrop` and `.dialog` are SIBLINGS, so nothing in the path `firstVisit` takes can
  reach `dismiss()`. Not shipped unexplained, because network.spec.ts is what guards the
  offline promise. Held at
  `…/scratchpad/d08-held/{FirstRun.svelte,firstrun.spec.ts,android-firstrun.patch}`.
  Next step: give `page.reload()` in `timedReload` an explicit navigation timeout so the
  hang fails fast and names itself instead of eating the test budget.
- [ ] **[opus]** Splash: read the cached palette (written but never read — dark users
  get a cream flash every launch); say "≈3 MB, one time — then Plumbline works with
  no connection"; start phase as `prepare` not `download` (warm boots claim to be
  fetching); map boot errors to human copy with raw string behind `<details>`.
- [ ] **[opus]** Global `error`/`unhandledrejection` handler → dismissible
  "something went wrong — reload" bar (none exists anywhere).
- [ ] **[opus]** Touch targets: one `min-height/width: 44px` rule across the chrome
  (search glass, ≡ menu, menu rows, study-sheet close, context-menu rows, Present
  stepbar, pickers — full list in the audit).
- [ ] **[opus]** Safe-area insets: header, `.present`, and landscape left/right —
  only the bottom nav honours them today (`Shell.svelte:561`).
- [ ] **[opus]** Raw OSIS refKeys in web UI copy ("Tag 1Cor 13:4") — use
  verse display names at the 5 sites (ContextMenu, TagPicker, ThreadPicker,
  PassagePicker, toasts); Android already does.
- [ ] **[opus]** Dialog focus management: a `use:modal` action (focus in, trap Tab,
  restore on close, local Escape) across the 9 `aria-modal` dialogs; Escape while
  focus is in an input currently does nothing. Add `role="status"` to the main
  toast (the update toast already has it).
- [x] **[opus]** BookNav: OT/NT toggle + current-book marker (port from
  `ui/BookNav.kt:142-149, 265-268`) + one-line reading-tint legend (title= never
  fires on touch).
- [ ] **[opus]** Empty states in `panel.rs`: search "0 results" gets guidance; weaves(0)
  gets a body; web `{#if blocks}` should treat `[]` as empty (fixing core fixes
  both shells).
- [ ] **[opus]** Chrome ignores the text-size setting and browser font prefs: publish
  `--uiScale` on `:root` and scale the chrome, or convert to rem.
- [x] **[FABLE]** "Delete my data" destructive-path spec: DONE 2026-07-29 — exact
  kill/survive scope, flow, ordering rule and test requirements in
  [docs/DELETE-MY-DATA.md](docs/DELETE-MY-DATA.md).
- [ ] **[opus]** Implement erase-my-data per
  [docs/DELETE-MY-DATA.md](docs/DELETE-MY-DATA.md) on BOTH shells, with the
  offline e2e + Android scope unit test it requires. Do not improvise beyond the
  spec's scope table.
- [ ] **[opus]** Wire the dead `packUpdated` signal to the existing update-toast
  wording; capture `beforeinstallprompt` → "Install" in the ≡ menu.
- [ ] **[opus]** Decouple the pasteable bug-report header from `PERF`
  (`SettingsDialog.svelte:176-216`), then flip `PERF` off for release (its own
  docstring says it shouldn't ship on).
- [x] **[opus]** Weave connectors drawn ~23 px off: measure the nav strip with
  `bind:this` + ResizeObserver instead of the stale `NAV_H = 33` const
  (`ConnectorsOverlay.svelte:10`) — same pattern as `--bottomNavH`.

## E. Release mechanics — before `git tag v1.0.0`

- [x] **[opus]** Version identity: workspace `Cargo.toml` 0.1.0 → 1.0.0 (About shows
  "engine 0.1.0" otherwise); bump `apps/web/package.json`; strip the `v` prefix
  consistently in *both* release.yml jobs (web shows "v1.0.0", Android "1.0.0").
- [x] **[opus]** LICENSE: append a data carve-out — MIT covers the code; `data/` and
  `bridge/` keep their own licenses (strongs.json and Abbott-Smith are CC-BY-SA),
  see BIBLIOGRAPHY.md. Link LICENSE from README.
- [ ] **[FABLE]** Hand-write the v1.0.0 release notes (`gh release create
  --notes-file`) — auto-generated notes are PR-title soup and the repo is the
  download page.
- [x] **[opus]** Release workflow: add `npm run check` to the pages job (a tag can
  currently ship type errors CI would catch); pin cargo-ndk via
  `taiki-e/install-action` in release.yml (matches ci.yml); add
  `workflow_dispatch`.
- [x] **[opus]** `git rm -r --cached weaves threads patches` (tracked against
  .gitignore's intent; second source of truth for the stock set); add `/patches/`
  to .gitignore; drop `"patches"` from hydrate `USER_DIRS` + its `--help` text.
- [x] **[opus]** FEATURE-MANIFEST.md cleanup: 5 residual highlight mentions, 5 false
  "not yet in Compose" blocks, icon marked "pending" but shipped, dead
  `apps/desktop/` paths, undeclared canon-strip delta, first-run "fetched live"
  line (it's hardcoded).
- [x] **[opus]** README: 4-step sideload instructions + APK sha256 + 3-4 more
  screenshots; BIBLIOGRAPHY.md: name the actual source module/edition for the 1769
  margin notes (ask Glendon if not derivable).
- [ ] **[opus]** Clippy/rustfmt gates: fix `search.rs:451` + `strongs.rs:136`, run
  `-D warnings` across all crates (ffi has never been fully linted), drop both
  `continue-on-error` lines in ci.yml.
- [x] **[opus]** Android lint: replace the blanket `abortOnError = false` with a
  targeted `disable += "NonNullableMutableLiveData"`.
- [ ] **[opus]** Split `crates/ffi/src/lib.rs` (3,861 lines; repo rule is 3k) —
  authoring + study-blocks sections are contiguous; no ABI change, bindgen guards.

## F. Performance — web

- [x] **[opus]** ConnectorsOverlay reallocates a full-viewport canvas every scroll
  frame on phones to draw nothing: guard the size assignment, bail before the
  alloc, mount conditionally (`ConnectorsOverlay.svelte:39-55`,
  `Shell.svelte:393`). #1 phone-jank suspect.
- [x] **[opus]** `$state.raw` for the display-list items (deep proxy walked 3×/frame,
  ~10k signals on Ps 119) + memoize `verseExtents` per layout
  (`ReaderPane.svelte:33`, `paint.ts:80-94`).
- [ ] **[opus]** Overlap the wasm fetch+compile with the stage-1 read (start
  un-awaited at the top of `boot()`, await at the instantiate site); start
  `loadFonts` un-awaited too (`boot.ts:86-97`, `engine.worker.ts:615-617`).
- [ ] **[opus]** Consolidate boot RPCs: boot reply carries palettes + toc (+ first
  chapter display list); `canonSegments` must not be a boot barrier
  (`App.svelte:49-86`).
- [ ] **[opus]** Debounce resize ~120 ms trailing (undebounced ResizeObserver
  re-lays-out per tick and thrashes the turn cache); consider raising
  `TURN_CACHE_MAX` (`ReaderPane.svelte:261-268`, `engine.worker.ts:139`).
- [ ] **[opus]** Search: 150–200 ms debounce; skip `fuzzy_hits` (full-vocabulary
  Levenshtein) for short/prefix queries; truncate postings to HIT_CAP before
  materializing; cap or exempt `searchBlocks` from the cache
  (`Shell.svelte:342`, `search.rs:275-378`).
- [ ] **[opus]** Slice warm phases 3/5/6 (xref 8.5 MB TSV parse, leitwort, bridge) the
  way SearchIxBuilder already is — they're the same shape as the fixed 54 s block
  (`crates/ffi/src/lib.rs:577-631`).
- [ ] **[opus]** Cache-layer quick fixes: the `startsWith("toc ")` exemption never
  fires (space vs `\0` — one-character bug, `session.svelte.ts:325-330`); bound
  `#cache` (LRU); memoize `weaveDots`/`noteVerses` with content comparison so
  epoch bumps stop repainting mid-scroll.
- [ ] **[FABLE]** Dependency-aware invalidation design: the `authored` event should
  carry what changed; per-key/method epochs instead of one global `cacheEpoch`
  (today one write refetches the world and every fill re-runs every derived).
- [ ] **[opus]** Move the measure memo to the Rust side of `FfiMeasure`
  (`lib.rs:825-846`): kills ~60% of wasm↔JS crossings cold, ~100% on re-layout,
  and benefits Android identically (JNA upcall per token today).
- [ ] **[FABLE]** Display-list transport: dropping `verseDisplay`/string-verse is a
  ~45% payload cut but bends the additive-only wire rule (engine+shell
  version-lock makes it arguable — decide); the structural fix is a
  typed-array/zero-copy list, which also obsoletes the proxy item above.
- [ ] **[FABLE]** Per-book pack split — quantified 2026-07-29: stage 1 is 99.3% one
  file (3.40 MB gz / 37.24 MB raw); split ⇒ first paint needs ~30 KB directory +
  one book, and it kills the ~150 MB transient boot peak. Engine-side per-chapter
  lazy decode already exists; touches data-prep, hydrate, manifest, `load_cache`,
  pin. Cross-cutting enough to design first.
- [ ] **[opus]** Code-split Present/Memorize/maps/dialogs behind `await import()` —
  zero splitting today; the shell-manifest plugin already handles lazy chunks
  offline.
- [ ] **[opus]** Memory: drop `Corpus::raw` (37 MB) once the warm has materialized
  every chapter; add `memory.buffer.byteLength` to the diagnostics op; y-bucket
  index for `hitAt` mousemove scans; parallelize the 30 serial warm-boot depot
  reads (cold path already does 4-way).

## G. Performance — Android

- [x] **[opus]** Scroll path (one file): move `scrollY` out of composition to a
  draw-phase read and stop writing it during composition
  (`ReaderPane.kt:175, 292-294`); replace the two `scrollY`-keyed LaunchedEffects
  with `snapshotFlow` + binary search over per-verse extents precomputed once per
  layout (`:246, :261-276`); hoist the per-frame `filter`/`groupBy`/`toArgbInt`
  allocations out of the draw loop (`:377-431`).
- [ ] **[opus]** Cancelled-layout native leak: `chapterHandle` close is unreachable on
  cancellation (`ReaderPane.kt:221`) — try/finally; and give the three Settings
  sliders `onValueChangeFinished` (a 2 s drag currently fires ~120 full layouts,
  ~119 leaked).
- [ ] **[opus]** `onLink` (~10 blocking engine calls) and `SearchOverlay.run()` onto
  `Dispatchers.Default` via the existing `loadStudy` pattern
  (`StudyScreen.kt:444-500, 1100-1112`).
- [ ] **[FABLE]** Replace the coarse `synchronized(engine)` monitor (49 sites) — the
  core is documented thread-safe for reads; a tap during a cold study build
  currently blocks the main thread for seconds (ANR class). Read-write lock or
  lock-free reads; concurrency semantics need care.
- [ ] **[opus]** `WarmIndexes()` builds all eight indexes at every cold start,
  including the off-by-default machine tier: warm `search_ix` eagerly only, gate
  the machine tier on `machineAnalysis`, and instrument on-device first
  (`MainActivity.kt:118-120`). Proper fix is `warm_step` in the C ABI — see the
  capabilities item in section H.
- [ ] **[opus]** Baseline profile + profileinstaller (20–40% typical Compose
  cold-start win; works sideloaded); R8 shrink-only (`isMinifyEnabled = true`, keep
  rules already written and correct); ABI splits (2.6 MB of dead x86_64 in every
  install); `mutableFloatStateOf`/`mutableIntStateOf` for scroll-path state.
- [ ] **[opus]** `StudyPane` → LazyColumn (eager Column lays out every block before
  first frame); process-wide Typeface cache (EB Garamond re-parsed per pane
  instance on main thread); buffered asset extraction + `noCompress` for
  jsonl/tsv/akjvb (34.8 MB through an 8 KB pipe today); backup zip I/O off the
  main thread; compose-compiler metrics/reports config.
- [ ] **[opus]** Chapter paint: record once per layout into an
  `android.graphics.Picture` and `drawPicture` per frame (today ~400–900 shaped
  `drawText` JNI calls per frame); 2–4 entry chapter display-list LRU so
  back-swipe is instant.

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
- [ ] **[opus]** Church/share consolidation: `plumbline_share_url_json` + core-owned
  clamps — collapses six duplicated shell pairs (shareUrl, cleanChurch,
  safeChurchUrl, churchTitle, visitChurch, PWA_URL).
- [ ] **[opus]** Dwell tracker → core (`DwellTracker::tick` + a
  `plumbline_reading_spec_json` endpoint) — ~80 identical lines per shell, both
  hardcoding thresholds they claim to fetch.
- [ ] **[opus]** Expose the user-dir/backup-dir lists from core — four hand-kept
  copies guarded only by "must stay in step" comments; also single-source the
  `pure-study/` legacy remap.
- [ ] **[opus]** Android verb dispatch: implement the 4 missing verbs (`untag`,
  `editThreadNotes`, `editWeaveNotes`, `editEntryNote`), make unhandled verbs loud
  on both shells (Android is a silent no-op; web is console.warn).
- [ ] **[opus]** Expose refKey parse/format helpers over the ABI and kill the 8+
  hand-parse sites (source of the numbered-book bug).
- [ ] **[opus]** Move domain logic out of `crates/ffi` into core with its tests:
  `english_gloss`, `distil_gloss`, `name_noise`/`CONCEPT_KEEP_NAMES`,
  `concept_label`, `memory_span`, tuning constants.
- [ ] **[opus]** Export `FLAG_RERENDERED` to the header with the compile-time assert
  (both shells hardcode `16`; it bypassed the mechanism built for exactly this).
- [ ] **[opus]** Dead code sweep: `pruneStale`, `pinHasStage`, `EMPTY_CHURCH`,
  ~12 unnecessary exports; decide `rnd::capabilities()` (exposed nowhere — wire it
  or delete it).

## I. Testing debt

- [x] **[opus]** WebKit Playwright project running at least the offline trio — the
  offline promise is untested on the engine where Cache API/eviction actually
  differ.
- [ ] **[opus]** **NEW 2026-07-29 (`I-01`).** Rewrite "boots offline after ONE visit"
  so its offline is a DEAD ORIGIN rather than `context.setOffline(true)`. Playwright's
  emulation makes WebKit stop consulting the service worker entirely — the reload dies
  with "WebKit encountered an internal error" — proven to be the harness and not us by a
  minimal cache-first SW on a throwaway origin failing identically while Chromium serves
  it from cache. So the offline promise's own test is the one test that cannot run on the
  engine that matters most; the two stalled-origin boots stand in for it today.
  `network.spec.ts` already has the machinery (`stallableOrigin` — close it instead of
  stalling), and it works on both engines. Same WebKit device booted to John 3 in 222 ms
  with its origin genuinely refusing connections.
- [ ] **[opus]** One UI-level authoring e2e: create a tag from the verse menu → add a
  second verse → convert to a weave (largest untested block; all authoring in
  tests today goes through back-door RPC).
- [ ] **[opus]** `store.rs` interrupted-write test; `usernote.rs` malformed-input +
  forward-compat tests (frozen format already inside shipped backup zips).
- [x] **[opus]** Legacy `pure-study/` zip restore test — both shells carry the shim,
  neither tests it.
- [x] **[opus]** Maps smoke e2e (ChordMap/ConceptMap/Constellation — ~500 lines,
  entirely unexercised).

## Future (not this release)

- **Other-language support, starting with German.** Blockers mapped by the
  2026-07-29 architecture audit: `TOKENIZATION_VERSION` is one global const
  conflating translation+tokenizer; `data/kjv.jsonl` is the data-home marker;
  `export::EDITION` compiled in; fixed 66-book canon table; and — biggest — user
  data (tags/notes/cards) carries no translation identifier. The serde-flatten,
  stable-id, wire-codegen, and first-run-into-core items above all directly reduce
  the cost of this.
