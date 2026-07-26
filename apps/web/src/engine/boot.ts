// Boot sequence: CORE data pack → virtual home → wasm instance → engine open.
// Returns the ready StudyEngine with persistence wired. The caller drives a
// progress UI; the heavy step after download is the first-visit corpus parse
// (the built cache is persisted so later visits skip it).
//
// Everything beyond the corpus (Strong's, cross-refs, the ~17 MB machine-tier
// artifacts) is NOT part of boot — the engine opens without them, same shape
// as the Android APK. The engine worker streams them in afterwards in yield-
// friendly chunks (see engine.worker.ts) so layout/tap RPCs never queue
// behind a long synchronous load on the worker thread.
//
// Every stage is timed into `trace` — the on-device answer to "where do the
// milliseconds actually go on a phone" (surfaced via the bootTrace RPC).

import { instantiate, type WasmEngine } from "./engine";
import { buildHome, type VirtualHome } from "./home";
import { fetchManifest, fetchPack, type PackManifest } from "./pack";
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
  /** Per-stage wall-clock in ms, in execution order (label, ms). */
  trace: [string, number][];
}

export async function boot(onPhase: (p: BootPhase) => void): Promise<BootResult> {
  const trace: [string, number][] = [];
  const timed = async <T>(label: string, f: () => T | Promise<T>): Promise<T> => {
    const t0 = performance.now();
    const v = await f();
    trace.push([label, Math.round(performance.now() - t0)]);
    return v;
  };

  onPhase({ phase: "download", fraction: 0 });
  const manifest = await timed("manifest", fetchManifest);
  const pack = await timed("stage1 fetch+gunzip (corpus)", () =>
    fetchPack(manifest, (p) => onPhase({ phase: "download", fraction: p.fraction, detail: p.currentFile })),
  );

  onPhase({ phase: "prepare" });
  const stockPaths = new Set(manifest.files.filter((f) => f.stock).map((f) => f.path));
  const home = await timed("virtual home build", () => buildHome(pack, stockPaths));
  const wasm = await timed("wasm compile+instantiate", () => instantiate(home.root));

  onPhase({ phase: "open" });
  // Yield so the "opening" progress message lands before the synchronous
  // parse (rAF on the main thread; a macrotask in the engine worker).
  await new Promise((r) =>
    typeof requestAnimationFrame !== "undefined" ? requestAnimationFrame(() => r(null)) : setTimeout(r, 0),
  );
  const engine = await timed("engine open (corpus parse)", () => StudyEngine.open(wasm, "/home"));

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
  return { engine, wasm, home, manifest, packVersion: manifest.version, trace };
}
