# PROGRESS — `rust-rewrite` branch

Where the Rust rebuild stands (2026-07-08 → 07-09). See [PLAN.md](PLAN.md) for
the architecture and the locked decisions; see
[crates/ffi/README.md](crates/ffi/README.md) for the C ABI + bindings.

## Working right now

- **`cargo test`** → **70 tests green** across core/layout/rnd/ffi (81 with
  `-p pure-rnd --features "bridge embeddings morphology"`), no warnings.
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
  - **Threads** + **Tags** header buttons browse personal study collections:
    a thread lists its passages (snapshot preview + note) as jump links; a tag
    lists its verses/concepts; a verse's tags also show in its word study.
  - **Suggested** header button opens the **weave review queue**: every proposal
    under `weaves/suggested` with its links as jump links and **✓ approve** /
    **✕ reject** actions — approve promotes it into `weaves/` (all links
    approved, merging a same-named weave) and drops the suggestion; reject
    deletes it. Both go through the cross-platform `core::store`.
  - **Authoring**: a word study offers **＋ tag verse** and **＋ add to thread**
    (a modal name prompt → create-or-append → atomic write → reload), and **✕**
    untags a verse. **Weave authoring**: single-click a word in each of two panes
    to pin endpoints (blue band), then **＋ link** in the header weaves them into
    a named weave — the new connector line appears at once. All writes go through
    the cross-platform `core::store` layer.
  - **Guided first run + study mode** (decision #4): the first launch asks
    **Simple reader** vs **Full study**; Simple hides the whole study/authoring
    surface for a clean reader, Full unlocks it. A toolbar toggle switches
    anytime; the choice + live zoom size persist in a cross-platform config
    (`core::config`; XDG / `%APPDATA%` / macOS App Support).
  - **Runs without `OVERLAY_HOME`**: `core::home` resolves the data dir (env →
    CWD-if-a-tree → next to the exe → per-user data dir) and the app logs which.
  - **Full-study extras** (Simple hides them): **word-span** weave links (pin a
    word, widen the span; the link records spanA/spanB), a **weave compare card**
    (linked passages side by side, span words bold), **entry-note** editing
    (thread notes, per-entry notes, weave notes), the **TSK topical
    cross-reference** tier (`core::crossref`; ~344k refs, vote-ranked), the
    **OT↔NT etymology bridge** (Strong's-derived), **concept embeddings**
    (≈ concepts near + ≈ across the testaments), **SIF "verses like this"**, and
    **morphology** (the token's Hebrew/Greek parse, e.g. Gen 1:1 "created" → "Qal
    perfect, 3rd masculine singular"). The R&D tiers load offline artifacts and
    absent-degrade; nothing is trained in-app (see [data-prep](data-prep/README.md)).
  - Verified visually against the real **31,102-verse** corpus. A cosmetic
    `radv … Vulkan` warning prints on start (GTK renderer fallback; ignore).
- **`pure-ffi` is now the real C ABI** both native shells will consume — not a
  stub. `open → layout chapter (display list, measured via a callback) →
  hit-test → Strong's → occurrences → search → free`, with opaque handles,
  panic-safe boundaries, and stable camelCase-JSON payloads.
  - Now also **reads + authors study data**: threads/tags/verse-xrefs +
    suggested-weaves read, plus `thread_add` / `tag_add` / `tag_remove` /
    `weave_add_link` and suggested-weave `weave_approve` / `weave_reject` writes
    (null = success; through the cross-platform `core::store`; needs a home
    dir), so the Windows/Android shells can author too — not just the GTK app.
  - Verified three ways against the real corpus: a Rust end-to-end test (incl. an
    authoring round-trip from a temp home), a **C** consumer
    ([`bindings/c/smoke.c`](crates/ffi/bindings/c/smoke.c)), and a **C#/P-Invoke**
    consumer that runs on this box against the Linux `.so`
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
| `crates/core` | canon (66 books, frozen tok stamp), `VRef`/`refKey`, corpus (JSONL loader + canonical-order validation + chapter index), Strong's (+ occurrence index, proper-noun heuristic), search (4-tier: exact/variant/lemma/typo, + phrase, + reference, + bare-Strong's), weave graph (canonical links, BFS components, union-merge, v2 JSON, **suggested-weave approve/reject**, notes editing), notes loader (`kjv-notes.jsonl`), **threads** + **tags** (load, membership, **authoring**: add/remove/edit notes + slugged file paths), **`crossref`** (TSK topical tier parser/index), **`config`** (study mode + body size, cross-platform), **`home`** (data-dir resolution), **`store`** (cross-platform atomic write: temp-sibling → fsync → rename) | 54 |
| `crates/layout` | reader layout + per-word hit-testing as a platform-agnostic algorithm over an injected `Measure` (GTK shell backs it with cairo) | 3 |
| `crates/rnd` | feature-gated R&D capabilities — **off by default**. `bridge`: OT↔NT etymology (Strong's-derived) **fused** with external witnesses (LXX/Abbott-Smith/TIPNR) weighted by trust priors. `embeddings`: concept-vector loader + cosine/cross neighbours + SIF "verses like this". `morphology`: OSHM/Robinson parse-code → gloss + sidecar. All *consume* offline artifacts (see [data-prep](data-prep/README.md)); no training in-app. Quotation detection awaits more hydrated inputs. | 1 (13 w/ all features) |
| `crates/hydrate` | `pure-hydrate` CLI: `copy` the pack into a home + `check` each artifact by loading it (verse/entry counts, TSK coverage, embedding dim/alignment, morphology coverage, fused-bridge link count) | — |
| `crates/ffi` | **the C ABI** (opaque engine/display-list handles, callback-measured layout, JSON payloads, **study-data read + authoring writes** incl. suggested-weave approve/reject + notes editing) + generated C/C# bindings + a Kotlin/JNA wrapper | 12 |
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
3. **~~Reader~~ — at parity for Linux.** Search, concordance, notes,
   cross-references, zoom, keyboard, Pango + EB Garamond, multi-pane, ambient
   weave connectors, hover glosses, canon strip, suggested-weave approve/reject,
   the weave compare card, and a stabler layout-driven verse-scroll are all done.
4. **~~Study-data authoring~~ — DONE (both GTK + C ABI).** Threads, tags, and
   weaves create/append/remove/edit-notes with atomic cross-platform writes,
   suggested-weave approve/reject, and **word-span** link selection (spanA/spanB).
   The guided first-run + Simple/Full mode landed too (decision #4).
5. **R&D layer** (`pure-rnd`) — the pack-free and pack-driven tiers are ported
   and surfaced in Full mode: the **etymology bridge** (`bridge`), **concept
   embeddings** + **SIF "verses like this"** (`embeddings`), **morphology**
   (`morphology`), plus the **TSK cross-references** in `core::crossref`. Each
   *consumes* an offline artifact (no training in-app). Still pending: the fused
   multi-source bridge + calibrated trust model, and cross-testament quotation
   detection (need more hydrated inputs — same port-consumer-and-ship pattern).
6. **Data delivery**: `core::home` finds a hydrated tree without `OVERLAY_HOME`;
   the ABI's `pure_engine_open_from_bytes` loads from asset bytes. Remaining —
   actually shipping the corpus + R&D pack with the app (bundle/download).
7. **~~data-prep~~ — documented.** The R&D artifacts, their provenance, and the
   build-once (no-GPU) reproduction path are recorded in
   [data-prep/README.md](data-prep/README.md); the generators stay in the
   offline Python (overlay `ml/` + `pipelines/`).
8. **~~CI~~ — DONE.** [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
   tests the portable crates (+ the `bridge` feature), regenerates the bindings
   and fails on drift, and cross-builds the Windows `.dll`.
9. **Native shell apps over the ABI** (gated on environment, not code):
   - **Android (Compose):** needs the **NDK + cargo-ndk** to build the `.so`
     into `jniLibs/`, then a Gradle/Compose app over the Kotlin/JNA wrapper.
   - **Windows (WinUI):** the `.dll` cross-builds here; build the C# app on a
     Windows host against the P/Invoke shim + wrapper (both already written; the
     demo runs against the Linux `.so` today as a proof).

## Session note

Set Hyprland window rules (float/center) for class `ca.cavallo.purestudy` to grab
a clean screenshot. They match only this app and clear on Hyprland reload.
