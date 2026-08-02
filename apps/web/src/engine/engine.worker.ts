// The engine worker (TODO #28): the WHOLE engine life runs here — pack fetch,
// IndexedDB home, wasm instantiate, engine open, warm, the deferred R&D load,
// authoring writes and their persistence. The main thread never blocks on an
// engine call again; it speaks the tiny RPC below and paints.
//
// SCHEDULING RULE (2026-07-26, the half-width-pane bug): this thread is the
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
import type { PackFile, PackManifest } from "./pack";
import {
  assetUrl,
  fetchManifest,
  fetchRndPack,
  fetchStage2Pack,
  fetchSuggestedWeaves,
  packFileUrl,
  setAssetBase,
  suggestedWeavesEntry,
  takePackTrace,
  verifyStored,
} from "./pack";
import { depotBytes, depotDelete, depotHas, depotKeys } from "./depot";
import { PERF } from "./perf";
import { measureFor, readerFont, fontExtent } from "../reader/measure";
import {
  aboutBlocks,
  configLoad,
  configSave,
  engineVersion,
  guideBlocks,
  routeLink,
  themePalette,
  shareLink,
  readingSpec,
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
// That second effect is INVISIBLE in a wall-clock timer, and it burned us: a
// phone reported "stage2 fetch+gunzip: 33993 ms" where a desktop over localhost
// reported 907 ms, and dividing bytes by seconds produced a confident, invented
// "200 KB/s connection" (2026-07-28). The bytes-per-second of a starved reader
// measures the starvation, not the network.
//
// So measure the starvation directly. A heartbeat that should fire every tick,
// and how late it actually was. Total lateness during boot IS the time this
// thread spent unavailable — no division, no inference, no assumption about
// anyone's wifi.
// A LATE TIMER IS NOT ALWAYS A BUSY THREAD, and the first version of this meter
// could not tell the difference. It reported a 24,921 ms "stall" on a launch
// where every byte came off the device and the largest measured chunk was 886 ms
// — arithmetic that cannot block for 25 seconds. The cause was the tab going to
// the background: Chrome freezes a hidden page's timers AND its in-flight
// requests, so the meter billed the reader's screen turning off as engine work,
// and on the launch before it billed a backgrounded download as a 34-second
// fetch of a 787 KB file (2026-07-28).
//
// A measurement that cannot distinguish "we were busy" from "the phone was
// asleep" is worse than none: it invents a crisis and points at the wrong code.
// So the page tells the worker when it is hidden, hidden time is excluded rather
// than counted, and it is reported separately — because "you were away for 25 s"
// is also a thing a reader of this report needs to know.
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
 *  without it, flipping the toggle served the cached KJV display list straight
 *  back and the page never changed (feedback 2026-07-27, "isn't live").
 *  Keeping it in the key rather than clearing the cache means toggling back and
 *  forth stays free, which is exactly what someone comparing wordings does. */
let akjvOn = false;

interface LayoutReq {
  book: string;
  chapter: number;
  font: number;
  width: number;
  lineSpacing: number;
  versePerLine: boolean;
}

function layoutChapter(m: LayoutReq): LaidOut | null {
  const key = `${m.book} ${m.chapter}|${m.font}|${m.width}|${m.lineSpacing}|${m.versePerLine}|${akjvOn}`;
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
  };
  const t1 = performance.now();
  const crossings0 = PERF ? booted!.wasm.measureCalls() : 0;
  const dl = booted!.engine.layoutChapter(m.book, m.chapter, cfg);
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

// NOTHING THE ENGINE BUILDS IS PERSISTED ANY MORE. There was one exception: the
// "verses like this" (SIF) model, the most expensive thing a launch did — 11.2 s
// of phone CPU, 41 sweeps of the whole corpus (2026-07-28) — which this worker
// saved into the depot after the warm built it and reinstalled on the next open.
// The feature was removed 2026-07-30 with the concept embedding it was built
// from, and the saved blob with it: prune is an allowlist, so any copy still on a
// device is reclaimed on that device's next launch.

// ── engine calls ──────────────────────────────────────────────────────────────
// EVERY engine request the shell makes arrives as `call` or `static`, and until
// now not one of them was timed. That is the hole three days of traces kept
// falling into: the boot stages, the warm chunks and the analysis chunks all
// report themselves, so a trace looks complete — while a single `call` can hold
// this thread for as long as it likes and leave no mark anywhere.
//
// It is not a hypothetical hole. `wordStudyBlocks` builds the occurrence index,
// the rendering lens, the cross-references, the concept model and the bridge
// SYNCHRONOUSLY when the reader taps a word before the warm has reached them —
// 818 ms on a desktop, and a phone reported a 24,921 ms freeze that no timed
// section accounted for (2026-07-28). A frozen thread also strands its own
// in-flight downloads, which is how one 787 KB file came to be reported at
// 34,448 ms beside a 3.3 MB file that took 475 ms.
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
    // Both are parsed into wasm memory by the steps above and never re-read. The
    // concept vectors used to be evicted here too; that artifact was dropped from
    // the pack on 2026-07-30 with the features that read it.
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
 *  cached (feedback 2026-07-27). */
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
/** The files THIS device's pack actually consists of.
 *
 *  Every stage but `optional` always counts. An `optional` entry counts only
 *  when this device already holds the previous generation of it — that is what
 *  "the reader asked for this" looks like a release later, and without the
 *  distinction the update sweep would download the suggested-weave bundle onto
 *  every device on the next deploy, which is the whole thing it exists to
 *  avoid. It also must not gate the pin: a device that never wanted the bundle
 *  would otherwise fail the completeness check forever and never re-pin again. */
async function thisDevicesFiles(live: PackManifest): Promise<PackFile[]> {
  const out: PackFile[] = [];
  for (const f of live.files) {
    if (f.stage !== "optional") {
      out.push(f);
      continue;
    }
    const prev = booted!.manifest.files.find((p) => p.path === f.path);
    if (prev && (await depotHas(packFileUrl(prev, booted!.manifest.version)))) out.push(f);
  }
  return out;
}

async function reconcilePack(): Promise<void> {
  const live = await fetchManifest();
  if (live.version === booted!.packVersion) return; // nothing deployed since
  const t0 = performance.now();
  let fetched = 0;
  const mine = await thisDevicesFiles(live);
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
  await writePin(live, assetUrl(""));
  booted!.trace.push([`reconciled to ${live.version} (${fetched} files)`, Math.round(performance.now() - t0)]);
  self.postMessage({ type: "packUpdated", version: live.version });
}

async function backgroundLoad(machineOn: boolean, deferRnd: boolean): Promise<void> {
  await Promise.race([firstLayout, new Promise((r) => setTimeout(r, 2500))]);
  try {
    const t0 = performance.now();
    const files = await fetchStage2Pack(booted!.manifest);
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
    const freedCore = booted!.home.evict(["data/strongs.json", "data/akjv.akjvb"]);
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
  // NOTHING ELSE IS EXEMPT. The saved "verses like this" model (`verse-sim.simb`)
  // was named here until 2026-07-30; with the feature gone the blob is unknown to
  // this list, which is exactly right — an allowlist reclaims it on the next
  // launch of a device that still carries one.

  let gone = 0;
  for (const url of await depotKeys()) {
    if (keep.has(url)) continue;
    if (await depotDelete(url)) gone++;
  }
  return gone;
}

// ── persisting the reader's own work ─────────────────────────────────────────
// This used to be `void booted.home.persistUserData()` inside a 50 ms debounce,
// and it lost writes two different ways.
//
//  * A FAILED WRITE went nowhere. QuotaExceededError on a full phone, or a
//    browser that has decided this origin may not have a database, rejected a
//    promise nobody was holding — while the shell had already told the reader
//    their note was saved. It was saved in the in-memory home, which dies with
//    the tab. So failures come back out to the shell now, and are retried.
//  * A TAB THAT WENT AWAY inside the debounce lost the note entirely: 50 ms is
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

async function loadFonts(fontUrl: string, italicUrl: string): Promise<number> {
  // Workers have their own FontFaceSet; measurement must see the real
  // Garamond metrics or lines would wrap differently than they paint.
  const scope = self as unknown as { fonts?: FontFaceSet };
  if (!scope.fonts) return 0; // very old engines: fall back to serif metrics
  let loaded = 0;
  for (const [url, style] of [
    [fontUrl, "normal"],
    [italicUrl, "italic"],
  ] as const) {
    try {
      const face = new FontFace("EB Garamond", `url(${url})`, { style, weight: "400 700" });
      await face.load();
      scope.fonts.add(face);
      loaded++;
    } catch {
      /* platform-serif metrics still beat a dead worker */
    }
  }
  // Counted, not just attempted: a silent failure here is invisible until a
  // reader notices text wrapping oddly, so the count is reported to the shell
  // and asserted by an e2e test.
  return loaded;
}

function statics(): Record<string, (...a: any[]) => any> {
  const w = booted!.wasm;
  return {
    routeLink: (uri: string) => routeLink(w, uri),
    configLoad: () => configLoad(w),
    configSave: (cfg: unknown) => configSave(w, cfg),
    themePalette: (theme: string) => themePalette(w, theme),
    guideBlocks: () => guideBlocks(w),
    aboutBlocks: () => aboutBlocks(w),
    engineVersion: () => engineVersion(w),
    // Engine-independent, both of them: a share link is pure string work over
    // the church clamps, and the reading spec is the core's own tuning table.
    // They are on the STATIC path so the shell can ask for them before (or
    // without) an engine, which is what the church button and the dwell timer
    // both need (H-10, H-11).
    shareLink: (request: unknown) => shareLink(w, request as Parameters<typeof shareLink>[1]),
    readingSpec: () => readingSpec(w),
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
        const fonts = loadFonts(m.fontUrl, m.italicUrl).then((n) => {
          fontsMs = Math.round(performance.now() - t0);
          return n;
        });
        // Handled immediately so nothing here can become an unhandled rejection
        // while it is un-awaited; the await below still sees a failure and it
        // lands in the same catch as any other boot failure. `loadFonts` swallows
        // a refused face itself, so this is the belt for a platform that throws
        // somewhere it does not expect to.
        void fonts.catch(() => {});
        booted = await boot((p) => self.postMessage({ type: "progress", ...p }));
        // BEFORE the reply, and therefore before any layout op can be answered:
        // measurement must see the real Garamond metrics or lines wrap where they
        // are not painted.
        const w0 = performance.now();
        const fontFaces = await fonts;
        booted.trace.unshift(
          ["worker fonts", fontsMs],
          ["worker font faces", fontFaces],
          ["worker fonts wait after boot", Math.round(performance.now() - w0)],
        );
        booted.engine.onAuthored = () => {
          schedulePersist();
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
        // Opt-in: absent means off, so a first visit does NOT pull the analysis
        // pack in the background.
        const machineOn = cfg.machineAnalysis === true;
        // Tell the shell up front, so it never offers a "Load analysis" button
        // for a load that is already on its way (feedback 2026-07-27).
        const rndAuto = await willAutoLoadRnd(machineOn, m.deferRnd === true);
        // What the folded-in reads actually cost this device, in the same trace
        // as every other boot stage — so the thing F-11 moved is a number on a
        // real phone rather than an argument. It is the same work either way;
        // what changed is that it no longer needs four more queue hops to
        // deliver it.
        const x0 = performance.now();
        const palettes = {
          light: themePalette(booted.wasm, "light"),
          dark: themePalette(booted.wasm, "dark"),
          night: themePalette(booted.wasm, "night"),
        };
        const toc = booted.engine.toc();
        booted.trace.push(["boot reply extras (palettes + toc)", Math.round(performance.now() - x0)]);
        void backgroundLoad(machineOn, m.deferRnd === true);
        reply({
          packVersion: booted.packVersion,
          config: cfg,
          version: engineVersion(booted.wasm),
          bundledOn: booted.home.bundledOn,
          rndAuto,
          fontFaces,
          // FOLDED INTO THE BOOT REPLY (audit F-11). This thread is the only one
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
      case "static": {
        reply(timedCall(m.fn, () => statics()[m.fn](...m.args)));
        // Config writes land in the in-memory WASI home; mirror them to
        // IndexedDB like any authoring write. Before this, config persisted
        // only when an authoring write happened to follow — a pure reader's
        // first-run choice (and pane layout) never survived a relaunch.
        //
        // NOT `schedulePersist()`: config is written at exactly the moments the
        // page is about to go away — answering first run, then reloading;
        // choosing a theme, then closing the tab — and a 50 ms debounce is long
        // enough to lose all of them. `persistNow()` still reports failure and
        // still retries; it just does not wait first. Debouncing this reopened
        // the 2026-07-26 bug (the chooser returned on every launch) and
        // app.spec.ts's "the first-run choice survives a relaunch" caught it.
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
        // The engine holds the weave library it read at open, so the files are
        // on disk and invisible until it re-reads them. Same call stage 2 uses
        // for exactly this reason.
        booted!.engine.loadCoreData();
        self.postMessage({ type: "authored" });
        reply(written);
        break;
      }
      default:
        fail(`unknown op ${m.op}`);
    }
  } catch (e) {
    fail(e);
  }
};
