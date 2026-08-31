# `plumbline-ffi` — the one C ABI over the core

A single, flat **C ABI** over `plumbline-core` + `plumbline-layout`. The web
shell binds it compiled to `wasm32-wasip1` and reimplements no study logic. (The
GTK/WinUI desktop shells were retired 2026-07-25 and the Compose/Kotlin one
2026-08-30; the ABI is what survived them, and is what a future native shell
would bind.)

## Shape

- **Opaque handles**: `PlumblineEngine` (loaded corpus + Strong's + search/occurrence
  indices) and `PlumblineDisplayList` (one laid-out chapter). C sees forward-declared
  structs; only the `plumbline_*` functions touch them.
- **Primitives** for scalars (chapter numbers, coordinates).
- **JSON** (NUL-terminated UTF-8) for every structured return value — the lowest
  common denominator across languages, tiny and additively evolvable. Schemas
  live in [`src/wire.rs`](src/wire.rs) and are the frozen contract.
- **Layout stays in Rust**: `plumbline_engine_layout_chapter` takes a
  `PlumblineMeasureFn` callback, so the shared greedy line-breaker measures text
  with the platform's own engine (canvas `measureText` on the web) while the
  per-word hit-region bookkeeping is written once.

## Memory & safety contract

- Every `char*` a `plumbline_*` function returns is caller-owned — free it with
  `plumbline_string_free`. A null return means "no value" or an error.
- Every handle is freed with its `*_free` (`plumbline_engine_free`,
  `plumbline_layout_free`). Freeing null is a no-op.
- Input pointers are borrowed for the call only; strings must be valid UTF-8.
- Every entry point is wrapped in `catch_unwind` — a Rust panic never unwinds
  across the boundary. **The `PlumblineMeasureFn` callback must be total**: it must
  not throw/panic (a foreign exception unwinding out of it is UB) and should
  return a finite, non-negative width (NaN/negative is clamped to 0).
- A `*const PlumblineEngine` is safe to share across threads for these read-only
  calls.

## The surface

| Function | Returns |
|----------|---------|
| `plumbline_version` / `plumbline_string_free` | version string / free any returned string |
| `plumbline_engine_open(home)` / `plumbline_engine_open_from_bytes(...)` | open from a data dir, or from bundled asset bytes |
| `plumbline_engine_free` | release the engine |
| `plumbline_engine_toc_json` | `{books:[{id,name,chapters}]}` |
| `plumbline_engine_chapter_count(book)` | chapter count (≥1) |
| `plumbline_engine_verse_json(ref)` / `plumbline_engine_token_json(ref, i)` | one verse / one token |
| `plumbline_engine_layout_chapter(book, ch, cfg, measure, ctx)` | a `PlumblineDisplayList` handle (null past the end) |
| `plumbline_layout_to_json` / `_height` / `_width` / `_item_count` | paint data + extents |
| `plumbline_layout_hit_test_json(dl, x, y)` | the word at a point, or null |
| `plumbline_layout_free` | release the display list |
| `plumbline_engine_strongs_json(code)` / `_strongs_occurrences_json(code)` | dictionary entry / concordance |
| `plumbline_engine_search_json(query)` | multi-tier search: `goto` or `hits` |
| `plumbline_engine_threads_json` / `_tags_json` / `_verse_xrefs_json(ref)` / `_suggested_weaves_json` | read personal study data, a verse's weave partners, and the suggested-weave review queue |
| `plumbline_engine_thread_add` / `_tag_add` / `_tag_remove` / `_weave_add_link` | **author** study data (null on success, else an owned error string; needs an engine opened from a home dir) |
| `plumbline_engine_bridge_partners_json(code)` | R&D: fused OT↔NT bridge partners with provenance + trust prior |
| `plumbline_engine_morph_json(ref, tok)` | R&D: a token's morphology parse + gloss |
| `plumbline_engine_weave_approve(index)` / `_weave_reject(index)` | **review** a suggested weave by its `suggested_weaves_json` ordinal — approve promotes it into `weaves/` (all links approved), reject deletes it |

Authoring writes go through `core::store`'s cross-platform atomic write (temp
sibling → fsync → rename); the caller supplies UTC timestamps. An engine opened
via `plumbline_engine_open_from_bytes` has no home and returns an error from the
authoring calls (study data is read-only).

The **R&D** reads consume the offline artifacts loaded at open (concept
morphology; the bridge's etymology layer works from the dictionary alone, its
external witnesses need a home). Each returns null when its artifact is absent,
so a shell shows the section exactly when it exists; no training happens across
the boundary.

Token flag bits are exported as `PLUMBLINE_FLAG_ADDED/DIVINE/TITLE/PARA`.

## Bindings

Regenerate the committed C header from the Rust source (a host-only tool, behind
a feature so a plain build/cross-build never pulls it):

```sh
cargo run -p plumbline-ffi --features bindgen --bin plumbline-bindgen
```

[`include/plumbline.h`](include/plumbline.h) (cbindgen) is the ABI's reference;
[`bindings/c/smoke.c`](bindings/c/smoke.c) is a full consumer. The wasm-only
exports in [`src/wasm.rs`](src/wasm.rs) are excluded from it by name — cbindgen
does not evaluate `cfg`.

## Building the native library

```sh
cargo build -p plumbline-ffi --release                                 # host .so/.a
cargo build -p plumbline-ffi --release --target wasm32-wasip1          # the web engine
```

## Tests

`cargo test -p plumbline-ffi` drives the whole ABI from Rust exactly as a foreign
caller would (open-from-bytes → layout via a C callback → hit-test → Strong's →
search → free). `smoke.c` exercises the same surface against the real
31,102-verse corpus through an actual C consumer.
