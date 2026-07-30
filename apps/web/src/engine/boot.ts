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
//
// TWO WAITS RUN AT ONCE. The engine binary and the text need nothing from each
// other until the instantiate, so the download of the first starts at the top of
// `boot()` and is collected where it is actually needed. Awaited in order — as
// this did until 2026-07-29 — the whole wasm download was dead time before first
// text, which is the number that matters most on a phone.

import { instantiate, type WasmEngine } from "./engine";
import { depotAvailable, depotHas, depotResponse } from "./depot";
import { buildHome, dropLegacyIdxcache, type VirtualHome } from "./home";
import { assetUrl, fetchManifest, fetchPack, fetchStageLocal, type PackManifest } from "./pack";
import { manifestFromPin, readPin, writePin } from "./pin";
import { PERF } from "./perf";
import { StudyEngine } from "./StudyEngine";

/** Where the engine binary lives — content-addressed on the build id, so a new
 *  build is a new depot entry beside the old one rather than an overwrite.
 *
 *  THREE places have to agree on this string: the prefetch below, the read inside
 *  `instantiate()` (engine.ts), and `pruneToPin`'s keep-set (engine.worker.ts).
 *  Disagreeing is silent and expensive in both directions — a prefetch under the
 *  wrong URL is a second 1.7 MB download, and a keep-set under the wrong URL is
 *  prune deleting the engine — so two of the three read it from here, and
 *  `e2e/boot-overlap.spec.ts` asserts a cold boot requests it EXACTLY ONCE. */
export function engineUrl(): string {
  return assetUrl(`plumbline_ffi.wasm?v=${__BUILD_ID__}`);
}

/** Start the engine binary arriving NOW, beside the stage-1 read.
 *
 *  This warms the DEPOT rather than handing bytes back, because the read site is
 *  inside `instantiate()` and reads through the depot: once these bytes are
 *  stored, that read is a local hit and nothing crosses the network twice.
 *
 *  Skipped when there is no depot to warm (private mode, plain http). Without
 *  somewhere to leave the bytes a prefetch is not an overlap, it is the same
 *  1.7 MB downloaded twice — so the honest thing on those devices is to let the
 *  instantiate fetch it, exactly as before.
 *
 *  Resolves with whether the handoff really happened: a `put` refused for quota
 *  leaves the instantiate to fetch it again, and that belongs in a trace rather
 *  than in a guess. Rejections belong to the caller — see the call site.
 *
 *  ONE depot lookup on a warm boot, which is the common case and the one that has
 *  to stay cheap: the binary is already there, the instantiate's own read will hit
 *  it, and there is nothing to overlap. */
async function prefetchEngine(): Promise<boolean> {
  if (!depotAvailable()) return false;
  const url = engineUrl();
  if (await depotHas(url)) return true;
  await depotResponse(url);
  return depotHas(url);
}

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

  // THE OVERLAP, started before anything else asks the network so the engine
  // binary and the text share the connection instead of queueing on it.
  //
  // ITS REJECTION IS ROUTED TWICE, on purpose. The `catch` here marks the promise
  // handled, so a failed engine fetch cannot surface as an unhandled rejection
  // while nobody is awaiting it (in a worker that is a console error at best and
  // a dead thread at worst, and the reader would see a splash that never moves).
  // The `await` at the instantiate site below still throws, which is the same
  // failure path the awaited `instantiate()` used: out of `boot`, out of the boot
  // RPC, onto the splash with a Retry.
  const engineBytes = prefetchEngine();
  void engineBytes.catch(() => {});

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
  // Collected here, not at the top: near-zero means the overlap paid for itself
  // and the binary arrived while the text was being read; a large number means
  // the engine download IS the critical path, which is a different fix (a smaller
  // binary) and worth being able to tell apart on a real device.
  const handedOff = await timed("engine bytes wait (overlapped)", () => engineBytes);
  if (PERF && !handedOff) trace.push(["engine bytes not stored — instantiate refetches", 1]);
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
