# Plumbline — Android shell (Jetpack Compose)

One of the product's two shells over the same `plumbline-core`, alongside the web
PWA (`apps/web`); the GTK and WinUI desktop shells were retired. No study logic
lives in Kotlin: the app calls the frozen 78-fn C ABI
(`crates/ffi/include/plumbline.h`) through **JNA**. See
[`docs/ANDROID-BOOTSTRAP.md`](../../docs/ANDROID-BOOTSTRAP.md) for the full plan
(fold modes, JNA-not-UniFFI decision, phasing).

This project **cannot be built in the headless dev container** — open
`apps/android/` in Android Studio (it ships a compatible bundled JDK) or build
from the CLI with a JDK 17–21. System `java 26` is too new for AGP.

## First-time setup

1. **Android Studio** generates `gradle/wrapper/gradle-wrapper.jar` on first
   Gradle sync. That binary jar is **not committed** (it is not text and can't
   be reviewed), so a fresh clone must either open once in Android Studio or run
   `gradle wrapper --gradle-version 8.10.2` with a system Gradle to materialize
   it. After that, `./gradlew` works.
2. Create `local.properties` (Android Studio does this automatically) pointing
   at the SDK, e.g. `sdk.dir=/opt/android-sdk`. It is git-ignored.
3. SDK Manager: Platform 35, Platform-Tools, an NDK (r27 LTS or newer), and an
   AOSP emulator image (see the bootstrap doc).

## Build the native core → `.so`

The C ABI is cross-compiled with `cargo-ndk` into `app/src/main/jniLibs/<abi>/`
(git-ignored — never committed). From the **repo root**:

```bash
export ANDROID_NDK_HOME=/opt/android-ndk
cargo ndk -t arm64-v8a -t x86_64 -p 24 \
  -o apps/android/app/src/main/jniLibs build -p plumbline-ffi --release
```

- `arm64-v8a` is the physical device (the target foldable); `x86_64` is the AOSP
  emulator. These match the `ndk.abiFilters` in `app/build.gradle.kts`.
- The library loads via `System.loadLibrary("plumbline_ffi")` (JNA `Native.load`).
- JNA's own `libjnidispatch.so` comes from the `net.java.dev.jna:jna:5.17.0@aar`
  dependency — the `@aar` classifier is required (5.17+ also fixes a 16 KB
  page-size crash). `cargo-ndk` 4.x injects the mandatory 16 KB page alignment;
  verify with `llvm-readelf -l …/libplumbline_ffi.so`.

## Data (bundled as assets)

The reader opens the engine with `OpenFromBytes(assetBytes)` — no writable home
is needed for reading. The frozen data pack is copied out of the repo `data/`
directory into `app/src/main/assets/data/` (also git-ignored) by a Gradle task
that runs automatically before every build:

```bash
./gradlew syncData      # copies kjv.jsonl, strongs.json, kjv-notes.jsonl,
                        # cross-references.tsv into app/src/main/assets/data/
```

`preBuild` depends on `syncData`, so `./gradlew assembleDebug` picks the data up
without a manual step. (~22 MB of assets — fine for an APK.) Personal study
writes, when added, go to the app's private files dir.

## The shared JNA binding

`app/build.gradle.kts` adds `crates/ffi/bindings/kotlin` as a source directory,
so `Plumbline.kt` (package `dev.plumbline.core`: the raw `PlumblineNative`
interface + the `PlumblineLayoutConfig` / `MeasureCallback` JNA types) compiles
straight into the app. That file is the single source of truth for the ABI and
is **not copied** here — edits belong in the FFI crate. The safe PascalCase
wrappers (`StudyEngine` / `Chapter` / `StudyConfig`) live in
`app/src/main/java/dev/plumbline/StudyEngine.kt`, one method per ABI entry point;
the web shell's `apps/web/src/engine/StudyEngine.ts` is its sibling over the same
surface, so a call added for one shell has an obvious twin in the other.

## Run

- **Emulator (dev loop):** Device Manager → Pixel 9 Pro Fold AVD (Emulator
  ≥ 35.2.10). `./gradlew installDebug` or Run in Android Studio. Fold/unfold with
  Ctrl+F / Ctrl+U to make `FoldingFeature` fire.
- **On the phone (GrapheneOS, no cable):** `./gradlew assembleDebug` →
  `app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` (ABI splits, 2026-07-30 —
  `app-x86_64-debug.apk` beside it is for an emulator), move it over via Syncthing / a
  cloud drive / `python -m http.server`, then tap to install; or use wireless
  adb (`adb pair` → `adb connect` → `adb install`). Sign with a consistent key
  (the debug key is fine) — switching keys forces an uninstall on update.

## Version pins (see root `build.gradle.kts` / `app/build.gradle.kts`)

| Component | Version |
|-----------|---------|
| Android Gradle Plugin | 8.7.3 |
| Kotlin + Compose compiler plugin | 2.0.21 |
| Gradle wrapper | 8.10.2 |
| Compose BOM | 2024.10.01 |
| material3.adaptive | 1.2.0 |
| androidx.window | 1.5.0 |
| JNA | 5.17.0 (`@aar`) |
| compileSdk / targetSdk / minSdk | 35 / 35 / 26 |
