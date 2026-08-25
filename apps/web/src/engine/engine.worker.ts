// The engine worker (TODO #28): the WHOLE engine life runs here — pack fetch,
// IndexedDB home, wasm instantiate, engine open, warm, the deferred R&D load,
// authoring writes and their persistence. The main thread never blocks on an
// engine call again; it speaks the tiny RPC below and paints.
//
// SCHEDULING RULE: this thread is the
// only one that can answer layout/tap RPCs, so background loading must never
// hold it for one long synchronous block. Stage-2 load, index warming, and
// the R&D load all run as separate macrotasks with yields between them —
// pending RPC messages interleave. Warming goes index-by-index through the
// wasm-only `warmStep` export instead of one monolithic `warmIndexes` call.
//
// Protocol (structured-clone JSON):
//   in:  { id, op: "boot", base, fontUrl, italicUrl, deferRnd } → boots; progress
//        streams, and the reply carries the session-immutables the shell needs
//        before first text (palettes, toc) so they are not four more queue hops
//   in:  { id, op: "call", method, args }           → StudyEngine[method](...args)
//   in:  { id, op: "static", fn, args }             → engine-independent fns
//   in:  { id, op: "layout", book, chapter, cfg, font } → display-list JSON
//   in:  { id, op: "loadRnd" }                      → deferred pack + re-warm
//   in:  { id, op: "bootTrace" }                    → [label, ms][] so far
//   in:  { id, op: "export" } / { id, op: "freeze" } / { id, op: "setBundled", on }
//   in:  { id, op: "suggestedState" }               → {available, installed, gzBytes}
//   in:  { id, op: "installSuggested" }             → optional weave set, count written
//   in:  { id, op: "flush" }                        → persist pending writes NOW
//   out: { id, result } | { id, error }
//   out: { type: "progress", phase, fraction?, detail? }   (during boot)
//   out: { type: "authored" }                       (any authoring write landed)
//   out: { type: "readingWrote" }                   (a dwell report landed)
//   out: { type: "persistFailed", detail, retrying } / { type: "persistOk" }
//   out: { type: "coreReady" } / { type: "rndReady" } / { type: "rndProgress", fraction }
//
// Layout measure runs HERE over an OffscreenCanvas (measure.ts adapts), so
// EB Garamond must be loaded into self.fonts before the first layout — the
// boot message carries the resolved font URL. Before the first LAYOUT, note,
// which is not the same as before the boot: the load runs alongside the whole
// boot and is collected just before the boot reply, since no layout op can be
// answered until the shell has that reply.

import { boot, engineUrl, type BootResult } from "./boot";
import { pinnedUrls, writePin } from "./pin";
import {
  assetUrl,
  devicePackFiles,
  hasOptional,
  fetchManifest,
  fetchRndPack,
  fetchStage2Pack,
  fetchSuggestedWeaves,
  fetchLangCorpus,
  fetchLangLexicon,
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
import { DEFAULT_FONT, FONT_CSS_FAMILY, FONT_FILES } from "./fonts.generated";
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
// This thread is the only one that can answer layout, taps and word studies, so
// anything synchronous it runs blocks all of them. It also blocks its OWN
// downloads: a response body is read one `await` at a time, and every one of
// those continuations needs the event loop, so a file can finish arriving at the
// radio and then sit in a queue behind a quarter-second of arithmetic.
//
// That second effect is INVISIBLE in a wall-clock timer: dividing bytes by a
// wall clock that includes the starvation produces a confident, invented
// "connection speed". The bytes-per-second of a starved reader measures the
// starvation, not the network.
//
// So measure the starvation directly. A heartbeat that should fire every tick,
// and how late it actually was. Total lateness during boot IS the time this
// thread spent unavailable — no division, no inference, no assumption about
// anyone's wifi.
//
// A LATE TIMER IS NOT ALWAYS A BUSY THREAD. A hidden page has its timers AND its
// in-flight requests frozen by the browser, so a meter that counted that time
// would bill the reader's screen turning off as engine work. A measurement that
// cannot distinguish "we were busy" from "the phone was asleep" is worse than
// none: it invents a crisis and points at the wrong code. So the page tells the
// worker when it is hidden, hidden time is excluded rather than counted, and it
// is reported separately — because "you were away for 25 s" is also a thing a
// reader of this report needs to know.
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
    // Browsers COALESCE a repeating timer whose thread was blocked — one late
    // callback, not a burst of missed ones — so this is the lateness of this
    // tick, which is exactly the quantity wanted.
    const late = now - stallLast - STALL_TICK;
    stallLast = now;
    // Hidden: the clock kept running but this thread was not being asked to do
    // anything, and the timer itself was throttled. Not a stall.
    if (pageHidden || late <= STALL_FLOOR) return;
    stall.totalMs += late;
    stall.worstMs = Math.max(stall.worstMs, late);
    stall.count++;
  }, STALL_TICK);
}

// ── the turn cache ────────────────────────────────────────────────────────────
// Laid-out chapters, keyed by everything the layout depends on. Overlays,
// notes and the verse band are painted OVER the display list by the shell, so
// authoring never invalidates this — only a width/font/spacing change does,
// and that changes the key. Small and LRU: a handful of chapters is enough to
// make paging back and forth free, without holding the canon in memory.
//
// 16, because the working set is larger than "a handful" suggests. Every pane
// prefetches BOTH its neighbours (ReaderPane.prefetchNeighbours) and three panes
// can be open, so a settled three-pane session is 9 live keys — at 8, the last
// prefetch evicted the first pane's own chapter and the prefetch became pure cost.
// The remaining 7 are a stale generation's grace for the one event that re-keys
// every pane at once (a width, font or spacing change): what is on screen has to
// outlive what it replaced, or the reader pays for it twice.
//
// A turn costs what its display list weighs. Measured under V8 with --expose-gc,
// heapUsed across 40 retained JSON.parse copies of one chapter's list: 322 B/item
// for Psalm 119 (2,643 items → 831 KB), 235 B/item for Gen 1, 343 B/item for John
// 3. The mean chapter is 691 items, so 16 turns is ~3 MB; even at the p99 chapter
// (1,701 items) the ceiling is ~8 MB, against the ~235 MB all 822,057 items of the
// canon would be.
const TURN_CACHE_MAX = 16;
type LaidOut = { items: unknown[]; height: number };
const turnCache = new Map<string, LaidOut>();
/** Whether the AKJV overlay is on. Tracked HERE because it changes the words a
 *  chapter lays out to, and the turn cache is keyed on everything that does —
 *  without it, flipping the toggle serves the cached KJV display list straight
 *  back and the page never changes.
 *  Keeping it in the key rather than clearing the cache means toggling back and
 *  forth stays free, which is exactly what someone comparing wordings does. */
let akjvOn = false;

/**
 * The engines for OTHER languages' texts, by language code — a pane reading
 * German beside an English one (docs/PER-PANE-LANGUAGE.md).
 *
 * They share the reader's data (every text sits at the KJV's verse addresses),
 * so this is a second view of one library rather than a second library. What it
 * is NOT is a second study store to keep in sync: an authoring write goes
 * through the primary engine, and `refreshAlts` re-reads the others.
 */
const altEngines = new Map<string, StudyEngine>();

/** The engine a request means: the primary unless it named another language. */
function engineFor(lang?: string | null): StudyEngine {
  if (!lang) return booted!.engine;
  const alt = altEngines.get(lang);
  // NOT a silent fall back to the primary. A pane labelled Deutsch painting the
  // KJV is the failure this whole path is built to avoid, so an unopened
  // language is an error the shell can act on — it is the shell that offers the
  // download.
  if (!alt) throw new Error(`the ${lang} text is not open on this device`);
  return alt;
}

/** After an authoring write: the alt engines re-read the study files the
 *  primary just rewrote, or a tag made in English never reaches the German
 *  pane's word study. */
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
  // The face token is part of the key: a cached layout was measured under one
  // face's metrics AND its optical scale (readerFont applies both), so a face
  // switch must miss here rather than serve geometry the new face will not
  // paint at. Same for `verseNumbers`: it moves every word on every line, so a
  // layout cached under one setting is wrong geometry under the other. (The
  // ITALICS switch is deliberately absent — it changes paint only, never
  // measurement, so its layouts are interchangeable.)
  // The LANGUAGE is part of the key for the same reason the face and the
  // overlay are: it changes the words this chapter lays out to. Without it a
  // German pane at the same width serves the English pane's cached display
  // list — the right geometry for the wrong Bible.
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
  // `raw` parses the display-list JSON the core produced — the second of the
  // two serialisation passes a chapter turn pays for.
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

// NOTHING THE ENGINE BUILDS IS PERSISTED. Prune is an allowlist, so any stale
// saved artifact still on a device is reclaimed on that device's next launch.

// ── engine calls ──────────────────────────────────────────────────────────────
// EVERY engine request the shell makes arrives as `call` or `static`, and each is
// timed. The boot stages, the warm chunks and the analysis chunks all report
// themselves, so a trace looks complete — while a single un-timed `call` can hold
// this thread for as long as it likes and leave no mark anywhere.
//
// It is not a hypothetical hole. `wordStudyBlocks` builds the occurrence index,
// the rendering lens, the cross-references, the concept model and the bridge
// SYNCHRONOUSLY when the reader taps a word before the warm has reached them —
// hundreds of ms on a desktop, many seconds on a phone, that no timed section
// would otherwise account for. A frozen thread also strands its own in-flight
// downloads.
//
// Cheap by construction: two clock reads per call, and only calls that actually
// cost something are kept.
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

/** Resolved once the first layout has been served — the reader is on screen
 *  and the worker may start spending time on background loads. The race in
 *  backgroundLoad() caps the wait so a boot that never lays out (unexpected,
 *  but possible) still gets its study data. */
let firstLayoutServed: (() => void) | null = null;
const firstLayout = new Promise<void>((r) => (firstLayoutServed = r));

/** Warm the lazy indexes one per macrotask (idempotent; safe to re-run after
 *  the R&D pack lands — the machine-tier indexes only build once it is in). */
let warmRun: Promise<void> | null = null;
function warmChunked(): Promise<void> {
  // SINGLE-FLIGHTED, like the R&D load below. Two callers reach here: the
  // background load, and the R&D load — which the first-run chooser kicks off via
  // ensureRnd (FirstRun.svelte) while the background load is often still in
  // flight, so both really do run at once on a normal first visit.
  //
  // NOT a throughput fix, and worth saying so plainly: the step counter each loop
  // keeps is ignored by the engine (`plumbline_engine_warm_step` takes `_step`),
  // and every call advances ONE shared phase counter — so two loops split the work
  // rather than duplicating it. Measured before and after: ~1,198 warm steps
  // either way. The steps are small on purpose, one budgeted slice per macrotask,
  // which is what keeps layout and tap RPCs answerable.
  //
  // What it does buy is a well-defined re-warm. With two drivers the "run again
  // after the R&D pack lands, to pick up the machine-tier indexes" pass was racy —
  // whichever loop happened to still be alive absorbed the second call. Now the
  // second caller joins the live pass, and because `warmRun` is cleared on
  // completion a genuinely later call still gets a fresh one. Every phase is
  // idempotent and the counter is shared, so joining mid-pass is safe.
  return (warmRun ??= (async () => {
    for (let step = 0; ; step++) {
      await yieldTask();
      const more = timedChunk(`warm step ${step}`, () => booted!.engine.warmStep(step));
      if (!more) break;
    }
    // The engine refuses to build an index inside a reader's tap while this warm
    // is running (see `defer_builds` in crates/ffi), so a study opened mid-warm
    // comes back with only the sections that were ready. Tell the shell the rest
    // exist now, or it shows that thinner answer until something unrelated
    // happens to re-fetch.
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
    // Downloaded; now the expensive part. Parsing the morphology is megabytes of
    // text, seconds of it on a phone — one artifact per macrotask so a tap in
    // between is still answered, and the shell is told we've moved from
    // downloading to preparing (the bar sat at 0% and the study sheet said
    // "— loading —" through the whole thing).
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

/** Everything beyond the text, AFTER the reader hands over (TODO #28 — text
 *  on screen is the north star): stage-2 core files (Strong's, cross-refs,
 *  margin notes) → warm → the R&D pack unless deferred (phones defer; the
 *  shell offers an explicit "load analysis" action instead). */
/** Data Saver — the one setting that still means "ask me before you spend". */
const saveData = (): boolean => (navigator as any).connection?.saveData === true;

/** Is the machine-tier pack already on this device? Then loading it costs no
 *  network at all, and putting a one-time-download button in front of a download
 *  that will not happen is theatre. The deferral exists to protect the
 *  reader's data and their first paint; neither is at stake once the bytes are
 *  cached. */
async function rndAlreadyCached(): Promise<boolean> {
  const files = booted!.manifest.files.filter((f) => f.stage === "analysis");
  if (!files.length) return false;
  for (const f of files) {
    if (!(await depotHas(packFileUrl(f, booted!.manifest.version)))) return false;
  }
  return true;
}

/** Whether this session will fetch the machine tier by itself. Phones defer it
 *  out of the BOOT path, never out of the session — the reader should not have
 *  to ask for it again on every launch. */
async function willAutoLoadRnd(machineOn: boolean, deferRnd: boolean): Promise<boolean> {
  if (!machineOn) return false;
  if (!deferRnd) return true;
  return (await rndAlreadyCached()) || !saveData();
}

/** Notice a data update, download only what changed, and re-pin — all of it OFF
 *  the boot path.
 *
 *  This is where the manifest fetch went. Boot no longer asks the network
 *  anything on a warm launch; the live manifest is fetched once here, after the
 *  reader already has text on screen, and diffed against the pin.
 *
 *  Because URLs are content-addressed on each file's own hash, a release that
 *  changes one weave downloads one weave — and unchanged files keep their URLs, so
 *  re-pinning copies nothing. The new bytes land BESIDE the old ones and the pin
 *  is rewritten only after every file it will name is verified present, so an
 *  interrupted update leaves the previous generation intact and bootable.
 *
 *  The new pack applies at the NEXT launch, deliberately. This session's engine
 *  has its text in wasm memory and the reader is mid-verse; swapping the corpus
 *  under them would be worse than waiting. */
async function reconcilePack(): Promise<void> {
  const live = await fetchManifest();
  if (live.version === booted!.packVersion) return; // nothing deployed since
  const t0 = performance.now();
  let fetched = 0;
  // What this device's pack IS — the optional bundle only where the reader
  // installed it. The same call decides what gets pinned below, so the sweep
  // and the pin can never disagree about which files should be here.
  const mine = devicePackFiles(live, (f) => hasOptional(booted!.home, f));
  for (const f of mine) {
    const url = packFileUrl(f, live.version);
    if (await depotHas(url)) continue; // unchanged: same hash, same URL, already here
    await depotBytes(url);
    // Verify what just arrived. A hash mismatch means a truncated body or an error
    // page served 200 — store that unchecked and the engine fails to parse it on
    // every launch with no way out. Bail and keep the pin we have.
    if (!(await verifyStored(f, live.version))) return;
    fetched++;
  }
  // Verify before committing. Cheap — a metadata lookup per file, no bodies read.
  for (const f of mine) {
    if (!(await depotHas(packFileUrl(f, live.version)))) return; // incomplete: keep the old pin
  }
  await writePin(live, assetUrl(""), mine);

  // From here the session speaks about the pack it actually has.
  booted!.manifest = live;

  booted!.trace.push([`reconciled to ${live.version} (${fetched} files)`, Math.round(performance.now() - t0)]);
  self.postMessage({ type: "packUpdated", version: live.version });
}

async function backgroundLoad(machineOn: boolean, deferRnd: boolean): Promise<void> {
  await Promise.race([firstLayout, new Promise((r) => setTimeout(r, 2500))]);
  try {
    // WHICH PACK ARE WE ACTUALLY ON? Ask before stage 2 — but only after an
    // upgrade.
    //
    // A warm boot's manifest IS the pin, and the pin describes the release this
    // device last completed. Stage 2 then fetches the study files THAT release
    // listed, so a file added since is never fetched and the feature reading it
    // is simply missing.
    //
    // Gated on the pin coming from an older BUILD, because a warm boot on an
    // unchanged release must ask the network for nothing whatsoever — not even
    // 5 KB (e2e/app.spec.ts counts requests). So: one extra fetch on the first
    // launch after an upgrade, none ever again, and `fetchManifest` falls back to
    // the stored copy so an offline upgrade behaves exactly as before.
    //
    // Everything downstream then works through the path that already existed,
    // which is the point — the alternative was a second mechanism for injecting
    // late files into a running engine.
    if (booted!.staleManifest) {
      try {
        booted!.manifest = await fetchManifest();
        booted!.trace.push(["manifest refreshed (newer build than pin)", 0]);
      } catch {
        /* offline: the pin's manifest is what we have, and it is enough */
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
    // Both are read exactly once, by loadCoreData, and the parsed forms live in
    // the engine from here on. NOT the margin notes, which load_study re-reads on
    // every authoring write, and NOT cross-references.tsv, whose lazy index can
    // still be built on an arbitrary later tap.
    // Every dictionary the pack could have delivered, not a hand-kept list:
    // whichever one `strongs_for` picked, loadCoreData has parsed it and the
    // home's copy is duplication. The paths come off the manifest, so a
    // language added to the registry is evicted by having been added.
    const freedCore = booted!.home.evict([
      "data/strongs.json",
      "data/akjv.akjvb",
      ...booted!.manifest.files.filter((f) => f.role?.startsWith("lexicon:")).map((f) => f.path),
    ]);
    if (freedCore) booted!.trace.push(["home evict after stage 2 (KB)", Math.round(freedCore / 1024)]);
    self.postMessage({ type: "coreReady" });
    await warmChunked();
    // BEFORE the analysis pack, not after. Reconciling is normally one 5 KB
    // manifest fetch and a pile of hash comparisons; queueing it behind a
    // megabyte of optional analytics meant a device could sit on a stale pin for
    // the length of that download, and pick the update up a launch later than it
    // needed to.
    // In its own try: a failed update must not cost the reader anything they
    // already have, and being offline here is the normal case, not an error.
    try {
      await reconcilePack();
    } catch {
      /* offline or a stalled manifest — the pin stands, the next launch retries */
    }
    if (await willAutoLoadRnd(machineOn, deferRnd)) await loadRndChunked();
  } catch {
    /* offline — the Settings toggle or next boot retries */
  }
}

/** Reclaim everything the device no longer needs.
 *
 *  An ALLOWLIST, not a denylist: keep what the pin (and the generation before it)
 *  names, plus the shell this build is made of, and delete the rest. The old rule
 *  was "delete versioned entries whose `?v=` is not the current pack" — which
 *  could not see per-file hashes, and could not reclaim a file dropped from the
 *  pack entirely, because nothing referenced its version any more.
 *
 *  HARD PRECONDITION: prune only runs with a readable pin AND a non-empty shell
 *  list. Without both, the keep-set is incomplete and an allowlist would delete
 *  the app. Skipping costs nothing but disk; getting it wrong costs the reader
 *  their offline copy.
 *
 *  TWO generations are kept, and prune runs at the START of a session rather than
 *  the end. That buys "one generation of grace" with no cross-tab coordination
 *  and no lock: a tab still reading the previous pack keeps working, and the worst
 *  case is one superseded pack lingering until the next launch — against the old
 *  behaviour, which was unbounded and had stranded three whole packs. */
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
  // manifest (it is the worker's to fetch, and far too big for a shell list).
  // From boot.ts, which is also what prefetches it: an allowlist that spells the
  // URL out for itself deletes the engine the day the spelling changes.
  keep.add(engineUrl());
  // NOTHING ELSE IS EXEMPT. A retired artifact like the old "verses like this"
  // model is unknown to this list, which is exactly right — an allowlist reclaims
  // it on the next launch of a device that still carries one.

  let gone = 0;
  for (const url of await depotKeys()) {
    if (keep.has(url)) continue;
    if (await depotDelete(url)) gone++;
  }
  return gone;
}

// ── persisting the reader's own work ─────────────────────────────────────────
// A plain debounced `void persistUserData()` loses writes two ways, and both are
// guarded against here:
//
//  * A FAILED WRITE must not go nowhere. QuotaExceededError on a full phone, or a
//    browser that has decided this origin may not have a database, rejects a
//    promise nobody is holding — while the shell has already told the reader
//    their note was saved. It was saved only in the in-memory home, which dies
//    with the tab. So failures come back out to the shell, and are retried.
//  * A TAB THAT GOES AWAY inside the debounce loses the note entirely: 50 ms is
//    nothing, and a hidden page has its timers frozen, so the pending callback
//    may simply never run. `flush` awaits the write instead of scheduling it,
//    and the main thread calls it on pagehide / visibilitychange-hidden.
//
// Everything funnels through `persistNow`, so "did this get told about?" has one
// answer. The multi-tab contract is untouched: the flush runs the SAME per-file
// diff (see home.ts), one moment earlier.
const PERSIST_DEBOUNCE = 50;
/** Backoff between retries, ms. It ENDS. A device that has refused five times is
 *  out of room rather than busy, and a timer that never stops costs battery
 *  while promising something we cannot deliver; the shell's notice carries a
 *  "Try again" the reader can use once they have freed some space. */
const PERSIST_BACKOFF = [250, 1_000, 4_000, 15_000];

let persistTimer: ReturnType<typeof setTimeout> | null = null;
let persistRetry: ReturnType<typeof setTimeout> | null = null;
let persistTries = 0;
/** Whether the shell is currently showing a failure notice — so a healthy
 *  session posts nothing at all, and a recovered one posts exactly once. */
let persistFailing = false;

/** What went wrong, in the browser's own words. An aborted IndexedDB transaction
 *  can reject with a NULL error, and "null" is not something a reader (or a bug
 *  report) can do anything with. */
function persistReason(e: unknown): string {
  if (e instanceof Error) return `${e.name}: ${e.message}`;
  return e == null ? "the browser gave no reason" : String(e);
}

/** Persist the authored subtree now. Resolves when THIS attempt has settled,
 *  which is what makes it awaitable from `flush`. Never rejects: a failed save
 *  is news for the reader, not an error for whoever happened to trigger it. */
async function persistNow(): Promise<void> {
  // Both timers are subsumed by running right now; leaving either armed would
  // double-write (and the debounce's write would diff against a moving tree).
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
    // `detail` is the browser's own words ("QuotaExceededError…"). The shell does
    // not shout it at the reader, but a failure we cannot name is a failure
    // nobody can act on, so it travels.
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
 * Load one family's faces into the worker's OWN FontFaceSet and point the
 * measure context at it. Workers do not share the document's fonts, and
 * measurement must see the REAL metrics of the face the main thread paints, or
 * lines wrap where they are not drawn.
 *
 * Returns how many faces the worker now has for that family — counted, not just
 * attempted, because a silent failure here is invisible until a reader notices
 * odd wrapping. The shell compares it against what the family should have.
 */
async function loadFonts(base: string, token: string): Promise<number> {
  const resolved = setReaderFont(token);
  const files = FONT_FILES[resolved];
  const want = files.italic ? 2 : 1;
  const scope = self as unknown as { fonts?: FontFaceSet };
  if (!scope.fonts) return 0; // very old engines: fall back to platform metrics
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
        // 400 700, not the file's own axis range: Fira Code's `wght` DEFAULTS to
        // 300, so a declaration that let the default through would measure and
        // paint the Light instance as body text.
        weight: "400 700",
      });
      await face.load();
      scope.fonts.add(face);
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
    // Engine-independent, both of them: a share link is pure string work over
    // the church clamps, and the reading spec is the core's own tuning table.
    // They are on the STATIC path so the shell can ask for them before (or
    // without) an engine, which is what the church button and the dwell timer
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
        // Before anything else, so the meter covers the whole boot — including
        // the downloads, which is the window the numbers are argued over.
        if (PERF) startStallMeter();
        // THE FONTS OVERLAP THE WHOLE BOOT. They are needed before the first
        // LAYOUT, not before the boot — and awaited here they held everything
        // behind two font files: the pack download, and the splash's first
        // progress message with it, so the boot watchdog's opening silence
        // window was spent on fonts before the reader saw a single percent.
        const t0 = performance.now();
        let fontsMs = 0;
        const fonts = loadFonts(m.base, m.textFont ?? DEFAULT_FONT).then((n) => {
          fontsMs = Math.round(performance.now() - t0);
          return n;
        });
        // Handled immediately so nothing here can become an unhandled rejection
        // while it is un-awaited; the await below still sees a failure and it
        // lands in the same catch as any other boot failure. `loadFonts` swallows
        // a refused face itself, so this is the belt for a platform that throws
        // somewhere it does not expect to.
        void fonts.catch(() => {});
        booted = await boot((p) => self.postMessage({ type: "progress", ...p }), m.locale ?? "", m.lang ?? "");
        // BEFORE the reply, and therefore before any layout op can be answered:
        // measurement must see the real metrics of the reader's chosen face or
        // lines wrap where they are not painted.
        const w0 = performance.now();
        const fontFaces = await fonts;
        booted.trace.unshift(
          ["worker fonts", fontsMs],
          ["worker font faces", fontFaces],
          ["worker fonts wait after boot", Math.round(performance.now() - w0)],
        );
        booted.engine.onAuthored = () => {
          schedulePersist();
          // The alt engines hold their own view of the study files this write
          // just changed. Re-read before the shell re-fetches, or a tag made on
          // the English pane is missing from the German pane's word study until
          // the next reload.
          refreshAlts();
          self.postMessage({ type: "authored" });
        };
        // Dwell reports persist ONLY the reading dir — see onReadingWrite — and
        // announce themselves SEPARATELY from `authored`. The shell has to drop
        // its cached reading reads or the navigator keeps painting the map it
        // fetched hours ago; it must not drop everything else with them, which is
        // the same reason this does not just fire `authored`.
        //
        // Its own retry ladder would be pointless — the next dwell tick rewrites
        // the same file — but the reader still hears about a device that is
        // refusing writes, because it is refusing THEIR notes too.
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
        // The language was set inside `boot`, before the engine opened — which is
        // where it has to be, because the engine picks WHICH CORPUS to open.
        // Opt-in: absent means off, so a first visit does NOT pull the analysis
        // pack in the background.
        const machineOn = cfg.machineAnalysis === true;
        // Tell the shell up front, so it never offers a "Load analysis" button
        // for a load that is already on its way.
        const rndAuto = await willAutoLoadRnd(machineOn, m.deferRnd === true);
        // What the folded-in reads actually cost this device, in the same trace
        // as every other boot stage. It is the same work either way; what changed
        // is that it no longer needs four more queue hops to deliver it.
        const x0 = performance.now();
        // Every concrete theme (core::theme::Theme) — the palettes the shell
        // paints from without a round trip. Keep in step with the Rust enum and
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
        // Every word the shell paints, resolved against the reader's setting and
        // the device's locale by the CORE (i18n::resolve), not by either shell.
        // Here rather than as its own call for the palettes' reason: this thread
        // answers one thing at a time, and the shell cannot paint a single screen
        // without it.
        const i18n = i18nCatalog(booted.wasm, typeof cfg.language === "string" ? cfg.language : "", m.locale ?? "");
        booted.trace.push(["boot reply extras (palettes + toc + i18n)", Math.round(performance.now() - x0)]);
        void backgroundLoad(machineOn, m.deferRnd === true);
        reply({
          packVersion: booted.packVersion,
          config: cfg,
          version: engineVersion(booted.wasm),
          bundledOn: booted.home.bundledOn,
          rndAuto,
          fontFaces,
          // FOLDED INTO THE BOOT REPLY. This thread is the only one
          // that can answer anything, so the shell's first four reads — three
          // theme palettes and the TOC — were four more full queue hops between
          // the boot reply and the first layout request, on the one path where
          // nothing else can proceed. They are pure functions of the engine that
          // just opened, so the reply that says "open" can carry them.
          //
          // Both are session-immutable and that is why this is safe to hand over
          // once: the palettes are compiled-in tables (crates/core/src/theme.rs)
          // and the TOC is corpus-derived, which is the same reason
          // session.svelte.ts PINS the TOC in its read-through cache.
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
      // The same call, against another language's text — word study on a German
      // pane, its verse text, its chapter counts. The METHOD NAME is unchanged:
      // every StudyEngine read works on any handle, so per-pane language costs
      // no per-feature RPC (docs/PER-PANE-LANGUAGE.md).
      //
      // AUTHORING NEVER COMES THROUGH HERE. Two writers over one home is a
      // corruption story the atomic store should not be asked to survive, and
      // the reader's data is shared anyway — so a write is the primary's, and
      // the alt engines re-read it.
      case "callIn": {
        const e = engineFor(m.lang as string) as unknown as Record<string, (...a: any[]) => unknown>;
        reply(timedCall(`${m.method}@${m.lang}`, () => e[m.method](...m.args)));
        break;
      }
      case "static": {
        reply(timedCall(m.fn, () => statics()[m.fn](...m.args)));
        // Config writes land in the in-memory WASI home; mirror them to
        // IndexedDB like any authoring write.
        //
        // NOT `schedulePersist()`: config is written at exactly the moments the
        // page is about to go away — answering first run, then reloading;
        // choosing a theme, then closing the tab — and a 50 ms debounce is long
        // enough to lose all of them. `persistNow()` still reports failure and
        // still retries; it just does not wait first. Debouncing this loses the
        // first-run choice across a relaunch (app.spec.ts pins that it survives).
        if (m.fn === "configSave") void persistNow();
        break;
      }
      case "layout": {
        reply(timedCall(`layout ${m.book} ${m.chapter}`, () => layoutChapter(m)));
        break;
      }
      case "prefetch": {
        // Warm the turn cache without shipping the display list back over
        // postMessage — the shell asks for the neighbouring chapters at idle,
        // so the next page turn is a cache hit.
        layoutChapter(m);
        reply(null);
        break;
      }
      case "layoutTrace": {
        reply(lastTurn);
        break;
      }
      case "setTextFont": {
        // The reader changed the scripture face. Load it here BEFORE replying:
        // the shell relayouts as soon as this resolves, and a layout measured
        // before the face arrived would be measured in the fallback and painted
        // in the real one. Already-loaded families resolve immediately.
        reply(await loadFonts(m.base, m.token));
        break;
      }
      case "fontExtent": {
        reply(fontExtent(m.px));
        break;
      }
      case "measure": {
        // One-off text widths for callers that need engine-side metrics
        // (space width etc. travel inside the layout cfg instead — this is
        // a utility escape hatch).
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
        // Everything the worker knows, in one round trip, so a report cannot be
        // assembled from readings taken at different moments.
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
        // The reader is leaving (or has asked us to try again). Persist NOW and
        // answer only once the transaction has settled, so the caller knows
        // whether it was worth waiting for — a `void` here would recreate the
        // very hole this op closes.
        //
        // Boot may not have finished: a tab hidden during a cold boot has no home
        // to write, and nothing was authored either, so there is nothing to lose.
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
        // What the Settings row needs to draw itself: whether this home already
        // has the set, and what asking for it would cost. Size comes from the
        // manifest rather than a constant, so the row can never quote a number
        // the pack has since changed.
        const entry = suggestedWeavesEntry(booted!.manifest);
        reply({
          available: entry !== null,
          installed: booted!.home.suggestedInstalled,
          gzBytes: entry?.gzBytes ?? 0,
        });
        break;
      }
      case "installSuggested": {
        // One file, so no progress ladder: it is ~110 KB, and a bar that jumps
        // 0→100 says less than the button simply staying busy.
        const bundle = await fetchSuggestedWeaves(booted!.manifest);
        if (!bundle) {
          fail("this build has no suggested-weave bundle");
          break;
        }
        const written = await booted!.home.installSuggestedWeaves(bundle);
        // RE-PIN. The bundle is part of this device's pack now, and prune keeps
        // only what the pin names — without this the next sweep reclaims the
        // download, and the device then looks like one that declined it.
        await writePin(booted!.manifest, assetUrl(""), devicePackFiles(booted!.manifest, (f) => hasOptional(booted!.home, f)));
        // The engine holds the weave library it read at open, so the files are
        // on disk and invisible until it re-reads them. Same call stage 2 uses
        // for exactly this reason.
        booted!.engine.loadCoreData();
        self.postMessage({ type: "authored" });
        reply(written);
        break;
      }
      // ── a language's scripture ───────────────────────────────────────────
      // The reader picking the language IS the ask, so there is no Settings
      // row: the picker calls this and then reloads. A progress fraction,
      // unlike the suggested bundle, because this is a couple of MB and a phone
      // deserves to see it moving.
      // A language no pane reads any more is a Bible-sized allocation doing
      // nothing. The SHELL owns the answer to "which are still in use" — it is
      // the thing that knows what every pane is reading — so it passes the list
      // and this frees the rest.
      case "releaseLangs": {
        const keep = new Set((m.keep as string[]) ?? []);
        let freed = 0;
        for (const [code, engine] of [...altEngines]) {
          if (keep.has(code)) continue;
          engine.free();
          altEngines.delete(code);
          freed++;
        }
        // The turn cache holds display lists laid out from those engines; a
        // stale entry would be served if the reader came back to that language
        // before anything else evicted it, and its engine is gone.
        if (freed) turnCache.clear();
        reply(freed);
        break;
      }
      case "wasmMemoryBytes": {
        reply(booted!.wasm.exports.memory.buffer.byteLength);
        break;
      }
      case "langPackState": {
        const entry = langCorpusEntry(booted!.manifest, m.code as string);
        reply({
          available: !!entry,
          installed: booted!.home.langsInstalled.has(m.code as string),
          gzBytes: entry?.gzBytes ?? 0,
        });
        break;
      }
      // OPEN A LANGUAGE FOR A PANE — download its text if this device has not
      // got it, then open a second engine on it. No reload, which is the whole
      // difference from the settings switch below: the English pane must keep
      // reading while the German text arrives.
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
          // That language's own dictionary rides the same ask: without it a
          // German pane's word study would serve English definitions, which is
          // half a translation.
          await fetchLangLexicon(booted!.manifest, code);
          await booted!.home.installLangCorpus(code, entry.path, cache);
          return true;
        };

        if (!booted!.home.langsInstalled.has(code) && !(await supply())) {
          fail(`the ${code} text could not be downloaded`);
          break;
        }
        // OPENING IS SYNCHRONOUS — a corpus open on the one thread that also
        // answers layout and taps. It reads the idxcache rather than parsing
        // JSONL, so it is a load and a directory, not a canon-wide decode.
        //
        // RETRIED ONCE THROUGH THE SUPPLY PATH, because the home is not a
        // promise: the eviction below frees this language's bytes after the
        // engine has read them, so a reader who goes back to German later in
        // the session finds the file gone. The depot still has it, so the cold
        // path is the repair — exactly as it is for the core pack.
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
        // EVICT WHAT THE ENGINE HAS ALREADY READ, exactly as the core boot does
        // after stage 2. The WASI shim's `File` copies its input, so until this
        // runs the home holds a second copy of every byte the corpus and the
        // dictionary just parsed — ~33 MB per language, measured.
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
        const entry = langCorpusEntry(booted!.manifest, code);
        const cache = await fetchLangCorpus(booted!.manifest, code, (p) =>
          self.postMessage({ type: "langPackProgress", fraction: p.fraction }),
        );
        if (!cache || !entry) {
          fail(`this build has no ${code} corpus`);
          break;
        }
        // That language's lexicon rides the same ask, into the depot — the
        // reload that follows reads it back through stage 2. A pack without one
        // is fine: study serves the English dictionary until it ships.
        await fetchLangLexicon(booted!.manifest, code);
        await booted!.home.installLangCorpus(code, entry.path, cache);
        // RE-PIN, for `installSuggested`'s reason: the corpus is part of this
        // device's pack now, and prune keeps only what the pin names — without
        // this the next sweep reclaims a 28 MB download.
        await writePin(booted!.manifest, assetUrl(""), devicePackFiles(booted!.manifest, (f) => hasOptional(booted!.home, f)));
        // NOT `loadCoreData`: the corpus is chosen when the engine OPENS, so
        // this one needs a reload rather than a re-read. The picker does that.
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
