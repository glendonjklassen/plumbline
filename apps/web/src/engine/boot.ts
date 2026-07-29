// Boot sequence: CORE data pack → virtual home → wasm instance → engine open.
// Returns the ready StudyEngine. Its hooks (`onAuthored`, `onReadingWrite`) are
// the CALLER's to wire — engine.worker.ts does it, and it must stay the only
// place: they are single-slot properties, so a handler set here is silently
// overwritten and the debounce nobody can find sits in this file. The caller
// drives a progress UI; the heavy step after download is the first-visit corpus
// parse (the built cache is persisted so later visits skip it).
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
import { buildHome, dropLegacyIdxcache, type VirtualHome } from "./home";
import { assetUrl, fetchManifest, fetchPack, fetchStageLocal, type PackManifest } from "./pack";
import { manifestFromPin, readPin, writePin } from "./pin";
import { PERF } from "./perf";
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
  /** Whether stage 1 came entirely from the depot, with no network request. */
  fromPin: boolean;
}

export async function boot(onPhase: (p: BootPhase) => void): Promise<BootResult> {
  const trace: [string, number][] = [];
  const timed = async <T>(label: string, f: () => T | Promise<T>): Promise<T> => {
    if (!PERF) return f();
    const t0 = performance.now();
    const v = await f();
    trace.push([label, Math.round(performance.now() - t0)]);
    return v;
  };

  onPhase({ phase: "download", fraction: 0 });
  // THE LADDER. First rung that works, wins.
  //
  //  1. A pin whose stage-1 files are all in the depot → zero network requests.
  //     This is the warm boot, and it is the common case.
  //  2. Anything else → the cold path: fetch the manifest, download what is
  //     missing, write a fresh pin.
  //
  // There is deliberately no "repair" rung between them. The depot read-through
  // already downloads only what it does not have, so the cold path IS the repair
  // — one manifest fetch, then just the absent bytes. A partially-evicted device
  // therefore gets one coherent decision instead of a per-file mix.
  const base = assetUrl("");
  let pinned = await timed("pin read (depot)", () => readPin(base));
  let manifest: PackManifest | null = null;
  let pack: Map<string, Uint8Array> | null = null;
  let fromPin = false;

  if (pinned) {
    const m = manifestFromPin(pinned);
    const local = await timed("stage1 read (depot, no network)", () => fetchStageLocal(m, "text"));
    if (local) {
      manifest = m;
      pack = local;
      fromPin = true;
    }
  }

  if (!manifest || !pack) {
    // The text arrives as the parsed-corpus cache — the pack's copy on a first
    // visit, this device's own copy afterwards. Either way the engine never
    // parses JSONL (8.4 s on a 2026 flagship phone; 2026-07-26 trace) and never
    // downloads it.
    const live = await timed("manifest (network)", fetchManifest);
    manifest = live;
    pack = await timed("stage1 fetch+gunzip (text)", () =>
      fetchPack(live, (p) => onPhase({ phase: "download", fraction: p.fraction, detail: p.currentFile })),
    );
    pinned = null; // stale: a fresh pin is written below, once the open succeeds
  }

  onPhase({ phase: "prepare" });
  const stockPaths = new Set(manifest.files.filter((f) => f.seedOnce).map((f) => f.path));
  const home = await timed("virtual home build", () =>
    buildHome(pack, stockPaths),
  );
  const wasm = await timed("wasm compile+instantiate", () => instantiate(home.root));

  onPhase({ phase: "open" });
  // Yield so the "opening" progress message lands before the synchronous
  // parse (rAF on the main thread; a macrotask in the engine worker).
  await new Promise((r) =>
    typeof requestAnimationFrame !== "undefined" ? requestAnimationFrame(() => r(null)) : setTimeout(r, 0),
  );
  // The label says whether the persisted corpus cache was there to skip the
  // 19 MB re-parse — the first question when this stage is slow on a device.
  const engine = await timed(
    home.hadIdxcache ? "engine open (idxcache fast path)" : "engine open (cold corpus parse)",
    () => StudyEngine.open(wasm, "/home"),
  );

  // BEFORE the reader can touch anything. This shell warms in slices, so the
  // engine must never build an index inside a tap — that is the 26,042 ms freeze
  // of 2026-07-28, which also strands every download in flight behind it.
  //
  // Here, and not when the warm starts: the warm begins only after stage 2 is
  // fetched and parsed (~550 ms after text on a phone), and the reader taps
  // inside that window. Deriving the flag from the first warm step shipped once
  // and fixed nothing.
  engine.deferBuilds(true);

  // PIN ONLY AFTER A SUCCESSFUL OPEN. The pin's value is that the next launch can
  // act on it without asking the network, so it must never name a pack that could
  // not actually boot this one. Cold path only — on the fast path it already
  // describes exactly this pack.
  if (!fromPin) await writePin(manifest, base);

  // The corpus cache is the big one: `load_cache` does a single whole-file read
  // and MOVES the bytes into the engine's own buffer, which every unvisited
  // chapter is then decoded out of. The node here is a pure duplicate of ~37 MB.
  const freed = home.evict(["data/kjv.jsonl.idxcache"]);
  if (PERF && freed) trace.push(["home evict after open (KB)", Math.round(freed / 1024)]);

  // Reclaim the legacy IndexedDB copy of the corpus cache, but only now — after
  // an open that PROVED the depot can supply the text. Deleting it before that
  // would take away the one copy a device with an evicted depot still had.
  void dropLegacyIdxcache().then((n) => {
    if (PERF && n) trace.push(["legacy IDB idxcache dropped (KB)", Math.round(n / 1024)]);
  });

  return { engine, wasm, home, manifest, packVersion: manifest.version, trace, fromPin };
}
