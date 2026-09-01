// The engine worker: the whole engine life runs here — pack fetch, IndexedDB home,
// wasm instantiate, engine open, warm, the deferred R&D load, authoring writes and
// their persistence. The main thread never blocks on an engine call; it speaks the
// RPC below and paints.
//
// THE SCHEDULING RULE, which the rest of this file is arranged around: this thread
// is the only one that can answer layout and tap RPCs, so anything synchronous it
// runs blocks all of them — and blocks its own in-flight downloads with them, since
// a response body is read one `await` at a time. Background loading must therefore
// never hold the thread for one long synchronous block: stage-2 load, index warming
// and the R&D load all run as separate macrotasks with yields between them, so
// pending RPC messages interleave. Warming goes index-by-index through the wasm-only
// `warmStep` export instead of one monolithic `warmIndexes` call.
//
// Protocol (structured-clone JSON). In: `{ id, op, ...args }`, where the ops are the
// switch at the bottom of this file. Out: `{ id, result }` | `{ id, error }`, plus
// unsolicited `{ type }` events — progress (during boot), authored, readingWrote,
// persistFailed / persistOk, coreReady, warmReady, rndProgress / rndPreparing /
// rndReady, langPackProgress / paneLangProgress.
//
// Layout measure runs here over an OffscreenCanvas (measure.ts adapts), so the
// scripture face must be in self.fonts before the first LAYOUT — which is not the
// same as before the boot: the font load runs alongside the whole boot and is
// collected just before the boot reply, since no layout op can be answered until the
// shell has that reply.

import { boot, engineUrl, type BootResult } from "./boot";
import { pinnedUrls, writePin } from "./pin";
import {
  assetUrl,
  devicePackFiles,
  hasOptional,
  fetchManifest,
  fetchPackEntries,
  fetchRndPack,
  fetchStage2Pack,
  fetchSuggestedWeaves,
  fetchLangCorpus,
  fetchLangLexicon,
  isCorpusRole,
  otherCorpora,
  langCorpusEntry,
  packFileUrl,
  setAssetBase,
  suggestedWeavesEntry,
  takePackTrace,
  verifyStored,
} from "./pack";
import { depotBytes, depotDelete, depotHas, depotKeys } from "./depot";
import { PERF } from "./perf";
import { measureFor, readerFont, fontExtent, readerFontToken, setReaderFont } from "../reader/measure";
import {
  DEFAULT_FONT,
  FONT_CSS_FAMILY,
  FONT_FILES,
  SCRIPT_FALLBACK_BY_TOKEN,
} from "./fonts.generated";
import {
  aboutBlocks,
  configLoad,
  configSave,
  engineVersion,
  guideBlocks,
  routeLink,
  i18nCatalog,
  i18nSetLanguage,
  themePalette,
  shareLink,
  readingSpec,
  sessionSlot,
  sessionSlotAt,
  StudyEngine,
  type LayoutCfg,
} from "./StudyEngine";

let booted: BootResult | null = null;
/** Cost split of the most recent chapter layout — Settings → diagnostics. */
let lastTurn: [string, number][] = [];

// ── the stall meter ───────────────────────────────────────────────────────────
// How long this thread spent unavailable, measured directly: a heartbeat that should
// fire every tick, and how late it actually was. Dividing bytes by a wall clock that
// includes the starvation would only invent a "connection speed" that measures the
// starvation rather than the network.
//
// A late timer is not always a busy thread: a hidden page has its timers AND its
// in-flight requests frozen, so hidden time is excluded rather than counted (the main
// thread forwards visibility) and reported on its own.
const STALL_TICK = 50;
/** Lateness under this is timer jitter, not a stall worth counting. */
const STALL_FLOOR = 20;
const stall = { totalMs: 0, worstMs: 0, count: 0, hiddenMs: 0 };
let stallLast = 0;
let pageHidden = false;
let hiddenSince = 0;

/** The page's visibility, forwarded from the main thread (a worker has no
 *  `document` and cannot see this for itself). */
function setPageHidden(hidden: boolean): void {
  if (hidden === pageHidden) return;
  const now = performance.now();
  pageHidden = hidden;
  if (hidden) {
    hiddenSince = now;
    return;
  }
  stall.hiddenMs += now - hiddenSince;
  // Do not bill the gap we were away for as lateness.
  stallLast = now;
}

function startStallMeter(): void {
  stallLast = performance.now();
  setInterval(() => {
    const now = performance.now();
    // Browsers coalesce a repeating timer whose thread was blocked into one late
    // callback, so this is the lateness of this tick — the quantity wanted.
    const late = now - stallLast - STALL_TICK;
    stallLast = now;
    // Hidden: the clock ran but nothing was asked of this thread, and the timer was
    // throttled. Not a stall.
    if (pageHidden || late <= STALL_FLOOR) return;
    stall.totalMs += late;
    stall.worstMs = Math.max(stall.worstMs, late);
    stall.count++;
  }, STALL_TICK);
}

// ── the turn cache ────────────────────────────────────────────────────────────
// Laid-out chapters, LRU, keyed by everything the layout depends on. Overlays, notes
// and the verse band are painted OVER the display list by the shell, so authoring
// never invalidates this — only a width/font/spacing change does, and that re-keys.
//
// 16: three panes each prefetching both neighbours (ReaderPane.prefetchNeighbours) is
// 9 live keys, and at 8 the last prefetch evicted the first pane's own chapter. The
// remaining 7 are one stale generation's grace for the event that re-keys every pane
// at once (a width, font or spacing change) — what is on screen has to outlive what
// it replaced. Roughly 3 MB at the mean chapter, ~8 MB at the p99.
const TURN_CACHE_MAX = 16;
type LaidOut = { items: unknown[]; height: number };
const turnCache = new Map<string, LaidOut>();
/** Whether the AKJV overlay is on. Tracked here because it changes the words a
 *  chapter lays out to, so it belongs in the turn-cache key: without it, flipping the
 *  toggle serves the cached KJV display list back. In the key rather than clearing
 *  the cache, so toggling back and forth stays free. */
let akjvOn = false;

/**
 * The engines for other languages' texts, by language code — a pane reading German
 * beside an English one. They share the reader's data (every text sits at the KJV's
 * verse addresses), so this is a second view of one library, not a second study store
 * to keep in sync: authoring goes through the primary engine and `refreshAlts`
 * re-reads the others.
 */
const altEngines = new Map<string, StudyEngine>();

/** The engine a request means: the primary unless it named another language. */
function engineFor(lang?: string | null): StudyEngine {
  if (!lang) return booted!.engine;
  const alt = altEngines.get(lang);
  // Never a silent fall back to the primary: a pane labelled Deutsch painting the KJV
  // is the failure this path exists to avoid, so an unopened language is an error the
  // shell can act on — it is the shell that offers the download.
  if (!alt) throw new Error(`the ${lang} text is not open on this device`);
  return alt;
}

/** After an authoring write: the alt engines re-read the study files the primary
 *  just rewrote, or a tag made in English never reaches the German pane. */
function refreshAlts(): void {
  for (const e of altEngines.values()) e.loadCoreData();
}

interface LayoutReq {
  book: string;
  chapter: number;
  font: number;
  width: number;
  lineSpacing: number;
  versePerLine: boolean;
  verseNumbers: boolean;
  /** The pane's text language; absent = the reader's own. */
  lang?: string | null;
}

function layoutChapter(m: LayoutReq): LaidOut | null {
  // Everything that changes the geometry or the words is in the key: the face token
  // (metrics and optical scale both), `verseNumbers` (it moves every word on every
  // line), and the language (a German pane at the same width would otherwise serve the
  // English pane's list — right geometry, wrong Bible). The ITALICS switch is
  // deliberately absent: it changes paint only, never measurement.
  const key =
    `${m.book} ${m.chapter}|${m.lang ?? ""}|${readerFontToken()}|${m.font}|${m.width}|` +
    `${m.lineSpacing}|${m.versePerLine}|${m.verseNumbers}|${akjvOn}`;
  const hit = turnCache.get(key);
  if (hit) {
    turnCache.delete(key); // re-insert to keep LRU order
    turnCache.set(key, hit);
    firstLayoutServed?.();
    firstLayoutServed = null;
    return hit;
  }

  const t0 = performance.now();
  const font = readerFont(m.font);
  const measure = measureFor(font);
  booted!.wasm.setMeasure(measure);
  const lineHeight = fontExtent(m.font) * m.lineSpacing;
  const cfg: LayoutCfg = {
    width: m.width,
    lineHeight,
    spaceWidth: measure(" "),
    verseNumGap: measure(" ") * 1.4,
    paraIndent: lineHeight * 0.9,
    paraSpacing: lineHeight * 0.45,
    versePerLine: m.versePerLine,
    verseNumbers: m.verseNumbers,
  };
  const t1 = performance.now();
  const crossings0 = PERF ? booted!.wasm.measureCalls() : 0;
  const dl = engineFor(m.lang).layoutChapter(m.book, m.chapter, cfg);
  firstLayoutServed?.();
  firstLayoutServed = null;
  const t2 = performance.now();
  if (!dl) return null;
  // Parses the display-list JSON the core produced — the second of the two
  // serialisation passes a chapter turn pays for.
  const raw = dl.raw as LaidOut;
  dl.free();
  const t3 = performance.now();
  if (PERF)
    lastTurn = [
      ["measure setup", Math.round(t1 - t0)],
      ["core layout + text measurement", Math.round(t2 - t1)],
      ["display-list JSON → objects", Math.round(t3 - t2)],
      ["items", raw.items.length],
      ["wasm→JS measure crossings", booted!.wasm.measureCalls() - crossings0],
    ];

  turnCache.set(key, raw);
  if (turnCache.size > TURN_CACHE_MAX) turnCache.delete(turnCache.keys().next().value!);
  return raw;
}

// ── background-load scheduling ────────────────────────────────────────────────

/** Let queued messages (layout, taps) run before the next synchronous chunk. */
const yieldTask = () => new Promise<void>((r) => setTimeout(r, 0));

// ── engine calls ──────────────────────────────────────────────────────────────
// Every engine request arrives as `call` or `static`, and each is timed: an un-timed
// one can hold this thread for as long as it likes and leave no mark anywhere.
// `wordStudyBlocks` is the real case — it builds the occurrence index, the lens, the
// cross-references, the concept model and the bridge synchronously when the reader
// taps a word before the warm has reached them, seconds of it on a phone. Two clock
// reads per call, and only the expensive ones are kept.
const SLOW_CALL_MS = 30;
const SLOW_CALLS_KEPT = 25;
/** The most expensive engine calls this session, worst first. */
let slowCalls: [string, number][] = [];

function timedCall<T>(name: string, f: () => T): T {
  if (!PERF) return f();
  const t0 = performance.now();
  try {
    return f();
  } finally {
    const ms = Math.round(performance.now() - t0);
    if (ms >= SLOW_CALL_MS) {
      slowCalls.push([name, ms]);
      slowCalls.sort((a, b) => b[1] - a[1]);
      if (slowCalls.length > SLOW_CALLS_KEPT) slowCalls.length = SLOW_CALLS_KEPT;
    }
  }
}

/** Run one synchronous chunk, timing it into the boot trace. */
function timedChunk<T>(label: string, f: () => T): T {
  const t0 = performance.now();
  const v = f();
  booted!.trace.push([label, Math.round(performance.now() - t0)]);
  return v;
}

/** Resolved once the first layout has been served: the reader is on screen and the
 *  worker may start spending time on background loads. The race in backgroundLoad()
 *  caps the wait, so a boot that never lays out still gets its study data. */
let firstLayoutServed: (() => void) | null = null;
const firstLayout = new Promise<void>((r) => (firstLayoutServed = r));

/** Warm the lazy indexes one per macrotask (idempotent; safe to re-run after
 *  the R&D pack lands — the machine-tier indexes only build once it is in). */
let warmRun: Promise<void> | null = null;
function warmChunked(): Promise<void> {
  // Single-flighted, like the R&D load below: the background load and the R&D load
  // (which FirstRun.svelte kicks off via ensureRnd) really do run at once on a normal
  // first visit. Not a throughput fix — the engine ignores the loop's step counter and
  // every call advances one shared phase counter, so two loops split the work rather
  // than duplicating it. The steps stay small on purpose, one slice per macrotask,
  // which is what keeps layout and tap RPCs answerable.
  //
  // What it buys is a well-defined re-warm after the R&D pack lands: a second caller
  // joins the live pass, and clearing `warmRun` on completion means a genuinely later
  // call still gets a fresh one. Every phase is idempotent and the counter is shared,
  // so joining mid-pass is safe.
  return (warmRun ??= (async () => {
    for (let step = 0; ; step++) {
      await yieldTask();
      const more = timedChunk(`warm step ${step}`, () => booted!.engine.warmStep(step));
      if (!more) break;
    }
    // While this warm runs the engine refuses to build an index inside a tap (see
    // `defer_builds` in crates/ffi), so a study opened mid-warm comes back with only
    // the ready sections. Tell the shell the rest exist now, or it shows that thinner
    // answer until something unrelated re-fetches.
    self.postMessage({ type: "warmReady" });
  })().finally(() => {
    warmRun = null;
  }));
}

/** Fetch + load the machine-tier pack, chunked, with progress events for the
 *  shell's "load analysis" affordance. Shared by the auto path and the
 *  loadRnd op; concurrent callers reuse the in-flight run. */
let rndRun: Promise<void> | null = null;
function loadRndChunked(): Promise<void> {
  return (rndRun ??= (async () => {
    if (!booted!.manifest.files.some((f) => f.stage === "analysis")) return;
    const t0 = performance.now();
    const files = await fetchRndPack(booted!.manifest, (p) =>
      self.postMessage({ type: "rndProgress", fraction: p.fraction }),
    );
    booted!.trace.push(["rnd fetch+gunzip", Math.round(performance.now() - t0)]);
    await yieldTask();
    // Now the expensive part: parsing the morphology is megabytes of text, seconds of
    // it on a phone. One artifact per macrotask so a tap in between is still answered,
    // and the shell is told we have moved from downloading to preparing.
    self.postMessage({ type: "rndPreparing" });
    booted!.home.addFiles(files);
    for (let step = 0; ; step++) {
      await yieldTask();
      const more = timedChunk(`rnd load step ${step}`, () => booted!.engine.loadRndStep(step));
      if (!more) break;
    }
    // Both are parsed into wasm memory by the steps above and never re-read.
    const freedRnd = booted!.home.evict(["data/morphology.morphb", "data/text-witness.json"]);
    if (freedRnd) booted!.trace.push(["home evict after analysis (KB)", Math.round(freedRnd / 1024)]);
    await warmChunked();
    self.postMessage({ type: "rndReady" });
  })().catch((e) => {
    rndRun = null; // offline — the Settings toggle or next boot retries
    throw e;
  }));
}

/** Data Saver — the one setting that still means "ask me before you spend". */
const saveData = (): boolean => (navigator as any).connection?.saveData === true;

/** Is the machine-tier pack already on this device? Then loading it costs no network,
 *  and the deferral — which protects the reader's data and their first paint — has
 *  nothing left to protect. */
async function rndAlreadyCached(): Promise<boolean> {
  const files = booted!.manifest.files.filter((f) => f.stage === "analysis");
  if (!files.length) return false;
  for (const f of files) {
    if (!(await depotHas(packFileUrl(f, booted!.manifest.version)))) return false;
  }
  return true;
}

/** Whether this session will fetch the machine tier by itself. Phones defer it out of
 *  the boot path, never out of the session. */
async function willAutoLoadRnd(machineOn: boolean, deferRnd: boolean): Promise<boolean> {
  if (!machineOn) return false;
  if (!deferRnd) return true;
  return (await rndAlreadyCached()) || !saveData();
}

/** Notice a data update, download only what changed, and re-pin — all of it off the
 *  boot path, so a warm launch asks the network nothing. The live manifest is fetched
 *  once here, after the reader has text on screen, and diffed against the pin.
 *
 *  URLs are content-addressed on each file's own hash, so a release that changes one
 *  weave downloads one weave and unchanged files keep their URLs. New bytes land
 *  beside the old ones, and the pin is rewritten only once every file it will name is
 *  verified present, so an interrupted update leaves the previous generation bootable.
 *
 *  The new pack applies at the NEXT launch: this session's engine has its text in wasm
 *  memory and the reader is mid-verse. Files a release ADDS are the exception and
 *  belong to `backgroundLoad`, which hands them to the running home directly.
 */
async function reconcilePack(): Promise<void> {
  const live = await fetchManifest();
  if (live.version === booted!.packVersion) return; // nothing deployed since
  const t0 = performance.now();
  let fetched = 0;
  // What this device's pack is — the optional bundle only where the reader installed
  // it. The same call decides what gets pinned below, so the sweep and the pin cannot
  // disagree about which files should be here.
  const mine = devicePackFiles(live, (f) => hasOptional(booted!.home, f));
  // Not the other Bibles, and not the analysis pack. They are in `mine` — the pin
  // claims their URLs and prune keeps them — but they must not gate this sweep:
  // ~9 MB of corpora and ~3 MB of analysis against the base pack's ~200 KB of
  // typical drift, and each has its own download path running alongside this one
  // (`fetchOtherCorpora`, `loadRndChunked`). Gating on them strands a slow or
  // offline device's pin on the old release until every last byte has landed.
  //
  // `analysis` joined `corpus` here when the tiers began defaulting ON: that made
  // the analysis pack a background download on every device, which is exactly
  // what the corpus carve-out is about. Before that it was fetched only where a
  // reader had opted in, and a device that had it had it before the sweep ran.
  // Gating on it also raced its own downloader — the sweep fetching a file
  // `loadRndChunked` was already fetching failed verification and abandoned the
  // whole re-pin, which is how a deploy left the reader stranded.
  const gating = mine.filter((f) => f.stage !== "corpus" && f.stage !== "analysis");
  for (const f of gating) {
    const url = packFileUrl(f, live.version);
    if (await depotHas(url)) continue; // unchanged: same hash, same URL, already here
    await depotBytes(url);
    // A hash mismatch means a truncated body or an error page served 200; stored
    // unchecked, the engine fails to parse it on every launch with no way out.
    if (!(await verifyStored(f, live.version))) return;
    fetched++;
  }
  // Verify before committing. Cheap — a metadata lookup per file, no bodies read.
  for (const f of gating) {
    if (!(await depotHas(packFileUrl(f, live.version)))) return; // incomplete: keep the old pin
  }
  // `mine`, not `gating`: the pin claims the corpora. URLs are content-addressed and
  // deterministic, so a claim for bytes still on their way names exactly where they
  // will land, prune's keep-set covers them mid-download, and nothing re-pins on
  // arrival.
  await writePin(live, assetUrl(""), mine);

  // From here the session speaks about the pack it actually has.
  booted!.manifest = live;

  booted!.trace.push([`reconciled to ${live.version} (${fetched} files)`, Math.round(performance.now() - t0)]);
  // No message out: this function's product is the depot and the pin. What the running
  // session needed out of a new release was the stage-1 diff in `backgroundLoad`, and
  // it has already had it.
}

async function backgroundLoad(machineOn: boolean, deferRnd: boolean): Promise<void> {
  await Promise.race([firstLayout, new Promise((r) => setTimeout(r, 2500))]);
  try {
    // Which pack are we actually on? A warm boot's manifest IS the pin, which
    // describes the release this device last completed — so stage 2 would fetch the
    // study files THAT release listed and a file added since would never arrive.
    //
    // Gated on the pin coming from an older build, because a warm boot on an unchanged
    // release must ask the network for nothing at all, not even 5 KB. One extra fetch
    // on the first launch after an upgrade, none ever again, and `fetchManifest` falls
    // back to the stored copy so an offline upgrade behaves as before.
    if (booted!.staleManifest) {
      // What the pin delivered, held before the live manifest replaces it: the
      // left-hand side of the diff below.
      const pinned = booted!.manifest;
      try {
        booted!.manifest = await fetchManifest();
        booted!.trace.push(["manifest refreshed (newer build than pin)", 0]);
      } catch {
        /* offline: the pin's manifest is what we have, and it is enough */
      }

      // Files a newer release added at stage 1. The stage-2 fetch below picks up
      // study-stage additions, but selects on `stage === "study"` — so without this a
      // text-stage addition reaches fresh profiles and nobody else, and the feature
      // reading it silently loads empty on every upgraded install.
      //
      // Additions by path only, never replacements: swapping bytes under a running
      // engine is what `reconcilePack`'s next-launch rule exists to avoid. `seedOnce`
      // entries are excluded because the reader's own copies of the stock study set
      // rule; corpus caches because only the one this boot chose is ever inflated.
      const have = new Set(pinned.files.map((f) => f.path));
      const adds = booted!.manifest.files.filter(
        (f) => f.stage === "text" && !f.seedOnce && !isCorpusRole(f.role) && !have.has(f.path),
      );
      if (adds.length) {
        try {
          const tAdd = performance.now();
          const late = await fetchPackEntries(booted!.manifest.version, adds);
          booted!.trace.push([
            `stage1 additions fetch+gunzip (${adds.length})`,
            Math.round(performance.now() - tAdd),
          ]);
          await yieldTask();
          timedChunk("stage1 additions load", () => booted!.home.addFiles(late));
        } catch {
          /* Offline part-way through: nothing was pinned, so `staleManifest` is
             still true on the next warm boot and this same diff runs again. */
        }
      }
    }

    const t0 = performance.now();
    const files = await fetchStage2Pack(booted!.manifest, undefined, booted!.home.langsInstalled);
    booted!.trace.push(["stage2 fetch+gunzip", Math.round(performance.now() - t0)]);
    await yieldTask();
    timedChunk("stage2 load (Strong's + notes)", () => {
      booted!.home.addFiles(files);
      booted!.engine.loadCoreData();
    });
    // Read exactly once, by loadCoreData, with the parsed forms living in the engine
    // from here on — so the home's copies are duplication. NOT the margin notes, which
    // load_study re-reads on every authoring write, and NOT cross-references.tsv, whose
    // lazy index can still be built on a later tap. The dictionary paths come off the
    // manifest rather than a hand-kept list, so a language added to the registry is
    // evicted by having been added.
    const freedCore = booted!.home.evict([
      "data/strongs.json",
      "data/akjv.akjvb",
      ...booted!.manifest.files.filter((f) => f.role?.startsWith("lexicon:")).map((f) => f.path),
    ]);
    if (freedCore) booted!.trace.push(["home evict after stage 2 (KB)", Math.round(freedCore / 1024)]);
    self.postMessage({ type: "coreReady" });
    await warmChunked();
    // Before the analysis pack, not after: reconciling is normally one 5 KB manifest
    // fetch and some hash comparisons, and queued behind a megabyte of optional
    // analytics a device sits on a stale pin for the length of that download. In its
    // own try, because being offline here is the normal case, not an error.
    try {
      await reconcilePack();
    } catch {
      /* offline or a stalled manifest — the pin stands, the next launch retries */
    }
    // The other Bibles, last: the reader has had text for a while by now, Strong's is
    // parsed, the chapter is warm.
    await fetchOtherCorpora();
    if (await willAutoLoadRnd(machineOn, deferRnd)) await loadRndChunked();
  } catch {
    /* offline — the Settings toggle or next boot retries */
  }
}

/** Every language's Bible except the one open, into the depot, so that picking a
 *  language is a switch and not a download errand.
 *
 *  Into the depot, never the home: these are compressed bytes, and only the corpus
 *  this boot opened is ever inflated (`isOtherCorpus`) — all of them in the home would
 *  be ~91 MB. That is why this is its own stage rather than `study`, which
 *  `fetchStage2Pack` loads wholesale.
 *
 *  One file at a time with a yield between, per the scheduling rule at the top of this
 *  file. Nothing the reader is waiting for is behind it.
 *
 *  Deliberately unverified, unlike `reconcilePack`'s downloads: nothing reads these
 *  bytes until a language switch, and that path opens the engine on them and falls
 *  back to the KJV if they will not parse.
 *
 *  Failure is silent and per-file: a device that gets two of three Bibles has two of
 *  three, and the next launch picks up the third because `depotHas` skips what is
 *  here. */
async function fetchOtherCorpora(): Promise<void> {
  const rest = otherCorpora(booted!.manifest, booted!.corpusRole);
  if (!rest.length) return;
  const t0 = performance.now();
  let got = 0;
  for (const f of rest) {
    const url = packFileUrl(f, booted!.manifest.version);
    try {
      if (!(await depotHas(url))) {
        await depotBytes(url);
        got++;
      }
    } catch {
      /* offline, or this one file 404s in a partial deploy: the rest still try */
    }
    await yieldTask();
  }
  if (!got) return;
  booted!.trace.push([`other Bibles fetched (${got}/${rest.length})`, Math.round(performance.now() - t0)]);
  // No re-pin, and its absence is load-bearing. Every pin writer — the boot's cold
  // path, `reconcilePack`, `installLangPack` — claims the corpora's URLs up front,
  // before the bytes exist, and prune's keep-set covers them from the first pin on. A
  // re-pin here would add nothing and could write `booted!.manifest` back over a newer
  // pin `reconcilePack` had just written.
}

/** Reclaim everything the device no longer needs.
 *
 *  An allowlist, not a denylist: keep what the pin (and the generation before it)
 *  names plus the shell this build is made of, and delete the rest. A denylist keyed
 *  on `?v=` cannot see per-file hashes, and cannot reclaim a file dropped from the
 *  pack entirely.
 *
 *  Hard precondition: prune runs only with a readable pin AND a non-empty shell list.
 *  Without both the keep-set is incomplete and an allowlist would delete the app.
 *  Skipping costs disk; getting it wrong costs the reader their offline copy.
 *
 *  Two generations are kept, and prune runs at the START of a session. That buys one
 *  generation of grace with no cross-tab coordination and no lock: a tab still reading
 *  the previous pack keeps working, and the worst case is one superseded pack
 *  lingering until the next launch. */
async function pruneToPin(shell: string[]): Promise<number> {
  const base = assetUrl("");
  const keep = await pinnedUrls(base);
  if (keep.size <= 2 || !shell.length) return 0; // no pin, or no shell list: refuse
  for (const f of shell) keep.add(assetUrl(f));
  keep.add(base);
  keep.add(assetUrl("index.html"));
  keep.add(assetUrl("shell-manifest.json"));
  keep.add(assetUrl("pack/manifest.json"));
  // The engine binary is versioned by build id rather than listed in the shell
  // manifest. From boot.ts, which is also what prefetches it: an allowlist that spells
  // the URL out for itself deletes the engine the day the spelling changes.
  keep.add(engineUrl());

  let gone = 0;
  for (const url of await depotKeys()) {
    if (keep.has(url)) continue;
    if (await depotDelete(url)) gone++;
  }
  return gone;
}

// ── persisting the reader's own work ─────────────────────────────────────────
// A plain debounced `void persistUserData()` loses writes two ways:
//
//  * A failed write rejects a promise nobody is holding, while the shell has already
//    told the reader their note was saved — saved only in the in-memory home, which
//    dies with the tab. So failures come back out to the shell, and are retried.
//  * A tab that goes away inside the debounce loses the note entirely: a hidden page
//    has its timers frozen, so the pending callback may never run. `flush` awaits the
//    write instead of scheduling it, and the main thread calls it on pagehide /
//    visibilitychange-hidden.
//
// Everything funnels through `persistNow`, so "was this reported?" has one answer.
// The flush runs the same per-file diff (see home.ts), one moment earlier.
const PERSIST_DEBOUNCE = 50;
/** Backoff between retries, ms. It ends: a device that has refused five times is out
 *  of room rather than busy, and a timer that never stops costs battery. The shell's
 *  notice carries a "Try again" for once the reader has freed some space. */
const PERSIST_BACKOFF = [250, 1_000, 4_000, 15_000];

let persistTimer: ReturnType<typeof setTimeout> | null = null;
let persistRetry: ReturnType<typeof setTimeout> | null = null;
let persistTries = 0;
/** Whether the shell is showing a failure notice — a healthy session posts nothing,
 *  a recovered one posts exactly once. */
let persistFailing = false;

/** What went wrong, in the browser's own words. An aborted IndexedDB transaction can
 *  reject with a null error, which no bug report can act on. */
function persistReason(e: unknown): string {
  if (e instanceof Error) return `${e.name}: ${e.message}`;
  return e == null ? "the browser gave no reason" : String(e);
}

/** Persist the authored subtree now. Resolves when this attempt has settled, which is
 *  what makes it awaitable from `flush`. Never rejects: a failed save is news for the
 *  reader, not an error for whoever triggered it. */
async function persistNow(): Promise<void> {
  // Both timers are subsumed by running now; either left armed would double-write,
  // and the debounce's write would diff against a moving tree.
  if (persistTimer) clearTimeout(persistTimer);
  if (persistRetry) clearTimeout(persistRetry);
  persistTimer = null;
  persistRetry = null;
  try {
    await booted!.home.persistUserData();
    persistTries = 0;
    if (persistFailing) {
      persistFailing = false;
      self.postMessage({ type: "persistOk" });
    }
  } catch (e) {
    persistFailing = true;
    const more = persistTries < PERSIST_BACKOFF.length;
    // The browser's own words. The shell does not shout them at the reader, but a
    // failure nobody can name is a failure nobody can act on.
    self.postMessage({
      type: "persistFailed",
      detail: persistReason(e),
      retrying: more,
    });
    if (more && !persistRetry) {
      persistRetry = setTimeout(() => {
        persistRetry = null;
        void persistNow();
      }, PERSIST_BACKOFF[persistTries++]);
    }
  }
}

/** Coalesce a burst of authoring writes into one persist. */
function schedulePersist(): void {
  if (persistTimer) return;
  persistTimer = setTimeout(() => {
    persistTimer = null;
    void persistNow();
  }, PERSIST_DEBOUNCE);
}

// ── statics ───────────────────────────────────────────────────────────────────

/** Which family tokens this worker has already put in its FontFaceSet, so a
 *  reader flipping between two faces pays each download once. */
const fontsLoaded = new Set<string>();

/**
 * Load one family's faces into the worker's own FontFaceSet and point the measure
 * context at it. Workers do not share the document's fonts, and measurement must see
 * the real metrics of the face the main thread paints, or lines wrap where they are
 * not drawn.
 *
 * Returns how many faces the worker now has for that family — counted, not attempted,
 * because a silent failure is invisible until a reader notices odd wrapping.
 */
async function loadFonts(base: string, token: string): Promise<number> {
  const resolved = setReaderFont(token);
  const files = FONT_FILES[resolved];
  const want = files.italic ? 2 : 1;
  const scope = self as unknown as { fonts?: FontFaceSet };
  const fonts = scope.fonts;
  if (!fonts) return 0; // very old engines: fall back to platform metrics
  // The script fallbacks, always, whichever family was asked for. Each is in every
  // Latin family's CSS stack (fonts.generated), so the document paints Arabic,
  // Gurmukhi and Devanagari in them; without them here the worker would measure that
  // text in whatever system font its OffscreenCanvas found. Loaded before the family,
  // and outside the `fontsLoaded` short-circuit below, which is keyed per family.
  //
  // Unconditional even at 287 KB: `pane.textLanguage` lets a reader open the Van Dyck
  // or the Hindi Bible in a second pane, so the script this worker must MEASURE is not
  // a function of the language it paints the chrome in. Narrowing it safely means
  // loading per corpus at layout time, which is a protocol change.
  //
  // Awaited in parallel — three serial `await`s on a slow link are three round trips
  // before the first line can be measured.
  await Promise.all(
    Object.entries(SCRIPT_FALLBACK_BY_TOKEN)
      .filter(([token]) => !fontsLoaded.has(token))
      .map(async ([token, paths]) => {
        for (const path of paths) {
          try {
            const face = new FontFace(FONT_CSS_FAMILY[token], `url(${new URL(path, base).href})`, {
              style: "normal",
              // 400 alone: these are static regulars (or variable faces the CSS
              // declares at 400), and 400 700 would answer a bold request with one.
              weight: "400",
            });
            await face.load();
            fonts.add(face);
            fontsLoaded.add(token);
          } catch {
            /* platform metrics still beat a dead worker */
          }
        }
      }),
  );
  if (fontsLoaded.has(resolved)) return want;
  let loaded = 0;
  for (const [path, style] of [
    [files.normal, "normal"],
    [files.italic, "italic"],
  ] as const) {
    if (!path) continue; // a family with no italic — see core::font
    try {
      const face = new FontFace(FONT_CSS_FAMILY[resolved], `url(${new URL(path, base).href})`, {
        style,
        // 400 700, not the file's own axis range: Fira Code's `wght` defaults to 300,
        // so letting the default through measures and paints the Light as body text.
        weight: "400 700",
      });
      await face.load();
      fonts.add(face);
      loaded++;
    } catch {
      /* platform metrics still beat a dead worker */
    }
  }
  if (loaded === want) fontsLoaded.add(resolved);
  return loaded;
}

function statics(): Record<string, (...a: any[]) => any> {
  const w = booted!.wasm;
  return {
    routeLink: (uri: string) => routeLink(w, uri),
    configLoad: () => configLoad(w),
    configSave: (cfg: unknown) => configSave(w, cfg),
    themePalette: (theme: string) => themePalette(w, theme),
    i18nCatalog: (chosen: string, device: string) => i18nCatalog(w, chosen, device),
    i18nSetLanguage: (chosen: string, device: string) => i18nSetLanguage(w, chosen, device),
    guideBlocks: () => guideBlocks(w),
    aboutBlocks: () => aboutBlocks(w),
    engineVersion: () => engineVersion(w),
    // Engine-independent, so they are on the static path and the shell can ask for
    // them before (or without) an engine — what the church button and the dwell timer
    // both need.
    shareLink: (request: unknown) => shareLink(w, request as Parameters<typeof shareLink>[1]),
    readingSpec: () => readingSpec(w),
    sessionSlot: (date: string, hour: number) => sessionSlot(w, date, hour),
    sessionSlotAt: (date: string, minute: number, sundayService: number) =>
      sessionSlotAt(w, date, minute, sundayService),
  };
}

self.onmessage = async (ev: MessageEvent) => {
  const m = ev.data;
  const reply = (result: unknown) => self.postMessage({ id: m.id, result });
  const fail = (e: unknown) => self.postMessage({ id: m.id, error: e instanceof Error ? e.message : String(e) });
  try {
    switch (m.op) {
      case "boot": {
        setAssetBase(m.base);
        // Before anything else, so the meter covers the downloads too.
        if (PERF) startStallMeter();
        // The fonts overlap the whole boot: they are needed before the first LAYOUT,
        // not before the boot, and awaited here they would hold the pack download and
        // the splash's first progress message behind two font files.
        const t0 = performance.now();
        let fontsMs = 0;
        const fonts = loadFonts(m.base, m.textFont ?? DEFAULT_FONT).then((n) => {
          fontsMs = Math.round(performance.now() - t0);
          return n;
        });
        // Handled immediately so nothing becomes an unhandled rejection while it is
        // un-awaited; the await below still sees a failure. `loadFonts` swallows a
        // refused face itself, so this is only for a platform that throws elsewhere.
        void fonts.catch(() => {});
        booted = await boot(
          (p) => self.postMessage({ type: "progress", ...p }),
          m.locale ?? "",
          m.lang ?? "",
          m.sharedLang ?? "",
        );
        // Before the reply, and so before any layout op can be answered: measurement
        // must see the real metrics of the chosen face.
        const w0 = performance.now();
        const fontFaces = await fonts;
        booted.trace.unshift(
          ["worker fonts", fontsMs],
          ["worker font faces", fontFaces],
          ["worker fonts wait after boot", Math.round(performance.now() - w0)],
        );
        booted.engine.onAuthored = () => {
          schedulePersist();
          // The alt engines hold their own view of the study files this write just
          // changed; re-read before the shell re-fetches.
          refreshAlts();
          self.postMessage({ type: "authored" });
        };
        // Dwell reports persist only the reading dir, and announce themselves
        // separately from `authored`: the shell must drop its cached reading reads or
        // the navigator keeps painting an hours-old map, but it must not drop
        // everything else with them. No retry ladder — the next dwell tick rewrites the
        // same file — but the reader still hears about a device refusing writes,
        // because it is refusing their notes too.
        booted.engine.onReadingWrite = () => {
          void booted!.home.persistUserDir("reading").catch((e) => {
            persistFailing = true;
            self.postMessage({
              type: "persistFailed",
              detail: persistReason(e),
              retrying: false,
            });
          });
          self.postMessage({ type: "readingWrote" });
        };
        const cfg = configLoad(booted.wasm) ?? {};
        // Opt-in: absent means off, so a first visit does not pull the analysis pack
        // in the background. (The language was set inside `boot`, before the open.)
        const machineOn = cfg.machineAnalysis === true;
        // So the shell never offers "Load analysis" for a load already on its way.
        const rndAuto = await willAutoLoadRnd(machineOn, m.deferRnd === true);
        const x0 = performance.now();
        // Every concrete theme (core::theme::Theme) — the palettes the shell paints
        // from without a round trip. Keep in step with the Rust enum and
        // SettingsDialog's `themes` list.
        const THEME_TOKENS = [
          "light",
          "dark",
          "night",
          "solarized-light",
          "solarized-dark",
          "gruvbox",
          "nord",
          "one-dark",
          "sepia",
          "catppuccin-mocha",
          "catppuccin-latte",
          "tokyo-night",
          "rose-pine",
          "synthwave",
          "scriptorium",
          "blueprint",
          "phosphor",
          "high-contrast",
        ];
        const palettes: Record<string, unknown> = {};
        for (const tk of THEME_TOKENS) palettes[tk] = themePalette(booted.wasm, tk);
        const toc = booted.engine.toc();
        // Resolved against the reader's setting and the device's locale by the core
        // (i18n::resolve). Here rather than as its own call for the palettes' reason:
        // this thread answers one thing at a time and no screen paints without it.
        // `|| m.sharedLang` for the SAME reason `boot()` has it, and it has to be
        // the same expression: the engine's own language and the shell's catalogue
        // are two halves of one answer, and a boot that resolves them differently
        // paints English chrome over a German Bible (or the reverse). The core
        // resolves and validates both; this only has to ask the same question.
        const chosenLang = typeof cfg.language === "string" && cfg.language ? cfg.language : (m.sharedLang ?? "");
        const i18n = i18nCatalog(booted.wasm, chosenLang, m.locale ?? "");
        booted.trace.push(["boot reply extras (palettes + toc + i18n)", Math.round(performance.now() - x0)]);
        void backgroundLoad(machineOn, m.deferRnd === true);
        reply({
          packVersion: booted.packVersion,
          config: cfg,
          version: engineVersion(booted.wasm),
          bundledOn: booted.home.bundledOn,
          rndAuto,
          fontFaces,
          // Folded into the boot reply: this thread answers one thing at a time, so
          // the shell's first reads would otherwise be full queue hops between the
          // boot reply and the first layout request. Safe to hand over once because
          // both are session-immutable — the palettes are compiled-in tables
          // (crates/core/src/theme.rs) and the TOC is corpus-derived, which is why
          // session.svelte.ts pins the TOC in its read-through cache.
          palettes,
          toc,
          i18n,
        });
        break;
      }
      case "call": {
        const e = booted!.engine as unknown as Record<string, (...a: any[]) => unknown>;
        reply(timedCall(m.method, () => e[m.method](...m.args)));
        // The overlay changes the words, so the turn cache has to know.
        if (m.method === "setAkjvOverlay") akjvOn = m.args[0] === true;
        break;
      }
      // The same call against another language's text. The method name is unchanged:
      // every StudyEngine read works on any handle, so per-pane language costs no
      // per-feature RPC. Authoring never comes through here — two writers over one
      // home is a corruption the atomic store should not be asked to survive, and the
      // reader's data is shared anyway.
      case "callIn": {
        const e = engineFor(m.lang as string) as unknown as Record<string, (...a: any[]) => unknown>;
        reply(timedCall(`${m.method}@${m.lang}`, () => e[m.method](...m.args)));
        break;
      }
      case "static": {
        reply(timedCall(m.fn, () => statics()[m.fn](...m.args)));
        // Config writes land in the in-memory WASI home; mirror them to IndexedDB
        // like any authoring write. Not `schedulePersist()`: config is written at
        // exactly the moments the page is about to go away — answering first run then
        // reloading, choosing a theme then closing the tab — and a 50 ms debounce is
        // long enough to lose all of them. `persistNow()` still reports and retries.
        if (m.fn === "configSave") void persistNow();
        break;
      }
      case "layout": {
        reply(timedCall(`layout ${m.book} ${m.chapter}`, () => layoutChapter(m)));
        break;
      }
      case "prefetch": {
        // Warm the turn cache without shipping the display list back over
        // postMessage, so the next page turn is a cache hit.
        layoutChapter(m);
        reply(null);
        break;
      }
      case "layoutTrace": {
        reply(lastTurn);
        break;
      }
      case "setTextFont": {
        // Loaded before replying: the shell relayouts as soon as this resolves, and a
        // layout measured before the face arrived is measured in the fallback and
        // painted in the real one. Already-loaded families resolve immediately.
        reply(await loadFonts(m.base, m.token));
        break;
      }
      case "fontExtent": {
        reply(fontExtent(m.px));
        break;
      }
      case "measure": {
        // One-off text widths for callers needing engine-side metrics; space width
        // and friends travel inside the layout cfg instead.
        reply(measureFor(readerFont(m.px))(m.text));
        break;
      }
      case "loadRnd": {
        await loadRndChunked();
        reply(null);
        break;
      }
      case "prune": {
        reply(await pruneToPin(m.shell ?? []));
        break;
      }
      case "bootTrace": {
        reply(booted ? [...booted.trace] : []);
        break;
      }
      case "visibility": {
        // Fire-and-forget from the main thread: no id, no reply.
        setPageHidden(m.hidden === true);
        return;
      }
      case "diagnostics": {
        // One round trip, so a report is not assembled from readings taken at
        // different moments.
        reply({
          trace: booted ? [...booted.trace] : [],
          turn: [...lastTurn],
          stall: { ...stall },
          slowCalls: [...slowCalls],
          packFiles: takePackTrace(),
          packVersion: booted?.packVersion ?? null,
          fromPin: booted?.fromPin ?? null,
        });
        break;
      }
      case "export": {
        reply([...booted!.home.exportUserData()]);
        break;
      }
      case "flush": {
        // Persist now and answer only once the transaction has settled — a `void`
        // here would recreate the hole this op closes. Boot may not have finished: a
        // tab hidden during a cold boot has no home to write and nothing authored.
        if (!booted) {
          reply(null);
          break;
        }
        persistTries = 0; // a fresh ladder: someone asked on purpose
        await persistNow();
        reply(null);
        break;
      }
      case "freeze": {
        booted!.home.freeze();
        reply(null);
        break;
      }
      case "setBundled": {
        await booted!.home.setBundled(m.on);
        reply(null);
        break;
      }
      case "suggestedState": {
        // What the Settings row needs to draw itself. The size comes from the
        // manifest rather than a constant, so the row cannot quote a stale number.
        const entry = suggestedWeavesEntry(booted!.manifest);
        reply({
          available: entry !== null,
          installed: booted!.home.suggestedInstalled,
          gzBytes: entry?.gzBytes ?? 0,
        });
        break;
      }
      case "installSuggested": {
        // One file (~110 KB), so no progress ladder: a bar that jumps 0→100 says less
        // than the button staying busy.
        const bundle = await fetchSuggestedWeaves(booted!.manifest);
        if (!bundle) {
          fail("this build has no suggested-weave bundle");
          break;
        }
        const written = await booted!.home.installSuggestedWeaves(bundle);
        // Re-pin: the bundle is part of this device's pack now, and prune keeps only
        // what the pin names — without this the next sweep reclaims the download.
        await writePin(booted!.manifest, assetUrl(""), devicePackFiles(booted!.manifest, (f) => hasOptional(booted!.home, f)));
        // The engine holds the weave library it read at open, so the files are on
        // disk and invisible until it re-reads them. Same call stage 2 uses.
        booted!.engine.loadCoreData();
        self.postMessage({ type: "authored" });
        reply(written);
        break;
      }
      // ── a language's scripture ───────────────────────────────────────────
      // A language no pane reads any more is a Bible-sized allocation doing nothing.
      // The shell knows what every pane is reading, so it passes the keep list.
      case "releaseLangs": {
        const keep = new Set((m.keep as string[]) ?? []);
        let freed = 0;
        for (const [code, engine] of [...altEngines]) {
          if (keep.has(code)) continue;
          engine.free();
          altEngines.delete(code);
          freed++;
        }
        // The turn cache holds display lists laid out from those engines, and their
        // engines are gone.
        if (freed) turnCache.clear();
        reply(freed);
        break;
      }
      case "wasmMemoryBytes": {
        reply(booted!.wasm.exports.memory.buffer.byteLength);
        break;
      }
      case "langPackState": {
        // Through the stale-pin window. A warm boot's manifest IS the pin, and on the
        // first launch after an upgrade that pin describes the release before this
        // one — which may never have heard of the language the reader is picking.
        // Answering "not available" from it gives the reader a translated interface
        // over the English KJV. One 5 KB manifest fetch closes the window; offline
        // keeps the old answer, which is the truth about what this device can do
        // offline.
        let entry = langCorpusEntry(booted!.manifest, m.code as string);
        if (!entry) {
          try {
            booted!.manifest = await fetchManifest();
            entry = langCorpusEntry(booted!.manifest, m.code as string);
          } catch {
            /* offline: the pin's manifest is what this device has */
          }
        }
        // `installed` is a claim about the depot, not about the reader's history:
        // every Bible ships in the background, so a corpus can be in the depot with no
        // `langsInstalled` marker, or mid-flight with one. What the switch flow needs
        // is "can the reload that follows open this text", and only the depot answers
        // that. The marker still gates the dictionary, which really is opt-in, so a
        // language whose lexicon has not been taken still reports uninstalled.
        const bytesHere = !!entry && (await depotHas(packFileUrl(entry, booted!.manifest.version)));
        const lex = booted!.manifest.files.find((f) => f.role === `lexicon:${m.code as string}`);
        reply({
          available: !!entry,
          installed: bytesHere && (!lex || booted!.home.langsInstalled.has(m.code as string)),
          gzBytes: entry?.gzBytes ?? 0,
        });
        break;
      }
      // Open a language for a pane: download its text if this device has not got it,
      // then open a second engine on it. No reload — that is the whole difference from
      // the settings switch below, since the other pane must keep reading.
      case "openPaneLang": {
        const code = m.code as string;
        if (altEngines.has(code)) {
          reply({ ready: true });
          break;
        }
        const entry = langCorpusEntry(booted!.manifest, code);
        if (!entry) {
          fail(`this build has no ${code} corpus`);
          break;
        }
        /** Put this language's text and dictionary into the home. */
        const supply = async (): Promise<boolean> => {
          const cache = await fetchLangCorpus(booted!.manifest, code, (p) =>
            self.postMessage({ type: "paneLangProgress", code, fraction: p.fraction }),
          );
          if (!cache) return false;
          // That language's dictionary rides the same ask, or its word study serves
          // English definitions.
          await fetchLangLexicon(booted!.manifest, code);
          await booted!.home.installLangCorpus(code, entry.path, cache);
          return true;
        };

        if (!booted!.home.langsInstalled.has(code) && !(await supply())) {
          fail(`the ${code} text could not be downloaded`);
          break;
        }
        // Opening is synchronous, on the one thread that also answers layout and
        // taps; it reads the idxcache rather than parsing JSONL, so it is a load and a
        // directory, not a canon-wide decode.
        //
        // Retried once through the supply path: the eviction below frees this
        // language's bytes after the engine has read them, so a reader coming back to
        // it later in the session finds the file gone. The depot still has it, so the
        // cold path is the repair.
        let opened: StudyEngine;
        try {
          opened = timedCall(`open ${code} engine`, () => StudyEngine.openLang(booted!.wasm, "/home", code));
        } catch {
          if (!(await supply())) {
            fail(`the ${code} text could not be restored`);
            break;
          }
          try {
            opened = timedCall(`reopen ${code} engine`, () => StudyEngine.openLang(booted!.wasm, "/home", code));
          } catch (e) {
            fail(e);
            break;
          }
        }
        altEngines.set(code, opened);
        // Evict what the engine has already read, as the core boot does after stage
        // 2: the WASI shim's `File` copies its input, so until this runs the home
        // holds a second copy of everything just parsed — ~33 MB per language.
        const freed = booted!.home.evict(
          booted!.manifest.files
            .filter((f) => f.role === `corpus:${code}` || f.role === `lexicon:${code}`)
            .map((f) => f.path),
        );
        if (freed) booted!.trace.push([`home evict after opening ${code} (KB)`, Math.round(freed / 1024)]);
        reply({ ready: true });
        break;
      }
      case "installLangPack": {
        const code = m.code as string;
        let entry = langCorpusEntry(booted!.manifest, code);
        // The stale-pin window again — see `langPackState`. The state call has usually
        // refreshed the manifest already, but this op must hold on its own.
        if (!entry) {
          try {
            booted!.manifest = await fetchManifest();
            entry = langCorpusEntry(booted!.manifest, code);
          } catch {
            /* offline: fail below with the honest message */
          }
        }
        const cache = await fetchLangCorpus(booted!.manifest, code, (p) =>
          self.postMessage({ type: "langPackProgress", fraction: p.fraction }),
        );
        if (!cache || !entry) {
          fail(`this build has no ${code} corpus`);
          break;
        }
        // That language's lexicon rides the same ask, into the depot; the reload that
        // follows reads it back through stage 2. A pack without one is fine — study
        // serves the English dictionary until it ships.
        await fetchLangLexicon(booted!.manifest, code);
        await booted!.home.installLangCorpus(code, entry.path, cache);
        // Re-pin, for `installSuggested`'s reason: without this the next sweep
        // reclaims a 28 MB download.
        await writePin(booted!.manifest, assetUrl(""), devicePackFiles(booted!.manifest, (f) => hasOptional(booted!.home, f)));
        // Not `loadCoreData`: the corpus is chosen when the engine opens, so this
        // needs a reload rather than a re-read. The picker does that.
        reply(true);
        break;
      }
      default:
        fail(`unknown op ${m.op}`);
    }
  } catch (e) {
    fail(e);
  }
};
