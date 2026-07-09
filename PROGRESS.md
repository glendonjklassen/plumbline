# PROGRESS — `rust-rewrite` branch

Where the Rust rebuild stopped (overnight session, 2026-07-08 → 07-09). See
[PLAN.md](PLAN.md) for the architecture and the locked decisions.

## Working right now

- **`cargo test`** → **33 tests green** across core/layout/rnd/ffi, no warnings.
- **`OVERLAY_HOME=/home/gjklassen/code/overlay cargo run -p pure-desktop`** →
  a real GTK4 reader window:
  - John 3 on warm paper: **gold verse numbers**, **faint gold underline** on
    Strong's-tagged words, **italic gray** for KJV-supplied words (`of`, `even`),
    correct **¶ paragraph breaks** (vv. 14/16/17/18/22).
  - Header nav: book dropdown + chapter spinner + prev/next, live title.
  - **Click a word → its Strong's entry** (lemma, translit, pronunciation,
    definition, KJV renderings) in the right-hand study panel.
  - Verified visually against the real **31,102-verse** corpus. A cosmetic
    `radv … Vulkan` warning prints on start (GTK renderer fallback; ignore).

## Crates (all ported faithfully from `../overlay`, read-only reference)

| crate | contents | tests |
|-------|----------|-------|
| `crates/core` | canon (66 books, frozen tok stamp), `VRef`/`refKey`, corpus (JSONL loader + canonical-order validation + chapter index), Strong's (+ occurrence index, proper-noun heuristic), search (4-tier: exact/variant/lemma/typo, + phrase, + reference, + bare-Strong's), weave graph (canonical links, BFS components, union-merge, v2 JSON) | 28 |
| `crates/layout` | reader layout + per-word hit-testing as a platform-agnostic algorithm over an injected `Measure` (GTK shell backs it with cairo) | 3 |
| `crates/rnd` | feature-gated R&D capability flags — **off by default** | 1 |
| `crates/ffi` | C ABI stub (version probe) for future UniFFI / csbindgen | 1 |
| `apps/desktop` | GTK4 + libadwaita shell (gtk4 0.11 / libadwaita 0.9) | — |

Dropped by decision: signed **patches** and **rules** (not ported).

## Next steps (ordered)

1. **Windows (WinUI) + Android (Compose) shells** — *you asked to add these.*
   Both are blocked on **one-time installs that need sudo** (I don't run sudo
   unattended):
   - Swap Arch's system `rust` for `rustup` (system rust can't add cross
     targets): `sudo pacman -Rns rust && sudo pacman -S rustup && rustup default stable`.
   - **Android:** install the Android NDK + `cargo install cargo-ndk`, then
     `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.
     Build the FFI `.so` with cargo-ndk; scaffold a Gradle/Jetpack-Compose app
     that loads it via UniFFI-generated Kotlin. (Targets the Pixel 9 Pro Fold.)
   - **Windows:** `sudo pacman -S mingw-w64-gcc` + `rustup target add
     x86_64-pc-windows-gnu aarch64-pc-windows-gnullvm` to cross-build the FFI
     `.dll` from Linux; the WinUI C# app itself is built on Windows (VM/CI) with
     csbindgen. ARM Windows is the `aarch64` target.
   Once `rustup` is in, I can scaffold both app skeletons + binding generation
   and cross-build the core/ffi libs from this machine.
2. **Flesh out `pure-ffi`**: `open corpus → render chapter (display list) →
   hit-test tap` as the one C ABI both native shells consume.
3. **Reader polish**: Pango + the bundled EB Garamond (replace cairo's toy font);
   Ctrl+click gating for Strong's (plain click reserved, as overlay does); hover
   underline; keyboard nav/scroll; multi-pane.
4. **Port remaining core**: threads, the notes loader (`kjv-notes.jsonl`), weave
   rendering across panes.
5. **R&D layer** (`pure-rnd`): concept engine, embeddings, morphology — behind
   cargo features, surfaced only in "Full study" mode (decision #4).
6. **Data delivery**: bundle KJV + Strong's in-app; optional R&D "packs" download;
   guided first-run flow.
7. Move the offline Python/SWORD pipeline into `data-prep/`.

## Session note

Set Hyprland window rules (float/center) for class `ca.cavallo.purestudy` to grab
a clean screenshot. They match only this app and clear on Hyprland reload.
