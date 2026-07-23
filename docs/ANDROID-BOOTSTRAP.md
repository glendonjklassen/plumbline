# Android shell — bootstrap plan

The third shell: a Jetpack Compose (Kotlin) app over the same `pure-core`, exactly
like the GTK and WinUI shells. No study logic in Kotlin — it calls the frozen C ABI.

Target device: **Google Pixel 9 Pro Fold** (GrapheneOS). Goal: a daily-driver
Bible reader with three fold-aware layout modes.

## Current state
- ✅ `pure-core` (all logic) + the 78-fn C ABI (`pure-ffi`), already built as a `cdylib`.
- ✅ Partial Kotlin/JNA binding: `crates/ffi/bindings/kotlin/PureStudy.kt` (~21/78 fns).
- ✅ `cargo-ndk` + Rust Android targets installed on the Linux dev box.
- ❌ NDK, `apps/android` project, cross-compiled `.so`, Compose UI, fold logic.

## Binding: JNA over the existing C ABI (NOT UniFFI)
JNA consumes the exact same 78 fns C# binds via csbindgen — zero Rust changes,
one flat C ABI stays the single source of truth. UniFFI can't wrap an existing
hand-written C ABI (it generates its own), so adopting it would mean a parallel
second ABI + regenerating C# via an immature tool — rejected. Finish the ~57
remaining `fun`s in `PureStudy.kt` against `crates/ffi/include/pure_study.h`,
keeping the `Pointer`-return + `pure_study_string_free` discipline the scaffold
already models. If the per-word `measure` layout callback ever profiles hot, drop
a single JNI shim for that one callback — not a rewrite.

## Toolchain / install (Arch)
```bash
yay -S android-ndk            # → /opt/android-ndk
export ANDROID_NDK_HOME=/opt/android-ndk
yay -S android-studio         # SDK Manager: Platform 35, Platform-Tools (adb), Emulator, AOSP image
# CLI alt: yay -S android-sdk-cmdline-tools-latest android-sdk-platform-tools
#          sdkmanager "platforms;android-35" "emulator" "ndk;27.2.12479018" "system-images;android-35;default;x86_64"
# cargo-ndk + rust targets already installed.
# JDK: system java 26 is too new for AGP — build in Android Studio (bundled JDK) or `yay -S jdk21-openjdk`.
```

### Cross-compile the core → `.so`
```bash
cargo ndk -t arm64-v8a -t x86_64 -p 24 \
  -o apps/android/app/src/main/jniLibs build -p pure-ffi --release
```
NDK r27 (LTS) or r28+; cargo-ndk 4.x auto-injects the mandatory 16 KB page
alignment (verify with `llvm-readelf -l`). Ship arm64-v8a (device) + x86_64
(emulator). Loaded via `System.loadLibrary("pure_ffi")`; JNA dep
`net.java.dev.jna:jna:5.17.0@aar` (the `@aar` matters; 5.17+ fixes a 16 KB crash).

### Data
Bundle `data/kjv.jsonl` + `strongs.json` (+ notes/xrefs) as app **assets** and open
the engine with `pure_engine_open_from_bytes` (no writable home needed for reading;
personal study data goes to the app's private files dir). ~22 MB of assets — fine.

## The three fold modes
Source of truth: `androidx.window` `WindowInfoTracker` → `FoldingFeature`
(`FLAT`/`HALF_OPENED`, hinge orientation, bounds). Panes: `androidx.compose.
material3.adaptive`. Derive a single `UiMode` from *(width breakpoint + FoldingFeature
present + state)* — **never gate two-pane on width alone** (the Fold's inner display
is ~1:1 and may not clear the 840dp "Expanded" breakpoint).

| Mode | Trigger | Layout |
|------|---------|--------|
| 1. Split vertical | folded/portrait + toggle | Column: Bible over Study (stacked halves) |
| 2. Fullscreen vertical | folded/portrait, single | one pane; toggle Bible ↔ Study |
| 3. Fold fullscreen | `FoldingFeature` present & FLAT | two panes side-by-side: Bible∥Bible or Bible∥Study |

"Closed cover screen" = no `FoldingFeature` reported (just a Compact window) → modes 1/2.

Pins: `androidx.window:window:1.5.0`, `androidx.compose.material3.adaptive:*:1.2.0`
(verify latest; these APIs churn — treat exact member names as version-specific).

## Emulator (dev loop)
Android Studio → SDK Manager: SDK Platform (API 35), Emulator, Platform-Tools, an
**AOSP** system image (no-GMS parity). Device Manager → **Pixel 9 Pro Fold** AVD
(needs Emulator ≥ 35.2.10) + a **Resizable** AVD for quick size checks. Fold/unfold:
toolbar **Ctrl+F / Ctrl+U**, or Extended controls → **Virtual sensors** (hinge angle)
— that makes `FoldingFeature` fire.

## Testing on the phone — emulator-first, sideload good builds, NO USB
Iterate on the emulator; only put a build on the phone once it's worth it. Two
no-cable paths:
1. **Dump the file & tap it** — `./gradlew assembleDebug` → `app-debug.apk`; move it
   over via Syncthing / cloud / a quick `python -m http.server`; open in the
   GrapheneOS Files app → tap → install (grant "install unknown apps" once; no Play
   Protect nag on GrapheneOS).
2. **Wireless adb** (scriptable, still no cable) — Developer options (Owner profile) →
   Wireless debugging → `adb pair <ip>:<port>` → `adb connect` → `adb install app.apk`.

GrapheneOS notes: USB debugging is **Owner-profile only**; prefer **wireless** to
sidestep the USB-C-locked hardening; the missing Google Play Services is irrelevant
for a fully offline app. Sign with a consistent key (debug key is fine) — switching
signing keys forces an uninstall on update.

## Phasing (verifiable-here → on-device)
1. **Toolchain + `.so`** — `cargo ndk` build of the C ABI (verifiable on the Linux box once the NDK is installed).
2. **Finish the JNA binding** (57 more fns) + a JVM smoke test mirroring the C# demo.
3. **Scaffold `apps/android`** (Gradle/Compose) — load the lib, open from asset bytes, render one chapter.
4. **Fold-aware layout** — the 3 modes via the derived `UiMode`. (Needs emulator/device.)
5. **Reach parity** — panes, study panel, search, highlights, weaves.

Steps 1–2 are verifiable headless here; 3–5 compile/run on the dev machine + device
(like WinUI). Record the Android binding/loading approach in
`docs/FEATURE-MANIFEST.md` as a shell delta until parity lands.
