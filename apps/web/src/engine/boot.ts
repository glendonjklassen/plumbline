// Boot sequence: data pack → virtual home → wasm instance → engine open.
// Returns the ready StudyEngine with persistence wired. The caller drives a
// progress UI; the heavy step after download is the first-visit corpus parse
// (the built cache is persisted so later visits skip it).

import { instantiate, type WasmEngine } from "./engine";
import { buildHome, type VirtualHome } from "./home";
import { fetchManifest, fetchPack } from "./pack";
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
  packVersion: string;
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
  // Yield a frame so the "opening" state paints before the synchronous parse.
  await new Promise((r) => requestAnimationFrame(() => r(null)));
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
  return { engine, wasm, home, packVersion: manifest.version };
}
