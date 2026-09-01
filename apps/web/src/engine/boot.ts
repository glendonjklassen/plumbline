// Boot sequence: core data pack → virtual home → wasm instance → engine open, and the
// ready StudyEngine back. The heavy step is the first-visit corpus parse; the built
// cache is persisted, so later visits skip it. Everything beyond the corpus (Strong's,
// cross-refs, the ~17 MB machine-tier artifacts) is the engine worker's to stream in
// afterwards, in yield-friendly chunks.
//
// The engine's hooks (`onAuthored`, `onReadingWrite`) are single-slot properties, so
// engine.worker.ts must stay the only place that wires them. Every stage is timed into
// `trace` (surfaced via the bootTrace RPC).

import { instantiate, type WasmEngine } from "./engine";
import { depotAvailable, depotHas, depotResponse } from "./depot";
import { buildHome, dropLegacyIdxcache, installedOptional, type VirtualHome } from "./home";
import {
  assetUrl,
  corpusRoleFor,
  devicePackFiles,
  hasOptional,
  isCorpusRole,
  isOtherCorpus,
  type PackFile,
  fetchManifest,
  fetchPack,
  fetchStageLocal,
  type PackManifest,
} from "./pack";
import { manifestFromPin, pinIsFromAnOlderBuild, readPin, writePin } from "./pin";
import { configLoad, i18nSetLanguage, StudyEngine } from "./StudyEngine";

/** Where the engine binary lives — content-addressed on the build id. The prefetch
 *  below, the read inside `instantiate()` (engine.ts) and `pruneToPin`'s keep-set
 *  (engine.worker.ts) must all agree on this string, and disagreeing is silent: a wrong
 *  URL is either a second 1.7 MB download or prune deleting the engine. */
export function engineUrl(): string {
  return assetUrl(`plumbline_ffi.wasm?v=${__BUILD_ID__}`);
}

/** Start the engine binary arriving now, beside the stage-1 read. Warms the depot
 *  rather than handing bytes back, since the read inside `instantiate()` goes through
 *  the depot too. Skipped without a depot (private mode, plain http), where a prefetch
 *  would just be the same 1.7 MB downloaded twice. Resolves with whether the handoff
 *  happened: a `put` refused for quota leaves the instantiate to fetch it again. */
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
  /** The corpus role this boot inflated (`corpusCache` or `corpus:<code>`) — what
   *  stage 1 actually used, not a value recomputed later from a setting. */
  corpusRole: string;
  /** This boot's manifest came from a pin an older build wrote, so it may not list
   *  every file this code expects. `backgroundLoad` refreshes it before stage 2 when so
   *  — and only then: a warm boot on an unchanged release asks the network nothing. */
  staleManifest: boolean;
}

/** Whether this pack carries the corpus behind `role` at all. Every Bible ships, so
 *  this is a question for the manifest, not for the reader's install history. */
function manifestHasCorpus(manifest: PackManifest, role: string): boolean {
  return manifest.files.some((f) => f.role === role);
}

export async function boot(
  onPhase: (p: BootPhase) => void,
  locale = "",
  lang = "",
  sharedLang = "",
): Promise<BootResult> {
  const trace: [string, number][] = [];
  // Deliberately not PERF-gated: the trace is a flight recorder, not a measurement.
  // The e2e suite asserts against it and `settleBackground` polls it, so every trace
  // push in `engine.worker.ts` is ungated too. One clock pair per boot stage.
  const timed = async <T>(label: string, f: () => T | Promise<T>): Promise<T> => {
    const t0 = performance.now();
    const v = await f();
    trace.push([label, Math.round(performance.now() - t0)]);
    return v;
  };

  // Before anything else asks the network, so the binary and the text share the
  // connection. The `catch` only marks the promise handled while nobody awaits it; the
  // `await` at the instantiate site below still throws, onto the splash with a Retry.
  const engineBytes = prefetchEngine();
  void engineBytes.catch(() => {});

  // Prepare, not download: the ladder below has decided nothing yet, and a warm boot
  // never asks the network at all. `download` is announced only at the cold rung.
  onPhase({ phase: "prepare" });
  // The ladder; first rung that works wins. (1) a pin whose stage-1 files are all in
  // the depot → zero network requests, the common case; (2) the cold path — fetch the
  // manifest, download what is missing, write a fresh pin. No "repair" rung between
  // them: the depot read-through downloads only what it lacks, so the cold path IS the
  // repair.
  const base = assetUrl("");
  // Read before anything is fetched: a second language's corpus is an `optional`
  // entry, so nothing on the stage-1 path would carry it otherwise.
  const taken = await timed("optional markers (idb)", installedOptional);
  const mine = (f: PackFile) => hasOptional(taken, f);
  // Which corpus to inflate — see `corpusRoleFor`. All of them stay in the pin and the
  // depot; this only decides which is gunzipped into the home, worth ~28 MB of work and
  // memory before first text. `lang` and `locale` are handed in by the main thread,
  // which is the only one with `localStorage` and the page's `navigator.language`.
  //
  // A function of a manifest rather than a value computed here: the pin's manifest and
  // the network's can disagree about which languages exist, so the answer must come
  // from whichever this boot loads from — deciding up front against a manifest not yet
  // read is how a boot skips every corpus, English included. The corpus arrives through
  // `also` because stage 1 fetches `text`, and other Bibles are not on that stage.
  // `sharedLang` is last: it only decides for a reader who has none of their own,
  // and `lang` (this device's last resolved code) is empty exactly then.
  const corpusFor = (m: PackManifest) =>
    corpusRoleFor(lang || sharedLang, locale, (role) => manifestHasCorpus(m, role));
  const stage1 = (m: PackManifest) => {
    const want = corpusFor(m);
    return {
      want,
      also: (f: PackFile) => mine(f) || (f.role !== undefined && f.role === want),
      skip: (f: PackFile) => isOtherCorpus(f, want),
    };
  };
  let pinned = await timed("pin read (depot)", () => readPin(base));
  let manifest: PackManifest | null = null;
  let pack: Map<string, Uint8Array> | null = null;
  let fromPin = false;
  let wantCorpus = "corpusCache";

  if (pinned) {
    const m = manifestFromPin(pinned);
    const { want, also, skip } = stage1(m);
    const local = await timed("stage1 read (depot, no network)", () => fetchStageLocal(m, "text", also, skip));
    if (local) {
      manifest = m;
      pack = local;
      fromPin = true;
      wantCorpus = want;
    }
  }

  if (!manifest || !pack) {
    // The cold rung, and the only place a download really happens.
    onPhase({ phase: "download", fraction: 0 });
    // The text arrives as the parsed-corpus cache, so the engine never parses JSONL
    // and never downloads it.
    const live = await timed("manifest (network)", fetchManifest);
    manifest = live;
    const { want, also, skip } = stage1(live);
    wantCorpus = want;
    pack = await timed("stage1 fetch+gunzip (text)", () =>
      fetchPack(live, (p) => onPhase({ phase: "download", fraction: p.fraction, detail: p.currentFile }), {
        also,
        skip,
      }),
    );
    pinned = null; // stale: a fresh pin is written below, once the open succeeds
  }

  // After the manifest, not before: the role is decided against the manifest this
  // boot loaded from, and the e2e suite reads this line to prove stage 1 opened the
  // right Bible rather than translating the chrome over the KJV.
  trace.push([`corpus loaded (${wantCorpus})`, 1]);

  onPhase({ phase: "prepare" });
  const stockPaths = new Set(manifest.files.filter((f) => f.seedOnce).map((f) => f.path));
  const home = await timed("virtual home build", () =>
    buildHome(pack, stockPaths),
  );
  // Collected here, not at the top: near-zero means the overlap paid for itself, a
  // large number means the engine download is the critical path (a different fix).
  const handedOff = await timed("engine bytes wait (overlapped)", () => engineBytes);
  if (!handedOff) trace.push(["engine bytes not stored — instantiate refetches", 1]);
  const wasm = await timed("wasm compile+instantiate", () => instantiate(home.root));

  // Before the open: the engine picks which corpus to open, so a language set after
  // this point picks nothing. `configLoad` is engine-independent, so it can answer
  // before there is an engine.
  const cfg = configLoad(wasm) ?? {};
  // A shared link's `?lang=` is a CHOICE made for this reader, so it is handed to
  // the engine the way the reader's own setting is — but only when they have not
  // made one themselves. Someone who has already chosen a language keeps it: a
  // link is allowed to introduce the app in a language, not to re-language an
  // app someone is already reading. (The church parameter's rule, for the same
  // reason.) The engine resolves and validates; an unshipped code falls back
  // exactly as an unshipped setting would.
  const chosen = typeof cfg.language === "string" ? cfg.language : "";
  i18nSetLanguage(wasm, chosen || sharedLang, locale);

  onPhase({ phase: "open" });
  // Yield so the "opening" progress message lands before the synchronous parse.
  await new Promise((r) =>
    typeof requestAnimationFrame !== "undefined" ? requestAnimationFrame(() => r(null)) : setTimeout(r, 0),
  );
  // The label says whether the persisted cache was there to skip the 19 MB re-parse.
  const engine = await timed(
    home.hadIdxcache ? "engine open (idxcache fast path)" : "engine open (cold corpus parse)",
    () => StudyEngine.open(wasm, "/home"),
  );

  // Here rather than when the warm starts, because warming begins only after stage 2
  // is parsed (~550 ms after text on a phone) and the reader taps inside that window.
  // The engine must never build an index inside a tap.
  engine.deferBuilds(true);

  // Only after a successful open: the next launch acts on the pin without asking the
  // network, so it must never name a pack that could not boot. Cold path only.
  if (!fromPin) await writePin(manifest, base, devicePackFiles(manifest, mine));

  // `load_cache` moves the bytes into the engine's own buffer, so the home's copy of
  // the cache it opened is a pure duplicate (~37 MB) and the rest were never read.
  const freed = home.evict(manifest.files.filter((f) => isCorpusRole(f.role)).map((f) => f.path));
  if (freed) trace.push(["home evict after open (KB)", Math.round(freed / 1024)]);

  // Only after an open that proved the depot can supply the text — earlier would take
  // away the one copy a device with an evicted depot still had.
  void dropLegacyIdxcache().then((n) => {
    if (n) trace.push(["legacy IDB idxcache dropped (KB)", Math.round(n / 1024)]);
  });

  return {
    engine,
    wasm,
    home,
    manifest,
    packVersion: manifest.version,
    trace,
    fromPin,
    corpusRole: wantCorpus,
    staleManifest: fromPin && pinIsFromAnOlderBuild(pinned),
  };
}
