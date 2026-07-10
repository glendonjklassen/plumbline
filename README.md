# pure-study

A cross-platform, KJV-only Bible-study tool built in Rust. Successor to *overlay*.

## Workspace layout

| Crate | What it is | Portable? |
|-------|------------|-----------|
| `crates/core` | Pure domain: corpus, Strong's, search, weaves, tags, config, cross-platform atomic store | ✅ pure Rust |
| `crates/layout` | Greedy line-breaker + hit-region bookkeeping (measures via a callback) | ✅ pure Rust |
| `crates/rnd` | Feature-gated R&D: etymology bridge, concept embeddings, morphology, keyness/leitwort, witness | ✅ pure Rust |
| `crates/ffi` | The single flat **C ABI** the native shells consume (cdylib) | ✅ pure Rust |
| `crates/hydrate` | `pure-hydrate` CLI: copy/check the data + ML-artifact packs into a data home | ✅ pure Rust |
| `apps/desktop` | GTK4 + libadwaita reader (Linux) | ⚠️ needs system GTK4 |

The five portable crates are dependency-light pure Rust and build on Linux,
macOS, and Windows (x86_64 **and** ARM64). All file I/O is cross-platform:
atomic temp→fsync→rename writes and per-OS config/data/cache dirs. The GTK
desktop shell is Linux-first; Windows/Android get their own native shells over
the C ABI (WinUI/C#, Compose/Kotlin) — not yet built.

## Build & test

```sh
# Portable crates — anywhere Rust runs:
cargo test -p pure-core -p pure-layout -p pure-rnd -p pure-ffi -p pure-hydrate
cargo test -p pure-rnd --features "bridge embeddings morphology concept"

# The GTK reader (Linux, needs GTK4 + libadwaita dev packages):
cargo run --release -p pure-desktop
```

The reader resolves its data home from `PURE_STUDY_HOME` / `OVERLAY_HOME`, then
the CWD tree, then the exe dir, then the per-user data directory. Hydrate one
with `cargo run -p pure-hydrate -- copy --from <overlay-tree> --to <home>`.

## Pulling & building on ARM64 Windows

The portable crates (everything except the GTK reader) build natively on an
ARM64 Windows machine — this is validated in CI (`windows-arm64-dll`, an
`aarch64-pc-windows-msvc` cdylib link via cargo-xwin) on every push.

On the ARM Windows box:

```powershell
# 1. Rust toolchain (rustup selects aarch64-pc-windows-msvc by default here).
#    Install the VS Build Tools "Desktop development with C++" workload first
#    (provides the MSVC linker).
git clone ssh://git@github.com/glendonjklassen/pure-study.git
cd pure-study

# 2. Build the C ABI .dll the native Windows shell links against:
cargo build --release -p pure-ffi
#    → target\release\pure_ffi.dll  (+ crates\ffi\include\pure_study.h,
#      crates\ffi\bindings\csharp\PureStudyNative.g.cs for P/Invoke)

# 3. Sanity-check the whole portable stack:
cargo test -p pure-core -p pure-layout -p pure-rnd -p pure-ffi -p pure-hydrate
```

The GTK reader (`pure-desktop`) is **not** expected to build here — it needs a
system GTK4 runtime, which is impractical on Windows ARM. The Windows GUI is the
planned native WinUI shell over `pure_ffi.dll`; until it lands, ARM Windows is a
build/library target (the C ABI + CLI), not a place the desktop reader runs.

## CI

Every push runs: portable-crate tests, the R&D-feature tests, an FFI
binding-drift guard (regenerates the C header + C# P/Invoke shim and fails on
any diff), and Windows cross-builds of the C ABI for **x86_64** (mingw) and
**ARM64** (MSVC via cargo-xwin).
