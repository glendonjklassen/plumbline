# `pure-ffi` — the one C ABI the native shells consume

This crate is the single, flat **C ABI** over `pure-core` + `pure-layout`
(decision #1: native-per-platform over a shared Rust core). Every shell —
GTK4/Linux today, WinUI/C# and Compose/Kotlin later — binds to *these* functions
and reimplements no study logic. The GTK desktop app links the core crates
directly and skips this boundary; Windows and Android cross it.

## Shape

- **Opaque handles**: `PureEngine` (loaded corpus + Strong's + search/occurrence
  indices) and `PureDisplayList` (one laid-out chapter). C sees forward-declared
  structs; only the `pure_*` functions touch them.
- **Primitives** for scalars (chapter numbers, coordinates).
- **JSON** (NUL-terminated UTF-8) for every structured return value — the lowest
  common denominator across C#, Kotlin, Swift and JS, tiny and additively
  evolvable, and the same shape a future sync SaaS will speak. Schemas live in
  [`src/wire.rs`](src/wire.rs) and are the frozen contract.
- **Layout stays in Rust**: `pure_engine_layout_chapter` takes a
  `PureMeasureFn` callback, so the shared greedy line-breaker measures text with
  each platform's own engine (Pango / DirectWrite / Android) while the per-word
  hit-region bookkeeping is written once.

## Memory & safety contract

- Every `char*` a `pure_*` function returns is caller-owned — free it with
  `pure_study_string_free`. A null return means "no value" or an error.
- Every handle is freed with its `*_free` (`pure_engine_free`,
  `pure_layout_free`). Freeing null is a no-op.
- Input pointers are borrowed for the call only; strings must be valid UTF-8.
- Every entry point is wrapped in `catch_unwind` — a Rust panic never unwinds
  across the boundary. **The `PureMeasureFn` callback must be total**: it must
  not throw/panic (a foreign exception unwinding out of it is UB) and should
  return a finite, non-negative width (NaN/negative is clamped to 0).
- A `*const PureEngine` is safe to share across threads for these read-only
  calls.

## The surface

| Function | Returns |
|----------|---------|
| `pure_study_version` / `pure_study_string_free` | version string / free any returned string |
| `pure_engine_open(home)` / `pure_engine_open_from_bytes(...)` | open from a data dir, or from bundled asset bytes |
| `pure_engine_free` | release the engine |
| `pure_engine_toc_json` | `{books:[{id,name,chapters}]}` |
| `pure_engine_chapter_count(book)` | chapter count (≥1) |
| `pure_engine_verse_json(ref)` / `pure_engine_token_json(ref, i)` | one verse / one token |
| `pure_engine_layout_chapter(book, ch, cfg, measure, ctx)` | a `PureDisplayList` handle (null past the end) |
| `pure_layout_to_json` / `_height` / `_width` / `_item_count` | paint data + extents |
| `pure_layout_hit_test_json(dl, x, y)` | the word at a point, or null |
| `pure_layout_free` | release the display list |
| `pure_engine_strongs_json(code)` / `_strongs_occurrences_json(code)` | dictionary entry / concordance |
| `pure_engine_search_json(query)` | multi-tier search: `goto` or `hits` |
| `pure_engine_threads_json` / `_tags_json` / `_verse_xrefs_json(ref)` / `_suggested_weaves_json` | read personal study data, a verse's weave partners, and the suggested-weave review queue |
| `pure_engine_thread_add` / `_tag_add` / `_tag_remove` / `_weave_add_link` | **author** study data (null on success, else an owned error string; needs an engine opened from a home dir) |
| `pure_engine_concept_neighbours_json(code, k)` / `_bridge_partners_json(code)` | R&D: concept embedding neighbours (near + cross-testament); fused OT↔NT bridge partners with provenance + trust prior |
| `pure_engine_morph_json(ref, tok)` / `_similar_verses_json(ref, k)` | R&D: a token's morphology parse+gloss; SIF "verses like this" (lazy-built, cached) |
| `pure_engine_weave_approve(index)` / `_weave_reject(index)` | **review** a suggested weave by its `suggested_weaves_json` ordinal — approve promotes it into `weaves/` (all links approved), reject deletes it |

Authoring writes go through `core::store`'s cross-platform atomic write (temp
sibling → fsync → rename); the caller supplies UTC timestamps. An engine opened
via `pure_engine_open_from_bytes` has no home and returns an error from the
authoring calls (study data is read-only).

The **R&D** reads consume the offline artifacts loaded at open (concept
embeddings, morphology; the bridge's etymology layer works from the dictionary
alone, its external witnesses need a home). Each returns null when its artifact
is absent, so a shell shows the section exactly when it exists; no training
happens across the boundary. `similar_verses` builds the SIF model lazily on
first call and caches it.

Token flag bits are exported as `PURE_FLAG_ADDED/DIVINE/TITLE/PARA`.

## Bindings

Regenerate the committed artifacts from the Rust source (host-only tools, behind
a feature so a plain build/cross-build never pulls them):

```sh
cargo run -p pure-ffi --features bindgen --bin pure-bindgen
```

- **C** — [`include/pure_study.h`](include/pure_study.h) (cbindgen). See
  [`bindings/c/smoke.c`](bindings/c/smoke.c) for a full consumer.
- **C# / WinUI** — [`bindings/csharp/PureStudyNative.g.cs`](bindings/csharp/PureStudyNative.g.cs)
  (csbindgen, generated) + [`PureStudy.cs`](bindings/csharp/PureStudy.cs) (the
  idiomatic hand-written wrapper). Runnable demo in
  [`bindings/csharp/demo/`](bindings/csharp/demo/):
  `dotnet run --project crates/ffi/bindings/csharp/demo -- ../overlay`.
- **Kotlin / Android** — [`bindings/kotlin/PureStudy.kt`](bindings/kotlin/PureStudy.kt),
  a JNA wrapper over the same C ABI (scaffold; builds inside the Gradle app once
  the `.so` is produced — see below).

## Building the native library

```sh
cargo build -p pure-ffi --release                                 # host .so/.a
cargo build -p pure-ffi --release --target x86_64-pc-windows-gnu  # Windows .dll (mingw)
```

**ARM-Windows** builds natively: on an ARM64 Windows box the default toolchain
is `stable-aarch64-pc-windows-msvc`, and a plain
`cargo build -p pure-ffi --release` links the ARM64 `.dll` — verified, tests
green and the C# demo running against it. The only prerequisite is the VS
Build Tools **“C++ ARM64/ARM64EC build tools”** component (the SDK alone isn't
enough — without it rustc falls back to whatever `link.exe` is on PATH).
Cross-compiling the same `.dll` *from Linux* instead needs **llvm-mingw**
(`aarch64-w64-mingw32-clang`) for the `aarch64-pc-windows-gnullvm` target.

**Android** is blocked on a one-time toolchain install (the Rust cross
*targets* are already added):

- Android `.so`: needs the **Android NDK** + `cargo install cargo-ndk`. Without
  it, `cargo build --target aarch64-linux-android` compiles but fails at link
  (falls back to the host `ld`). Then:
  `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o app/src/main/jniLibs build -p pure-ffi --release`.

## Tests

`cargo test -p pure-ffi` drives the whole ABI from Rust exactly as a foreign
caller would (open-from-bytes → layout via a C callback → hit-test → Strong's →
search → free). The `smoke.c` and C# demo exercise the same surface against the
real 31,102-verse corpus through actual C and .NET consumers.
