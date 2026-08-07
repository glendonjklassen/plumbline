# Plumbline — working rules

## Product rules

- **Two shells, one product: Android (Compose) and web (PWA).** Android is
  the **UX gold standard** — its layout/menu patterns port to the web, not
  the other way around. A feature added in either shell lands in the other
  in the same change set when possible; anything that can't goes in
  [docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md) under shell deltas.
  That manifest is the parity contract — read it before shell work instead
  of re-surveying the repo. (The GTK and WinUI desktop shells were retired
  and removed 2026-07-25 — git history has them; ignore stale references.)
- Distribution: the PWA (apps/web) for most people; the signed APK on GitHub
  Releases for rooted/sideloading users. No Play Store, no Google account.
- `../overlay` (Haskell) is the read-only reference implementation — port
  from it, never modify it.
- Frozen contracts: the on-disk data formats (§Data formats below), `kjv1769-tok2`
  tokenization stamp, the camelCase wire JSON (additive evolution only), and
  refKey (`"Gen 1:7"`). The format tags keep their pre-rename names on purpose —
  `pure-note-v1` (`crates/core/src/usernote.rs`) is serialized into every user
  note file and already sits inside shipped backup zips, so renaming it with the
  product would break restore-from-backup; same for `overlay-weave-v2` and
  `overlay-memory-v1`.
- The bundled stock study set lives at `apps/android/app/src/main/assets/stock`
  (threads/tags/weaves); both shells seed it once and user edits/deletions
  stick.

## Layout of the tree

| Crate (package) | What it is |
|-----------------|------------|
| `crates/core` (`plumbline-core`) | Pure domain: corpus, Strong's, search, weaves, tags, config, atomic store |
| `crates/layout` (`plumbline-layout`) | Greedy line-breaker + hit regions (measures via callback) |
| `crates/rnd` (`plumbline-rnd`) | Feature-gated analytics: bridge, morphology, keyness, witness, concept |
| `crates/ffi` (`plumbline-ffi`) | The single flat C ABI for native shells (cdylib) — see [crates/ffi/README.md](crates/ffi/README.md) |
| `crates/hydrate` (`plumbline-hydrate`) | CLI: copy/verify the data pack into a home |
| `apps/android` | The Compose shell (Android) — the UX gold standard |
| `apps/web` | The PWA shell (Svelte + the core compiled to wasm32-wasip1) |

The five portable crates are dependency-light pure Rust and build anywhere,
including `wasm32-wasip1` (the web shell) and the Android NDK targets. CI runs
the portable tests, the R&D-feature tests, an MSRV check, an FFI
binding-drift guard, the web shell's Playwright suite, and the Android APK
build — engine cross-compiled for both shipped ABIs — on every push. The
offline pipeline that produced the data pack is documented in
[data-prep/README.md](data-prep/README.md).

## Architecture

Decisions locked 2026-07-08, still in force:

| # | Decision | Choice |
|---|----------|--------|
| 1 | UI strategy | **Native shell per platform** over a shared Rust core. Today: Jetpack Compose (Android, the UX gold standard) + a PWA (web) covering every desktop. The GTK/WinUI desktop shells were built first and retired 2026-07-25. |
| 2 | Build order | Desktop first (GTK4) → Windows → Android → web; the desktops then retired in favour of the PWA. |
| 3 | Data delivery | **Bundle core, download R&D** — KJV + Strong's ship in-app; heavy analytics artifacts are optional packs. |
| 4 | R&D default | **Guided first-run** — first launch picks the analysis tiers (scholars' / machine) with examples; the text and the reader's own data are always on (revised 2026-07-25 from the original Simple/Full split). |
| — | Patches / signed rules | Dropped — the Ed25519 point-patch/rule layer was not ported. |
| — | Future | The paid sync SaaS was **cancelled 2026-07-25** — the product is entirely free. Keep the data-model discipline it imposed anyway (stable ids, no host-local assumptions, exportable single-file JSON). |

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
chapter + width + font metrics (via an injected measure callback —
android.graphics.Paint on Android, canvas measureText on the web), the core
produces a *display list*: positioned glyph runs plus a table of tappable word
rectangles, each carrying its verse ref, token index, and Strong's refs. A
shell only paints that list and sends tap / hover `(x, y)` back for the core to
hit-test. Word-level study features are written once, and shells stay
genuinely thin.

## Data formats (frozen)

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

The **tokenizer** stays an offline data-prep step — the runtime only
*consumes* `kjv.jsonl`, carrying the version stamp check but not the tokenizer
itself.

The **data home** is the first of: `$PLUMBLINE_HOME` / `$OVERLAY_HOME`, a
directory tree containing `data/kjv.jsonl` (a checkout counts), the
executable's directory, or the per-user data dir (`~/.local/share/plumbline`
on Linux). Seed one outside the checkout with:

```sh
cargo run --release -p plumbline-hydrate -- copy --from . --to ~/.local/share/plumbline
```

## Working on this machine (Linux)

- Everything Rust builds and tests natively; the two shipping targets
  cross-build from here — the Android `.so` via `cargo-ndk` (NDK at
  `/opt/android-ndk`, see [docs/ANDROID-BOOTSTRAP.md](docs/ANDROID-BOOTSTRAP.md))
  and the web engine via `wasm32-wasip1`. There is no desktop shell to build
  anymore, so nothing in the tree needs a Windows or GTK toolchain.
- Android needs **JDK 21** (`JAVA_HOME=java-21-openjdk`); a newer system JDK is
  too new for AGP and fails the Gradle build.

## UI testing

- **Native shells (Android):** never drive the UI with synthetic input —
  build, then hand over; the maintainer tests on-device and gives feedback.
  Launching is fine only when asked.
- **Web shell:** Playwright end-to-end tests are sanctioned and wanted
  (`apps/web`, `npm run test:e2e`). Keep the boot-responsiveness regression
  test green — the engine lives in ONE worker thread, so a long synchronous
  engine call starves every layout/tap RPC queued behind it. Background
  loading must stay chunked with yields (see `engine.worker.ts`).
- **Mutation-test any regression test you add.** Break the fix, watch the new
  test fail, restore. Two tests written on 2026-07-26 passed against the very
  bug they described: one used `page.route()` (Playwright interception
  **bypasses service workers** — SW behaviour must be driven by a real
  stalling origin, see `e2e/network.spec.ts`), the other used a fixed
  millisecond ceiling that a whole un-chunked warm still fit inside (budgets
  for worker-scheduling tests must be **derived from the machine's own
  measured chunk cost**, not a constant). A third on 2026-08-03: a *ratio*
  between two things that BOTH regress. It compared a German chapter turn
  against an English one to catch a per-word cost, and passed against the very
  bug it described because the defect slowed English too. **A comparative
  budget cannot see a cost both sides pay** — calibrate against something the
  defect does not touch (there, the same chapter re-served from the turn cache).
  A mutation is also only faithful if the artifact under test was actually
  rebuilt: `pack:wasm` stages to `public/`, and only `npm run build` copies it
  into `dist/`, so a skipped build tests the *fixed* engine twice and reports
  the mutation as survived.
- **A warm boot must make ZERO network requests before text.** The pin
  (`engine/pin.ts`) is a manifest stored in the depot, written only after every
  file it names is verified present — so boot never asks the network anything it
  already has. Pack URLs are content-addressed per file (`?h=<hash of raw bytes>`)
  and stored EXPLICITLY in the pin, which is what makes a release that changes one
  file download one file. The pin is a CLAIM, NOT A PROOF: browsers evict, so
  every read is "try the depot, else fall back to the cold path" — and the cold
  path IS the repair, because the read-through only downloads what is absent.
  `sw.js` no longer touches `/pack/`, the wasm or the pin; those get an early
  `return`, so nothing the engine needs depends on the service worker winning its
  first-visit race.
- **The offline promise is a test, not a hope.** A first visit must leave the
  device able to boot with the network off. The SW cannot manage that alone:
  it isn't controlling the page while the shell loads and it claims the engine
  worker mid-boot, so app code stores its own downloads. **`engine/depot.ts` is
  the only module that touches the Cache API** — pack, wasm and shell all go
  through it, and the invariant is that nothing the engine worker needs may
  depend on being SW-controlled (a bare `fetch()` for one of those works on a
  desktop and fails offline on a phone). `ignoreVary: true` is baked into its
  lookups: responses come back `Vary: Origin` and Vite's `<script crossorigin>`
  requests carry an Origin that a plain fetch does not, which otherwise hides an
  entry from the request it was stored for. Entries the depot writes are
  Responses it constructs, so they carry no Vary at all.
- **The data pack's manifest is the load spec** (`formatVersion: 2`). Each entry
  carries `stage` (`text` | `study` | `analysis`), `seedOnce` for the stock study
  set, `role: "corpusCache"`, and a per-file `hash` over the RAW bytes — raw
  because some hosts serve `.gz` with `Content-Encoding: gzip`, so the loader
  cannot know which form it received. The loader switches on those instead of
  re-deriving tiers from filenames; `scripts/check-web-pack.mjs` (run by
  `pack:data` and CI) validates the shape and re-derives every hash from the
  shipped bytes. The corpus **idxcache must build byte-identically** — it is the
  biggest file and the manifest hashes it, so a nondeterministic cache re-mints
  every URL and re-downloads the whole pack on a release that changed no data.
- **The in-memory home evicts read pack files, and only under `data/`.** The WASI
  shim's `File` copies its input, so the home held a second copy of everything
  (~45 MB). Eviction never touches the user subtree, because `persistUserData`
  derives deletions by diffing that tree against IndexedDB — evicting there would
  permanently delete the reader's data on their next write, and truncate the
  backup zip. `data/kjv-notes.jsonl` can never be evicted either: `load_study`
  re-reads it at every authoring site.

## Verification

- `cargo test --locked -p plumbline-core -p plumbline-layout -p plumbline-rnd -p plumbline-ffi -p plumbline-hydrate`
- `cargo test --locked -p plumbline-rnd` (featureless — must stay compiling)
- `cargo test --locked -p plumbline-rnd --features "bridge morphology concept"`
- After touching `crates/ffi`'s extern surface: regenerate bindings
  (`cargo run -p plumbline-ffi --features bindgen --bin plumbline-bindgen`) — CI fails
  on drift (the check is header ↔ the hand-written Kotlin JNA binding) — and
  keep the wasm-only exports in `crates/ffi/src/wasm.rs` in the bindgen
  exclude list.
- Web: `cd apps/web && npm run check && npm run build`. Full pipeline:
  `npm run pack:data`, `cargo build -p plumbline-ffi --release --target
  wasm32-wasip1`, `npm run pack:wasm`, `npm run build`, `npm run preview`.
- **Touched a Rust crate? The web suite needs the wasm rebuilt, not just the
  bundle.** `npm run build` only re-bundles TypeScript; the engine — and with it
  the theme palette, every core behaviour, and the whole ABI — lives in
  `dist/plumbline_ffi.wasm`. Run `cargo build -p plumbline-ffi --release --target
  wasm32-wasip1 && npm run pack:wasm` before `npm run build`, or Playwright will
  test the last engine you packed and report failures that are stale by
  construction. This has cost time twice (2026-07-29): once on a theme colour that
  looked unchanged, once on removed ABI endpoints that looked still-present.
- **Same rule for Android, and it is easier to miss: gradle does NOT build the
  engine.** The APK embeds whatever `.so` is sitting in
  `apps/android/app/src/main/jniLibs`, and only `cargo ndk` puts it there. A
  gradle build after a crate change succeeds, its unit tests pass (they are JVM
  tests and never load the native library), lint is clean — and the APK you hand
  over runs the engine from whenever you last cross-compiled. Caught 2026-07-30
  with the `.so` **two days stale**, so an on-device UAT would have been testing an
  engine that predated the whole batch. Before building an APK for anyone to run:
  `ANDROID_NDK_HOME=/opt/android-ndk cargo ndk -t arm64-v8a -t x86_64 --platform 26
  -o apps/android/app/src/main/jniLibs build -p plumbline-ffi --release`. To check
  an APK really has it, `unzip -l` it and compare the `.so`'s size with the one on
  disk.
- No 3k-line source files.

## Releases

- Tag `v*` → `.github/workflows/release.yml` builds a signed Android APK
  (arm64-v8a + x86_64) and attaches it to a GitHub Release — the repo is the
  download page — and deploys the PWA to GitHub Pages at
  <https://plumblinebible.org/> (custom domain live 2026-07-25; the old
  glendonjklassen.github.io/plumbline URL 301s there).
- The APK job needs four repo secrets — `ANDROID_KEYSTORE_BASE64`,
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`
  (generation steps are in the workflow header). Without them the job no-ops
  with a warning. The keystore is the app's stable update identity — back it
  up; losing it forces users to uninstall to upgrade. Local APK build:
  `apps/android/gradlew -p apps/android :app:assembleDebug` with
  `JAVA_HOME=java-21-openjdk` (the .so comes from `cargo ndk -t arm64-v8a
  -t x86_64 -o apps/android/app/src/main/jniLibs build -p plumbline-ffi --release`).
