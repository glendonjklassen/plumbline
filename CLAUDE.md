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
- Frozen contracts: the on-disk data formats (README.md §Data formats), `kjv1769-tok2`
  tokenization stamp, the camelCase wire JSON (additive evolution only), and
  refKey (`"Gen 1:7"`). The format tags keep their pre-rename names on purpose —
  `pure-note-v1` (`crates/core/src/usernote.rs`) is serialized into every user
  note file and already sits inside shipped backup zips, so renaming it with the
  product would break restore-from-backup; same for `overlay-weave-v2` and
  `overlay-memory-v1`.
- The bundled stock study set lives at `apps/android/app/src/main/assets/stock`
  (threads/tags/weaves); both shells seed it once and user edits/deletions
  stick.

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
  test green — the engine runs on the main thread, so long synchronous engine
  calls freeze the UI.

## Verification

- `cargo test --locked -p plumbline-core -p plumbline-layout -p plumbline-rnd -p plumbline-ffi -p plumbline-hydrate`
- `cargo test --locked -p plumbline-rnd` (featureless — must stay compiling)
- `cargo test --locked -p plumbline-rnd --features "bridge embeddings morphology concept"`
- After touching `crates/ffi`'s extern surface: regenerate bindings
  (`cargo run -p plumbline-ffi --features bindgen --bin plumbline-bindgen`) — CI fails
  on drift (the check is header ↔ the hand-written Kotlin JNA binding) — and
  keep the wasm-only exports in `crates/ffi/src/wasm.rs` in the bindgen
  exclude list.
- Web: `cd apps/web && npm run check && npm run build`. Full pipeline:
  `npm run pack:data`, `cargo build -p plumbline-ffi --release --target
  wasm32-wasip1`, `npm run pack:wasm`, `npm run build`, `npm run preview`.
- No 3k-line source files.

## Releases

- Tag `v*` → `.github/workflows/release.yml` builds a signed Android APK
  (arm64-v8a + x86_64) and attaches it to a GitHub Release — the repo is the
  download page. The PWA deploy (hosting TBD: Azure SWA vs GitHub Pages) will
  join the same workflow once hosting is decided.
- The APK job needs four repo secrets — `ANDROID_KEYSTORE_BASE64`,
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`
  (generation steps are in the workflow header). Without them the job no-ops
  with a warning. The keystore is the app's stable update identity — back it
  up; losing it forces users to uninstall to upgrade. Local APK build:
  `apps/android/gradlew -p apps/android :app:assembleDebug` with
  `JAVA_HOME=java-21-openjdk` (the .so comes from `cargo ndk -t arm64-v8a
  -t x86_64 -o apps/android/app/src/main/jniLibs build -p plumbline-ffi --release`).
