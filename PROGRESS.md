# PROGRESS — `rust-rewrite` branch

Where the Rust rebuild stands (2026-07-08 → 07-09). See [PLAN.md](PLAN.md) for
the architecture and the locked decisions; see
[crates/ffi/README.md](crates/ffi/README.md) for the C ABI + bindings.

## Working right now

- **`cargo test`** → **44 tests green** across core/layout/rnd/ffi, no warnings.
- **`OVERLAY_HOME=/home/gjklassen/code/overlay cargo run -p pure-desktop`** →
  a real GTK4 reader window, now with the core's study surface wired in:
  - **Multi-pane**: 1–3 reading columns side by side, each with its own book /
    chapter nav + scroll (per-pane **+**/**✕**); the active pane (gold top accent)
    is what search, cross-references, and the study panel act on.
  - **Ambient weave connectors**: a transparent overlay draws soft gold curves
    between cross-referenced verses whose endpoints are both on screen (scroll-
    aware via GTK `compute_point`) — e.g. Gen 15:6 ↔ Rom 4:3 with Gen ∥ Romans
    open. Endpoint + gutter dots mark the linked verses.
  - John 3 on warm paper, set in the **bundled EB Garamond** shaped by **Pango**
    (registered with fontconfig at startup — no system install; cairo's toy font
    API is gone): **gold verse numbers**, **faint gold underline** on
    Strong's-tagged words, **italic gray** for KJV-supplied words (`of`, `even`),
    correct **¶ paragraph breaks** (vv. 14/16/17/18/22).
  - Header nav: book dropdown + chapter spinner + prev/next, live title.
  - **Double-click a word → its Strong's entry** (lemma, translit, pronunciation,
    definition, KJV renderings) + this verse's **1769 margin notes** + a
    **“N occurrences ▸”** link into that code's **concordance**. (Single click is
    reserved; hit regions align with the glyphs.)
  - **Search box** in the header (word / phrase / reference, multi-tier): a
    reference jumps; hits list as clickable reference links that **navigate and
    scroll** the reader to the verse (with a soft gold band on the target).
  - The **study panel is on-demand**: hidden by default (full-width reader), it
    opens for a word lookup / search / concordance and closes on **Esc** or when
    the search box is cleared.
  - **Zoom** (Ctrl +/− and Ctrl+scroll) and **keyboard** nav (PageUp/Down /
    Space to scroll, Home/End, ←/→ or `[`/`]` for prev/next chapter).
  - **Hover** a Strong's-tagged word for a quick gloss tooltip (code + lemma +
    short definition); the **canon-overview strip** under the panes maps the 66
    books in 8 sections with the OT/NT divide and a pin per pane — click to jump
    the active pane anywhere.
  - Verified visually against the real **31,102-verse** corpus. A cosmetic
    `radv … Vulkan` warning prints on start (GTK renderer fallback; ignore).
- **`pure-ffi` is now the real C ABI** both native shells will consume — not a
  stub. `open → layout chapter (display list, measured via a callback) →
  hit-test → Strong's → occurrences → search → free`, with opaque handles,
  panic-safe boundaries, and stable camelCase-JSON payloads.
  - Verified three ways against the real corpus: a Rust end-to-end test, a
    **C** consumer ([`bindings/c/smoke.c`](crates/ffi/bindings/c/smoke.c)), and a
    **C#/P-Invoke** consumer that runs on this box against the Linux `.so`
    (`dotnet run --project crates/ffi/bindings/csharp/demo -- ../overlay`).
  - Bindings generated from the source: C header (cbindgen) + C# P/Invoke
    (csbindgen), plus a hand-written idiomatic C# wrapper and a Kotlin/JNA
    wrapper. Regenerate with `cargo run -p pure-ffi --features bindgen`.
  - Cross-builds: host `.so`/`.a` ✅, **Windows x86_64 `.dll`** ✅ (mingw).
  - A **6-lens adversarial review** (with 2-skeptic verification per finding)
    found the memory/panic foundation clean and fixed 6 wire-contract defects
    (verse key, exported flag constants, callback totality/NaN clamp, null on
    out-of-range chapter, explicit-null item fields, u32→u16 truncation).

## Crates (all ported faithfully from `../overlay`, read-only reference)

| crate | contents | tests |
|-------|----------|-------|
| `crates/core` | canon (66 books, frozen tok stamp), `VRef`/`refKey`, corpus (JSONL loader + canonical-order validation + chapter index), Strong's (+ occurrence index, proper-noun heuristic), search (4-tier: exact/variant/lemma/typo, + phrase, + reference, + bare-Strong's), weave graph (canonical links, BFS components, union-merge, v2 JSON), **notes loader** (`kjv-notes.jsonl`) | 30 |
| `crates/layout` | reader layout + per-word hit-testing as a platform-agnostic algorithm over an injected `Measure` (GTK shell backs it with cairo) | 3 |
| `crates/rnd` | feature-gated R&D capability flags — **off by default** | 1 |
| `crates/ffi` | **the C ABI** (opaque engine/display-list handles, callback-measured layout, JSON payloads) + generated C/C# bindings + a Kotlin/JNA wrapper | 10 |
| `apps/desktop` | GTK4 + libadwaita shell (gtk4 0.11 / libadwaita 0.9) | — |

Dropped by decision: signed **patches** and **rules** (not ported).

## Toolchain status (updated 2026-07-09)

`rustup` is in and the cross **targets are added** (android arm64/armv7/x86_64,
windows x86_64-gnu + aarch64-gnullvm); **mingw-w64** is present. So:

- **Windows x86_64 `.dll`** cross-builds today. ✅
- **Android `.so`** is blocked only on the **Android NDK + `cargo install
  cargo-ndk`** — without the NDK, `cargo build --target aarch64-linux-android`
  compiles the Rust fine but fails at *link* (falls back to host `ld`).
- **ARM-Windows `.dll`** needs **llvm-mingw** (`aarch64-w64-mingw32-clang`) for
  the `aarch64-pc-windows-gnullvm` target.

## Next steps (ordered)

1. **~~Flesh out `pure-ffi`~~ — DONE.** The one C ABI both shells consume is
   built, tested, cross-built for Windows, and reviewed. See above +
   [crates/ffi/README.md](crates/ffi/README.md).
2. **Native shell apps over the ABI:**
   - **Android (Compose):** once the NDK + cargo-ndk are installed, build the
     `.so` per ABI into `jniLibs/`, then a Gradle/Compose app that loads it via
     the JNA wrapper in [`bindings/kotlin`](crates/ffi/bindings/kotlin). The
     Kotlin binding is already written. (Targets the Pixel 9 Pro Fold.)
   - **Windows (WinUI):** the `.dll` cross-builds here; build the C# app on
     Windows (VM/CI) against the generated P/Invoke shim + the wrapper in
     [`bindings/csharp`](crates/ffi/bindings/csharp). Both are already written
     and the demo runs against the Linux `.so` today as a proof.
3. **Reader** (search / concordance / notes / cross-references / zoom / keyboard /
   **Pango + EB Garamond** / **multi-pane** / **ambient weave connectors** /
   **hover glosses** / **canon strip** all done): remaining — a weave compare card
   + approve/reject; threads + tags (need a `core::thread` port first); a stabler
   verse-scroll than the 50 ms nudge.
4. **Port remaining core**: threads, the notes loader (`kjv-notes.jsonl` — the
   FFI already threads an empty `Notes` through search, ready to fill), weave
   rendering across panes. Then expose threads/weave/notes through the ABI.
5. **R&D layer** (`pure-rnd`): concept engine, embeddings, morphology — behind
   cargo features, surfaced only in "Full study" mode (decision #4).
6. **Data delivery**: bundle KJV + Strong's in-app (the ABI's
   `pure_engine_open_from_bytes` already supports loading from asset bytes);
   optional R&D "packs" download; guided first-run flow.
7. Move the offline Python/SWORD pipeline into `data-prep/`.
8. **CI**: add a step that regenerates the bindings and fails on a diff (catches
   doc-vs-wire drift, the class the ABI review flagged).

## Session note

Set Hyprland window rules (float/center) for class `ca.cavallo.purestudy` to grab
a clean screenshot. They match only this app and clear on Hyprland reload.
