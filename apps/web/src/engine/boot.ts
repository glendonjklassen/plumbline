// Boot sequence: CORE data pack → virtual home → wasm instance → engine open.
// Returns the ready StudyEngine with persistence wired. The caller drives a
// progress UI; the heavy step after download is the first-visit corpus parse
// (the built cache is persisted so later visits skip it).
//
// The machine-tier artifacts (morphology, concept vectors — ~17 MB raw) are
// NOT part of boot: the engine opens without them (same shape as the Android
// APK, which never bundles them), and loadRndPack() streams them in after
// first paint (TODO #28). The engine runs on the main thread, so the split is
// what keeps the splash short on phones.

import { instantiate, type WasmEngine } from "./engine";
import { buildHome, type VirtualHome } from "./home";
import { fetchManifest, fetchPack, fetchRndPack, fetchStage2Pack, type PackManifest } from "./pack";
import { StudyEngine } from "./StudyEngine";

export interface BootPhase {
  phase: "download" | "prepare" | "open" | "warm";
  /** 0..1 within the download phase; indeterminate elsewhere. */
  fraction?: number;
  detail?: string;
}

export interface BootResult {
  engine: StudyEngine;
  wasm: WasmEngine;
  home: VirtualHome;
  manifest: PackManifest;
  packVersion: string;
}

/** Fetch the stage-2 core files (Strong's, cross-refs, margin notes, bridge)
 *  into the live home — the engine picks them up via load_core_data. */
export async function loadStage2(r: BootResult): Promise<void> {
  const files = await fetchStage2Pack(r.manifest);
  r.home.addFiles(files);
  r.engine.loadCoreData();
}

/** Fetch the deferred machine-tier pack, hand its files to the live home, and
 *  (re)warm — the SIF model only builds once the embedding is in. One long
 *  synchronous engine block at the end (main-thread engine), so callers
 *  schedule this at an idle moment, never during interaction. Idempotent. */
export async function loadRndPack(r: BootResult): Promise<void> {
  if (!r.manifest.files.some((f) => f.rnd)) return;
  const files = await fetchRndPack(r.manifest);
  r.home.addFiles(files);
  r.engine.loadRndData();
  r.engine.warmIndexes();
}

export async function boot(onPhase: (p: BootPhase) => void): Promise<BootResult> {
  onPhase({ phase: "download", fraction: 0 });
  const manifest = await fetchManifest();
  const pack = await fetchPack(manifest, (p) =>
    onPhase({ phase: "download", fraction: p.fraction, detail: p.currentFile }),
  );

  onPhase({ phase: "prepare" });
  const stockPaths = new Set(manifest.files.filter((f) => f.stock).map((f) => f.path));
  const home = await buildHome(pack, stockPaths);
  const wasm = await instantiate(home.root);

  onPhase({ phase: "open" });
  // Yield so the "opening" progress message lands before the synchronous
  // parse (rAF on the main thread; a macrotask in the engine worker).
  await new Promise((r) =>
    typeof requestAnimationFrame !== "undefined" ? requestAnimationFrame(() => r(null)) : setTimeout(r, 0),
  );
  const engine = StudyEngine.open(wasm, "/home");

  // Persistence choreography: any authoring write mirrors the user subtree.
  let pending = false;
  engine.onAuthored = () => {
    if (pending) return;
    pending = true;
    setTimeout(() => {
      pending = false;
      void home.persistUserData();
    }, 50);
  };

  void home.persistIdxcache();
  return { engine, wasm, home, manifest, packVersion: manifest.version };
}
