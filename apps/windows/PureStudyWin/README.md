# PureStudyWin — the WinUI 3 shell

The Windows shell over the `pure-ffi` C ABI (PLAN.md decision #1). Code-only
WinUI 3 (no XAML files → no XAML compiler; a plain `dotnet build` works) +
**Win2D**: the Rust core lays out each chapter, measuring text through a
DirectWrite-backed callback, and this shell paints the display list and
forwards pointer coordinates back for hit-testing — no study logic in C#.

## Build & run

```powershell
cargo build -p pure-ffi --release        # the engine DLL (copied on build)
dotnet run --project apps/windows/PureStudyWin
```

The data home resolves like the core: `PURE_STUDY_HOME` / `OVERLAY_HOME`, else
the nearest ancestor of the exe (or CWD) containing `data/kjv.jsonl` — running
from the repo finds the in-repo pack.

## What works (first milestone)

- Chapter rendering: bundled EB Garamond, gold verse numbers, italic gray
  KJV-supplied words, faint gold underline on Strong's-tagged words, ¶ breaks;
  centered column capped at a readable measure.
- Nav: book dropdown, chapter box, ‹ › (cross-book), `[` / `]`, ←/→,
  PageUp/Down/Space/Home/End/↑↓, Ctrl+scroll & Ctrl+/− zoom, wheel scroll.
- Double-click a word → study panel: verse text, Strong's entries (lemma,
  translit, pronunciation, derivation, definition, KJV renderings) + the
  concordance as jump links.
- Search (Ctrl+F): references jump (with a soft gold band on the verse);
  word/phrase queries list hits as jump links. Esc closes the panel.

## Not yet (vs. the GTK shell)

Multi-pane, weave connectors + authoring, threads/tags, suggested-weave
review, hover glosses, canon strip, margin notes (needs an ABI addition),
Simple/Full first-run config, session restore, R&D tiers (concept map,
morphology, similar verses). The crash log (if any) lands next to the exe as
`crash.log`.

## Notes

- Unpackaged + `WindowsAppSDKSelfContained` — runs as a plain exe, no MSIX.
- The code-only-WinUI gotcha: the `App` must implement `IXamlMetadataProvider`
  (forwarding to `XamlControlsXamlMetaDataProvider`) and merge
  `XamlControlsResources`; without the provider the process dies at startup
  with `0xc000027b` before any managed exception surfaces.
