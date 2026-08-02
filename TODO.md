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
That decision also means **1.0.0 must not appear anywhere a reader could read it
as this release** — the version-prose sweep is the last item in §E, and README's
sideload block is already fixed and guarded by a test.

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
- [x] **[opus]** Stock toggle OFF deletes the reader's *edits* to stock-named items
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
- [x] **[opus]** Implement stable ids per [docs/STABLE-IDS.md](docs/STABLE-IDS.md)
  (first 1.0.x, NOT before the flatten item ships in v1.0): `id` + `updated` on
  Thread/Tag/Weave, lazy assignment on save, duplicate-id resolution, the four
  required tests. **DONE 2026-07-30, in this tag** — the ordering rule was "after
  the flatten item", and that landed in this same unreleased catalogue, so no
  shipped build can strip the new fields. The wire is untouched (ordinals stay
  until §H codegen). One thing the doc asked for is NOT implemented, deliberately:
  "the stale file is removed on the next save" needs a rename endpoint, and
  neither shell has one — nothing can create the artifact yet.

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
- [x] **[opus]** **NEW 2026-07-29, found by the WebKit project (`I-01`).** `sw.js`'s
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
- [x] **[opus]** Icons/manifest: generate 192/512 + maskable from `public/icon.svg`;
  add manifest `id`, `lang`, `orientation`, `categories`, `screenshots`; dark
  `theme-color` meta pair.
- [x] **[opus]** `<noscript>` block; `404.html` redirect preserving search+hash;
  optional `public/CNAME` (belt-and-braces — Pages setting is verified working).
- [x] **[opus]** Canvas reader exposes zero accessible text (screen readers, Ctrl+F,
  translate see nothing): hidden text mirror rebuilt from the display list +
  `role`/`aria-label` on the wrapper (`ReaderPane.svelte:438-451`). Also
  role/label/keyboard path for `CanonStrip.svelte:85-87`.
- [x] **[opus]** URL routing: mirror pane 0 into `location.hash` (`#/John/3`),
  `pushState` when a transient surface opens, `popstate` → `dismissTransient()` so
  Back closes overlays instead of exiting the PWA. Nothing is bookmarkable today.
  **DONE 2026-07-30.** Three `Shell.svelte`-local surfaces are still outside Back's
  reach (`searchOpen`, `shareApp`, `menuOpen` are component `$state`, not Session
  fields, so `transientOpen` cannot see them) — folded into the `use:modal` item
  below, which touches those dialogs anyway.
- [x] **[opus]** "Share link" verse action in `ContextMenu` →
  `shareUrl(PWA_URL, s.church, {at: refKey})` — the `?at=` plumbing exists, only
  Present's QR uses it.
- [x] **[opus]** Light-theme contrast fails WCAG AA: darken `faded` → ~#6e6862 and
  `gold` → ~#846327 in `theme.rs:200-212` (fixes both shells); fix Present
  `.linkbtn` (~1.5:1!) and `.stepbar`; restate literal light values inside the
  white `.share-dialog`; dim MapFrame paper in night.
- [x] **[opus]** A stray tap outside the first-run card permanently loses onboarding:
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
  hang fails fast and names itself instead of eating the test budget. **DONE 2026-07-30 — the retry worked.** choose/tiers/church are questions and ignore a miss; welcome/curious exit through `startInJohn()` so `intro` is always written. Android's `BackHandler` is disabled on the chooser so the system closes the app with nothing decided. The 2026-07-29 hang DID NOT REPRODUCE: network.spec.ts is 29 s and 4/4 on three consecutive runs with the fix in the tree, full suite 185/185. The 45 s navigation timeout stays as the guard.
- [x] **[opus]** Splash: read the cached palette (written but never read — dark users
  get a cream flash every launch); say "≈3 MB, one time — then Plumbline works with
  no connection"; start phase as `prepare` not `download` (warm boots claim to be
  fetching); map boot errors to human copy with raw string behind `<details>`. **DONE 2026-07-30.** Palette read is a blocking inline script in index.html's head (Svelte mounts after first paint); head `<style>` gives light/dark defaults from theme.rs, re-derived by the test. New `engine/bootError.ts` maps failures to reader copy, raw behind `<details>`. Writing it found a real copy bug — the network rule said "scripture data" when the ENGINE BINARY had failed — so it names no payload now.
- [x] **[opus]** Global `error`/`unhandledrejection` handler → dismissible
  "something went wrong — reload" bar (none exists anywhere). **DONE 2026-07-30.** Once per session, never during boot (the splash owns that), and `ResizeObserver loop` filtered as the classic false positive. Also wired `rpc.onFatal`, which existed and was connected to nothing.
- [x] **[opus]** Touch targets: one `min-height/width: 44px` rule across the chrome
  (search glass, ≡ menu, menu rows, study-sheet close, context-menu rows, Present
  stepbar, pickers — full list in the audit). **DONE 2026-07-30** as one `button, summary` rule. Exposed two real bugs: the stacked toast's 74px clearance was measured off a 27px button and overlapped once the floor applied, and the header could not afford 10px gaps at 44px. **RESIDUAL:** below ~340px the header overflows when Welcome+Church+Share are all present (354px in 320px). Hiding the app name there is a product call — left for Glendon.
- [x] **[opus]** Safe-area insets: header, `.present`, and landscape left/right —
  only the bottom nav honours them today (`Shell.svelte:561`). **DONE 2026-07-30.** Four insets named once on `:root` in app.css, which is now the ONLY place `env()` is written — that is what lets `e2e/safe-area.spec.ts` override them and prove the chrome moves, since a headless browser has no notch. Bottom inset for non-Present surfaces deliberately left (would double-count against the nav).
- [x] **[opus]** Raw OSIS refKeys in web UI copy ("Tag 1Cor 13:4") — use
  verse display names at the 5 sites (ContextMenu, TagPicker, ThreadPicker,
  PassagePicker, toasts); Android already does. **DONE 2026-07-30** at the three
  sites that had the bug; PassagePicker and the toasts were already display-name
  clean, so five sites turned out to be three.
- [x] **[opus]** Dialog focus management: a `use:modal` action (focus in, trap Tab,
  restore on close, local Escape) across the 9 `aria-modal` dialogs; Escape while
  focus is in an input currently does nothing. Add `role="status"` to the main
  toast (the update toast already has it). **DONE 2026-08-01.** One action across **14** `aria-modal` surfaces (the item said 9). Escape ended up DOCUMENT-level over a modal stack, not on the node: focus often leaves a dialog when the control the reader was on is removed, and a node listener silently stops firing — which for `askConfirm` is a promise that never settles, not a dead key.
- [x] **[opus]** BookNav: OT/NT toggle + current-book marker (port from
  `ui/BookNav.kt:142-149, 265-268`) + one-line reading-tint legend (title= never
  fires on touch).
- [x] **[opus]** **NEW 2026-07-30.** Chromium no longer computes `aria-valuetext`
  for a canvas with `role="slider"`, so a screen reader on it announces the canon
  strip's position as **"42"** instead of "Revelation". Evidence: a full AX-tree
  dump shows `valuetext: ""` and `value: <aria-valuenow>` while the DOM carries
  `aria-valuetext="Revelation"` at the same instant (`CanonStrip.svelte`). The
  attributes are right, so this needs a second channel — an `aria-live="polite"`
  region announcing the book on change is the cheap fix; `role="slider"` on a
  canvas may simply be the wrong primitive. `e2e/a11y.spec.ts` asserts the
  attributes and the tree's numeric position, and says so where it used to assert
  the book name. **DONE 2026-07-30.** A polite live region beside the canvas, announcing the book on change; the attributes stay, since they are correct and other AT may use them. The test carries a canary: if Chromium computes `valuetext` for a canvas slider again, it says so and this channel can go.
- [x] **[opus]** Empty states in `panel.rs`: search "0 results" gets guidance; weaves(0)
  gets a body; web `{#if blocks}` should treat `[]` as empty (fixing core fixes
  both shells). **DONE 2026-07-30**, with a test that NO list producer can answer
  with an empty vec — which is the invariant the web guard now leans on.
- [x] **[opus]** Chrome ignores the text-size setting and browser font prefs: publish
  `--uiScale` on `:root` and scale the chrome, or convert to rem. **DONE 2026-08-01.** `--uiScale` on `:root`, 128 declarations; chosen over rem so it cannot reach the reading canvas. A 1rem probe carries the browser's own font preference. Header now wraps rather than pushing the ≡ off-screen. **Android has the same bug and still does** — `studyScale` reaches only the study surface; recorded as a shell delta.
- [x] **[FABLE]** "Delete my data" destructive-path spec: DONE 2026-07-29 — exact
  kill/survive scope, flow, ordering rule and test requirements in
  [docs/DELETE-MY-DATA.md](docs/DELETE-MY-DATA.md).
- [~] **[DROPPED]** Implement erase-my-data per
  [docs/DELETE-MY-DATA.md](docs/DELETE-MY-DATA.md) on BOTH shells, with the
  offline e2e + Android scope unit test it requires. Do not improvise beyond the
  spec's scope table. **NOT DOING — Glendon's call 2026-07-31. The spec in docs/DELETE-MY-DATA.md stands if it is ever wanted.**
- [~] **[DROPPED]** Wire the dead `packUpdated` signal to the existing update-toast
  wording; capture `beforeinstallprompt` → "Install" in the ≡ menu. **NOT DOING — Glendon's call 2026-07-31.**
- [x] **[opus]** Decouple the pasteable bug-report header from `PERF`
  (`SettingsDialog.svelte:176-216`), then flip `PERF` off for release (its own
  docstring says it shouldn't ship on). **DONE 2026-07-30.** Flipping it off turned
  two `app.spec.ts` tests red, correctly: the boot TRACE was PERF-gated in
  `boot.ts` while every trace push in `engine.worker.ts` was not. The trace is the
  flight recorder the suite reads, so it is ungated now; PERF keeps only what costs
  per turn, per engine call or per text measurement.
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
- [x] **[opus]** Clippy/rustfmt gates: fix `search.rs:451` + `strongs.rs:136`, run
  `-D warnings` across all crates (ffi has never been fully linted), drop both
  `continue-on-error` lines in ci.yml. **DONE 2026-07-30.** 36 distinct sites across four configurations (workspace/all-targets/all-features, rnd featureless, rnd full, wasm32) — the old step linted libs only. No crate-root blanket; five narrow `#[allow(dead_code)]` on the cfg(wasm32)-only warm entry points. New `rustfmt.toml` records the tree's real 120-col style, cutting the reformat from 859 hunks/42 files to 437/37. Both `continue-on-error` lines gone.
- [x] **[opus]** Android lint: replace the blanket `abortOnError = false` with a
  targeted `disable += "NonNullableMutableLiveData"`.
- [~] **[DROPPED]** Split `crates/ffi/src/lib.rs` (3,861 lines; repo rule is 3k) —
  authoring + study-blocks sections are contiguous; no ABI change, bindgen guards. **NOT DOING — Glendon's call 2026-07-31. Down to ~3.2k lines after the concept removals; still over the 3k rule, which the rule now tolerates.**
- [~] **[DROPPED]** **Version-prose sweep — LAST item before the tag.** The manifests
  say 0.36.0 and a test now pins README to it, but prose written when this was a
  1.0.0 audit still says otherwise: this file's own title and §E's header, and
  `docs/STABLE-IDS.md`'s "v1.0 / first 1.0.x" sequencing (B-02's agent updates
  that one). Keep 1.0.0 only where it means Glendon's future milestone; anywhere it
  could be read as *this* release, say 0.36.0 or "the next tag". Run it last so it
  catches whatever the intervening batches write. **NOT DOING — Glendon's call 2026-07-31. Cosmetic; the manifests and the release gate are already consistent.**
## D2. The offline promise (2026-07-31)

- [x] **[opus]** **The PWA showed a white screen in airplane mode.** Two writers
  of one shell: `precache.ts` wrote the document under both navigation keys but
  guarded them with `depotHas`, so they were never replaced, while `sw.js` cached
  navigations and updated only `./index.html`. After a deploy the two keys could
  hold different builds, and `pruneToPin` deleted the older build's bundle. An
  installed PWA opens `start_url: "./"` — the key frozen at the first build ever
  seen — so the document was served and its bundle was gone. **DONE 2026-07-31**:
  the service worker refuses the document outright, the precache writes it last
  from one response into both keys, and an incomplete shell reclaims nothing.
  Three tests over a simulated deploy; five mutations.

## F. Performance — web

- [x] **[opus]** ConnectorsOverlay reallocates a full-viewport canvas every scroll
  frame on phones to draw nothing: guard the size assignment, bail before the
  alloc, mount conditionally (`ConnectorsOverlay.svelte:39-55`,
  `Shell.svelte:393`). #1 phone-jank suspect.
- [x] **[opus]** `$state.raw` for the display-list items (deep proxy walked 3×/frame,
  ~10k signals on Ps 119) + memoize `verseExtents` per layout
  (`ReaderPane.svelte:33`, `paint.ts:80-94`).
- [x] **[opus]** Overlap the wasm fetch+compile with the stage-1 read (start
  un-awaited at the top of `boot()`, await at the instantiate site); start
  `loadFonts` un-awaited too (`boot.ts:86-97`, `engine.worker.ts:615-617`).
- [x] **[opus]** Consolidate boot RPCs: boot reply carries palettes + toc (+ first
  chapter display list); `canonSegments` must not be a boot barrier
  (`App.svelte:49-86`). **DONE 2026-07-30.** 6 messages across 3 await barriers → 1 message in 1 barrier. Palettes + TOC ride the boot reply (`BOOT_READS` allow-list: session-immutable, zero-arg); `canonSegments` is off the path entirely — the strip/navigator/maps fetch it through `q()`. Test stalls it forever and asserts the text still arrives.
- [x] **[opus]** Debounce resize ~120 ms trailing (undebounced ResizeObserver
  re-lays-out per tick and thrashes the turn cache); consider raising
  `TURN_CACHE_MAX` (`ReaderPane.svelte:261-268`, `engine.worker.ts:139`).
  **DONE 2026-07-30**; `TURN_CACHE_MAX` 8 → 16, sized against three panes each
  prefetching both neighbours (9 live keys) with a measured ~3 MB cost.
- [x] **[opus]** Search: 150–200 ms debounce; skip `fuzzy_hits` (full-vocabulary
  Levenshtein) for short/prefix queries; truncate postings to HIT_CAP before
  materializing; cap or exempt `searchBlocks` from the cache
  (`Shell.svelte:342`, `search.rs:275-378`). **DONE 2026-08-01, three of four.** 180 ms debounce, truncate-before-materialize, and ~38k allocations out of the fuzzy pass: "god" 1.3 → 0.3 ms, "thes" 1.7 → 0.1 ms. Identical results over 15,936 queries. **Skipping fuzzy for short/prefix queries was measured and NOT done**: short already skips it, what remains costs 0.2–0.5 ms, and the rule breaks real typos (a dropped last letter is a strict prefix). A test pins that.
- [x] **[opus]** Slice warm phases 3/5/6 (xref 8.5 MB TSV parse, leitwort, bridge) the
  way SearchIxBuilder already is — they're the same shape as the fixed 54 s block
  (`crates/ffi/src/lib.rs:577-631`). **DONE 2026-07-30 for 3 and 5; 6 measured and
  deliberately left.** Desktop numbers: xref 117 ms → worst slice 7.1 ms, leitwort
  84 ms → 11.6 ms, **bridge 3 ms** — an order of magnitude under the ~300 ms chunk
  budget even at a phone's 5–10×, so slicing it would add a builder, a mutex and a
  cursor to buy nothing. Profile tests (`--ignored xref_slice_profile`,
  `leitwort_slice_profile`) print the numbers on demand.
- [x] **[opus]** Cache-layer quick fixes: the `startsWith("toc ")` exemption never
  fires (space vs `\0` — one-character bug, `session.svelte.ts:325-330`); bound
  `#cache` (LRU); memoize `weaveDots`/`noteVerses` with content comparison so
  epoch bumps stop repainting mid-scroll.
- [ ] **[FABLE]** Dependency-aware invalidation design: the `authored` event should
  carry what changed; per-key/method epochs instead of one global `cacheEpoch`
  (today one write refetches the world and every fill re-runs every derived).
- [x] **[opus]** Move the measure memo to the Rust side of `FfiMeasure`
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
- [~] **[DROPPED]** Code-split Present/Memorize/maps/dialogs behind `await import()` —
  zero splitting today; the shell-manifest plugin already handles lazy chunks
  offline. **NOT DOING — Glendon's call 2026-07-31. Note it would have added lazy chunks to the shell, which is more surface for the 2026-07-31 offline bug — worth re-reading that fix first if it ever comes back.**
- [~] **[DROPPED]** Memory: drop `Corpus::raw` (37 MB) once the warm has materialized
  every chapter; add `memory.buffer.byteLength` to the diagnostics op; y-bucket
  index for `hitAt` mousemove scans; parallelize the 30 serial warm-boot depot
  reads (cold path already does 4-way). **NOT DOING — Glendon's call 2026-07-31. **Worth remembering**: this is ~37 MB of resident memory on a phone, and the y-bucket index for `hitAt` is a per-mousemove scan. If the app is ever reported as sluggish or killed in the background, start here.**
## G. Performance — Android

- [x] **[opus]** Scroll path (one file): move `scrollY` out of composition to a
  draw-phase read and stop writing it during composition
  (`ReaderPane.kt:175, 292-294`); replace the two `scrollY`-keyed LaunchedEffects
  with `snapshotFlow` + binary search over per-verse extents precomputed once per
  layout (`:246, :261-276`); hoist the per-frame `filter`/`groupBy`/`toArgbInt`
  allocations out of the draw loop (`:377-431`).
- [x] **[opus]** Cancelled-layout native leak: `chapterHandle` close is unreachable on
  cancellation (`ReaderPane.kt:221`) — try/finally; and give the three Settings
  sliders `onValueChangeFinished` (a 2 s drag currently fires ~120 full layouts,
  ~119 leaked).
- [x] **[opus]** `onLink` (~10 blocking engine calls) and `SearchOverlay.run()` onto
  `Dispatchers.Default` via the existing `loadStudy` pattern
  (`StudyScreen.kt:444-500, 1100-1112`). **DONE 2026-07-30**, 25 JVM tests over the
  extracted decisions. Still on the main thread and out of scope: `TocJson` /
  `StudyConfig.LoadJson` / `PaletteJson` in `remember { }` during first composition
  — first-frame boot cost rather than a tap stall, and fixing it needs a loading
  state for the whole screen.
- [ ] **[FABLE]** Replace the coarse `synchronized(engine)` monitor (49 sites) — the
  core is documented thread-safe for reads; a tap during a cold study build
  currently blocks the main thread for seconds (ANR class). Read-write lock or
  lock-free reads; concurrency semantics need care.
- [x] **[opus]** `WarmIndexes()` builds all eight indexes at every cold start,
  including the off-by-default machine tier: warm `search_ix` eagerly only, gate
  the machine tier on `machineAnalysis`, and instrument on-device first
  (`MainActivity.kt:118-120`). Proper fix is `warm_step` in the C ABI — see the
  capabilities item in section H.
- [x] **[opus]** Baseline profile + profileinstaller (20–40% typical Compose
  cold-start win; works sideloaded); R8 shrink-only (`isMinifyEnabled = true`, keep
  rules already written and correct); ABI splits (2.6 MB of dead x86_64 in every
  install); `mutableFloatStateOf`/`mutableIntStateOf` for scroll-path state.
  **DONE 2026-07-30, with one part device-gated:** the profile is HAND-AUTHORED,
  because a real one comes from a macrobenchmark run on a device and there is none
  on this machine. Release APKs now 11.1 MB (arm64) / 11.4 MB (x86_64), separately.
  release.yml and ci.yml both had to change — the split filenames broke the one
  `cp` the release job does, inside a step a `workflow_dispatch` dry run skips.
- [x] **[opus]** `StudyPane` → LazyColumn (eager Column lays out every block before
  first frame); process-wide Typeface cache (EB Garamond re-parsed per pane
  instance on main thread); buffered asset extraction + `noCompress` for
  jsonl/tsv/akjvb (34.8 MB through an 8 KB pipe today); backup zip I/O off the
  main thread; compose-compiler metrics/reports config. **DONE 2026-07-30, except `noCompress` which is measured and OFF.** LazyColumn, buffered extraction, backup off the main thread, compose metrics behind a property. The Typeface fix is two caches, not one: Compose's `FontFamily` AND ReaderPane's three canvas `Typeface`s (the per-pane parse the item names) — both process-wide now, warmed off-thread. `noCompress` ships off behind `-PplumblineNoCompressData`: a real A/B release build says +24,143,359 bytes (11.10 → 35.24 MB, +218%) to save a few hundred ms of inflate once per install. Wrong trade for a sideloaded download.
- [x] **[opus]** Chapter paint: record once per layout into an
  `android.graphics.Picture` and `drawPicture` per frame (today ~400–900 shaped
  `drawText` JNI calls per frame); 2–4 entry chapter display-list LRU so
  back-swipe is instant. **DONE 2026-07-30.** Explicit enumerated `ChapterPaintKey` drives both re-layout and re-record so they cannot drift; per-frame/per-tap layers (scroll, search bands, pin, note dots) stay out of the Picture. 16 JVM tests, 6 mutations proven. NOT device-verified: watch for stale text after a theme or text-size change.
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
- [x] **[opus]** Church/share consolidation: `plumbline_share_url_json` + core-owned
  clamps — collapses six duplicated shell pairs (shareUrl, cleanChurch,
  safeChurchUrl, churchTitle, visitChurch, PWA_URL). **DONE 2026-08-01.** `core::church` owns it; one endpoint replaces six shell pairs. Four live disagreements found; the encoding one mattered (Android left a literal `+` in a church name). Web keeps a TS copy — share links are read synchronously out of `$derived` and the engine is in a worker — pinned by a shared vector table. Title joins with a colon now.
- [x] **[opus]** Dwell tracker → core (`DwellTracker::tick` + a
  `plumbline_reading_spec_json` endpoint) — ~80 identical lines per shell, both
  hardcoding thresholds they claim to fetch. **DONE 2026-08-01.** `DwellTracker` owns grace, idle, cadence and the tail; both shells send one sample a second. Android's stale `ReadingSpec` defaults said 220 wpm two days after the core moved to 300, and were live before every fetch landed — the model is deleted, so the phone cannot hold a stale threshold.
- [~] **[DROPPED]** Expose the user-dir/backup-dir lists from core — four hand-kept
  copies guarded only by "must stay in step" comments; also single-source the
  `pure-study/` legacy remap. **NOT DOING — Glendon's call 2026-07-31. The four hand-kept copies stay, guarded only by their comments.**
- [~] **[DROPPED]** Android verb dispatch: implement the 4 missing verbs (`untag`,
  `editThreadNotes`, `editWeaveNotes`, `editEntryNote`), make unhandled verbs loud
  on both shells (Android is a silent no-op; web is console.warn). **NOT DOING — Glendon's call 2026-07-31. **Worth remembering**: this is not only cleanup. Four verbs (`untag`, `editThreadNotes`, `editWeaveNotes`, `editEntryNote`) are a SILENT NO-OP on Android today, so those taps do nothing and say nothing.**
- [~] **[DROPPED]** Expose refKey parse/format helpers over the ABI and kill the 8+
  hand-parse sites (source of the numbered-book bug). **NOT DOING — Glendon's call 2026-07-31. The 8+ hand-parse sites stay; they were the source of the numbered-book bug.**
- [~] **[DROPPED]** Move domain logic out of `crates/ffi` into core with its tests:
  `english_gloss`, `distil_gloss`, `name_noise`/`CONCEPT_KEEP_NAMES`,
  `concept_label`, `memory_span`, tuning constants. **NOT DOING — Glendon's call 2026-07-31.**
- [x] **[opus]** Export `FLAG_RERENDERED` to the header with the compile-time assert
  (both shells hardcode `16`; it bypassed the mechanism built for exactly this).
- [x] **[opus]** Dead code sweep: `pruneStale`, `pinHasStage`, `EMPTY_CHURCH`,
  ~12 unnecessary exports; decide `rnd::capabilities()` (exposed nowhere — wire it
  or delete it).

## I. Testing debt

- [x] **[opus]** WebKit Playwright project running at least the offline trio — the
  offline promise is untested on the engine where Cache API/eviction actually
  differ.
- [x] **[opus]** **NEW 2026-07-29 (`I-01`).** Rewrite "boots offline after ONE visit"
  so its offline is a DEAD ORIGIN rather than `context.setOffline(true)`. Playwright's
  emulation makes WebKit stop consulting the service worker entirely — the reload dies
  with "WebKit encountered an internal error" — proven to be the harness and not us by a
  minimal cache-first SW on a throwaway origin failing identically while Chromium serves
  it from cache. So the offline promise's own test is the one test that cannot run on the
  engine that matters most; the two stalled-origin boots stand in for it today.
  `network.spec.ts` already has the machinery (`stallableOrigin` — close it instead of
  stalling), and it works on both engines. Same WebKit device booted to John 3 in 222 ms
  with its origin genuinely refusing connections.
- [x] **[opus]** One UI-level authoring e2e: create a tag from the verse menu → add a
  second verse → convert to a weave (largest untested block; all authoring in
  tests today goes through back-door RPC). **DONE 2026-07-30.** Found one gap worth
  fixing later: `TagWeave.svelte` is the only surface in the flow with no
  `data-surface` attribute, so the locator has to go through `role="dialog"` + text,
  and it is missing from `e2e/surfaces.spec.ts`'s SURFACES table too.
- [x] **[opus]** `store.rs` interrupted-write test; `usernote.rs` malformed-input +
  forward-compat tests (frozen format already inside shipped backup zips).
  **DONE 2026-07-30.** Writing them found two real gaps in `set_note`, both fixed
  here: unparseable bytes were rewritten from scratch (now moved aside as `.bad`,
  the rescue `config.rs` already had — lifted into `store.rs` so there is one copy
  of the rule), and a `pure-note-v2` file was silently restamped as v1 (now
  refused, the way `thread.rs` refuses to clobber). Atomicity is now DRIVEN: a
  reader thread hammering the file while another rewrites it, which reddens on a
  plain `fs::write`.
- [x] **[opus]** Legacy `pure-study/` zip restore test — both shells carry the shim,
  neither tests it.
- [x] **[opus]** Maps smoke e2e (ChordMap/ConceptMap/Constellation — ~500 lines,
  entirely unexercised). The ConceptMap third of it went with the concept map
  itself on 2026-07-30; the other two stand.

## Future (not this release)

- **Hymnal**: Adding a hymnal of public domain songs. Text size "presentable" so
  that people can share a phone and sing together. It should have automated
  scroll and preferably the ability to show and transpose the chords so that we
  can also play it.

- **Other-language support, starting with German.** Blockers mapped by the
  2026-07-29 architecture audit: `TOKENIZATION_VERSION` is one global const
  conflating translation+tokenizer; `data/kjv.jsonl` is the data-home marker;
  `export::EDITION` compiled in; fixed 66-book canon table; and — biggest — user
  data (tags/notes/cards) carries no translation identifier. The serde-flatten,
  stable-id, wire-codegen, and first-run-into-core items above all directly reduce
  the cost of this. Multi-language support includes hymnal as much as possible.
