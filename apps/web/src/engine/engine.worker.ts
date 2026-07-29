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
//   in:  { id, op: "boot", base, fontUrl, italicUrl, deferRnd } → boots; progress streams
//   in:  { id, op: "call", method, args }           → StudyEngine[method](...args)
//   in:  { id, op: "static", fn, args }             → engine-independent fns
//   in:  { id, op: "layout", book, chapter, cfg, font } → display-list JSON
//   in:  { id, op: "loadRnd" }                      → deferred pack + re-warm
//   in:  { id, op: "bootTrace" }                    → [label, ms][] so far
//   in:  { id, op: "export" } / { id, op: "freeze" } / { id, op: "setBundled", on }
//   out: { id, result } | { id, error }
//   out: { type: "progress", phase, fraction?, detail? }   (during boot)
//   out: { type: "authored" }                       (any authoring write landed)
//   out: { type: "coreReady" } / { type: "rndReady" } / { type: "rndProgress", fraction }
//
// Layout measure runs HERE over an OffscreenCanvas (measure.ts adapts), so
// EB Garamond must be loaded into self.fonts before the first layout — the
// boot message carries the resolved font URL.

import { boot, type BootResult } from "./boot";
import { pinnedUrls, writePin } from "./pin";
import {
  assetUrl,
  fetchManifest,
  fetchRndPack,
  fetchStage2Pack,
  packFileUrl,
  setAssetBase,
  takePackTrace,
  verifyStored,
} from "./pack";
import { depotBytes, depotDelete, depotGet, depotHas, depotKeys, depotPut } from "./depot";
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
const TURN_CACHE_MAX = 8;
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

// ── the saved "verses like this" model ────────────────────────────────────────
// The most expensive thing a launch does — 11.2 s of phone CPU, 41 sweeps of the
// whole corpus (2026-07-28) — for a model that is a pure function of the corpus
// and the embedding, both of which the device already holds. It was recomputed
// on every single open because nothing an engine builds survives the tab.
//
// It lives in the DEPOT rather than IndexedDB, by the rule that governs both: the
// depot holds what can be re-derived, IndexedDB holds what cannot. This can be
// rebuilt from bytes already on the device, so losing it to eviction costs one
// slow launch and nothing else.
const SIF_URL = "__depot/verse-sim.simb";

/** What the model was built FROM. A cached model served against a different
 *  corpus or embedding answers with the WRONG VERSES and nothing throws — so the
 *  stamp is the whole safety story, and a mismatch means rebuild, never repair. */
function sifStamp(): string {
  return `sif1/${booted!.packVersion}`;
}

/** Restore the saved model, if this device has a matching one. */
async function loadSavedVerseSim(): Promise<boolean> {
  try {
    const hit = await depotGet(assetUrl(SIF_URL));
    if (!hit) return false;
    const bytes = new Uint8Array(await hit.arrayBuffer());
    const t0 = performance.now();
    const ok = booted!.engine.verseSimLoad(bytes, sifStamp());
    booted!.trace.push([
      ok ? "verses-like-this RESTORED (no rebuild)" : "verses-like-this cache refused",
      Math.round(performance.now() - t0),
    ]);
    return ok;
  } catch {
    return false; // storage blocked or unreadable: build it, same as a first run
  }
}

/** Save it, once, after the warm has built it. */
async function saveVerseSim(): Promise<void> {
  try {
    const t0 = performance.now();
    const bytes = booted!.engine.verseSimSave(sifStamp());
    if (!bytes) return; // not built (no analysis pack on this device)
    const stored = await depotPut(assetUrl(SIF_URL), bytes, "application/octet-stream");
    if (PERF && stored) {
      booted!.trace.push([
        `verses-like-this saved (KB)`,
        Math.round(bytes.length / 1024),
      ]);
      booted!.trace.push(["verses-like-this save", Math.round(performance.now() - t0)]);
    }
  } catch {
    /* quota or blocked storage: the reader just pays the rebuild next launch */
  }
}

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
 *  the R&D pack lands — the SIF model only builds once the embedding is in). */
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
  // after the R&D pack lands, to pick up the SIF model" pass was racy — whichever
  // loop happened to still be alive absorbed the second call. Now the second
  // caller joins the live pass, and because `warmRun` is cleared on completion a
  // genuinely later call still gets a fresh one. The engine's tail phase goes back
  // for the SIF model once an embedding is present, so joining mid-pass is safe.
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
    // Downloaded; now the expensive part. Parsing the embedding and the
    // morphology is ~17 MB of text, seconds of it on a phone — one artifact
    // per macrotask so a tap in between is still answered, and the shell is
    // told we've moved from downloading to preparing (the bar sat at 0% and
    // the study sheet said "— loading —" through the whole thing).
    self.postMessage({ type: "rndPreparing" });
    booted!.home.addFiles(files);
    for (let step = 0; ; step++) {
      await yieldTask();
      const more = timedChunk(`rnd load step ${step}`, () => booted!.engine.loadRndStep(step));
      if (!more) break;
    }
    // The embedding and the morphology are parsed into wasm memory by the steps
    // above and never re-read; the two vector sidecars are only ever arguments to
    // that same parse, so they go as one unit with it.
    const freedRnd = booted!.home.evict([
      "data/concept-vectors.vecb",
      "data/concept-vectors.vec.meta",
      "data/concept-vectors.vec.freq",
      "data/morphology.morphb",
      "data/text-witness.json",
    ]);
    if (freedRnd) booted!.trace.push(["home evict after analysis (KB)", Math.round(freedRnd / 1024)]);
    // The saved model, before the warm would rebuild it. The embedding has just
    // landed, which is the earliest moment a stored model can be installed — and
    // installing it makes the warm's SIF phase a no-op instead of 11 seconds.
    const restored = await loadSavedVerseSim();
    await warmChunked();
    // Built for the first time on this device: keep it. Off the critical path,
    // after the reader already has everything.
    if (!restored) await saveVerseSim();
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
 *  network at all, and putting a "one-time ~4 MB download" button in front of a
 *  download that will not happen is theatre. The deferral exists to protect the
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
async function reconcilePack(): Promise<void> {
  const live = await fetchManifest();
  if (live.version === booted!.packVersion) return; // nothing deployed since
  const t0 = performance.now();
  let fetched = 0;
  for (const f of live.files) {
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
  for (const f of live.files) {
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
    // manifest fetch and a pile of hash comparisons; queueing it behind ~4 MB of
    // optional analytics meant a device could sit on a stale pin for the length
    // of that download, and pick the update up a launch later than it needed to.
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
  keep.add(assetUrl(`plumbline_ffi.wasm?v=${__BUILD_ID__}`));
  // The saved "verses like this" model. Prune is an ALLOWLIST, so anything not
  // named here is deleted — and this was, on the very next launch, which is why
  // the first version of it re-saved 12 MB every open and never once restored.
  // Its own stamp handles staleness; prune must not second-guess that.
  keep.add(assetUrl(SIF_URL));

  let gone = 0;
  for (const url of await depotKeys()) {
    if (keep.has(url)) continue;
    if (await depotDelete(url)) gone++;
  }
  return gone;
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
        const t0 = performance.now();
        const fontFaces = await loadFonts(m.fontUrl, m.italicUrl);
        const fontsMs = Math.round(performance.now() - t0);
        booted = await boot((p) => self.postMessage({ type: "progress", ...p }));
        booted.trace.unshift(["worker fonts", fontsMs], ["worker font faces", fontFaces]);
        let persistPending = false;
        booted.engine.onAuthored = () => {
          if (!persistPending) {
            persistPending = true;
            setTimeout(() => {
              persistPending = false;
              void booted!.home.persistUserData();
            }, 50);
          }
          self.postMessage({ type: "authored" });
        };
        // Dwell reports persist ONLY the reading dir — see onReadingWrite.
        booted.engine.onReadingWrite = () => {
          void booted!.home.persistUserDir("reading");
        };
        const cfg = configLoad(booted.wasm) ?? {};
        // Opt-in: absent means off, so a first visit does NOT pull the analysis
        // pack in the background.
        const machineOn = cfg.machineAnalysis === true;
        // Tell the shell up front, so it never offers a "Load analysis" button
        // for a load that is already on its way (feedback 2026-07-27).
        const rndAuto = await willAutoLoadRnd(machineOn, m.deferRnd === true);
        void backgroundLoad(machineOn, m.deferRnd === true);
        reply({
          packVersion: booted.packVersion,
          config: cfg,
          version: engineVersion(booted.wasm),
          bundledOn: booted.home.bundledOn,
          rndAuto,
          fontFaces,
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
        if (m.fn === "configSave") void booted!.home.persistUserData();
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
      default:
        fail(`unknown op ${m.op}`);
    }
  } catch (e) {
    fail(e);
  }
};
