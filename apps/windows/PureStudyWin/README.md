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

## Feature surface

At parity with the GTK shell: multi-pane reading, ambient weave connectors +
authoring, threads/tags, suggested-weave review, hover glosses, the canon
strip, margin notes, Simple/Full first-run config, session restore, and the
Full-study tiers (word study, renderings lens, authority tiers, concept map,
chord map, constellation, morphology, verses-like-this). The parity contract —
and any shell deltas — is [docs/FEATURE-MANIFEST.md](../../../docs/FEATURE-MANIFEST.md);
read it before shell work. The crash log (if any) lands next to the exe as
`crash.log`.

## Notes

- Unpackaged + `WindowsAppSDKSelfContained` — runs as a plain exe, no MSIX.
- The code-only-WinUI gotcha: the `App` must implement `IXamlMetadataProvider`
  (forwarding to `XamlControlsXamlMetaDataProvider`) and merge
  `XamlControlsResources`; without the provider the process dies at startup
  with `0xc000027b` before any managed exception surfaces.
