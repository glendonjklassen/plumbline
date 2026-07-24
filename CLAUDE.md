# pure-study — working rules

## Product rules

- **Shell parity is bidirectional.** GTK (Linux), WinUI (Windows), and the
  future Compose (Android) shells are one product. A feature added in any
  shell must be implemented in the others in the same change set when
  possible; anything that can't land everywhere yet goes in
  [docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md) under shell deltas.
  That manifest is the parity contract — read it before shell work instead of
  re-surveying the repo.
- `../overlay` (Haskell) is the read-only reference implementation — port
  from it, never modify it.
- Frozen contracts: the on-disk data formats (README.md §Data formats), `kjv1769-tok2`
  tokenization stamp, the camelCase wire JSON (additive evolution only), and
  refKey (`"Gen 1:7"`).

## Working on this machine (Windows ARM64)

- Everything Rust builds/tests natively (`aarch64-pc-windows-msvc`); x64/x86
  DLLs cross-build via explicit `--target`. **GTK does not compile here** —
  changes to `apps/desktop` must mirror existing code patterns and are
  validated by the `desktop-gtk` CI job (ubuntu), not locally.
- **Never drive the UI with synthetic input** (no clicks/SendKeys/window
  automation). Build, then hand over — Glendon tests personally and gives
  feedback. Launching the app is fine only when asked.
- The WinUI app: `cargo build -p pure-ffi --release` then
  `dotnet run --project apps/windows/PureStudyWin`. Data home resolves from
  the repo root (`data/`).

## Verification

- `cargo test --locked -p pure-core -p pure-layout -p pure-rnd -p pure-ffi -p pure-hydrate`
- `cargo test --locked -p pure-rnd` (featureless — must stay compiling)
- `cargo test --locked -p pure-rnd --features "bridge embeddings morphology concept"`
- After touching `crates/ffi`'s extern surface: regenerate bindings
  (`cargo run -p pure-ffi --features bindgen --bin pure-bindgen`) — CI fails
  on drift — and rebuild the release DLL for the app.
- No 3k-line source files (the desktop `main.rs` split is tracked in
  TODO.md §Engineering & data work).

## Releases

- Tag `v*` → `.github/workflows/release.yml` builds self-contained Windows
  apps (arm64/x64/x86, data pack bundled) **and** a signed Android APK
  (arm64-v8a + x86_64), attaching them all to a GitHub Release — the repo is
  the download page. Local dry run:
  `pwsh scripts/package-windows.ps1 -Arch x64 -Version vtest`.
- The Android APK job needs four repo secrets — `ANDROID_KEYSTORE_BASE64`,
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`
  (generation steps are in the workflow header). Without them the job no-ops
  with a warning so the Windows release still ships. The keystore is the app's
  stable update identity — back it up; losing it forces users to uninstall to
  upgrade. Local APK build: `apps/android/gradlew -p apps/android
  :app:assembleDebug` with `JAVA_HOME=java-21-openjdk` (the .so comes from
  `cargo ndk -t arm64-v8a -t x86_64 -o apps/android/app/src/main/jniLibs build
  -p pure-ffi --release`).
