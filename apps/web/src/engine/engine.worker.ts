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
import { fetchRndPack, fetchStage2Pack, packFileUrl, setAssetBase } from "./pack";
import { depotHas } from "./depot";
import { PERF } from "./perf";
import { measureFor, readerFont, fontExtent } from "../reader/measure";
import {
  aboutBlocks,
  configLoad,
  configSave,
  engineVersion,
  guideBlocks,
  highlightTones,
  routeLink,
  themePalette,
  type LayoutCfg,
} from "./StudyEngine";

let booted: BootResult | null = null;
/** Cost split of the most recent chapter layout — Settings → diagnostics. */
let lastTurn: [string, number][] = [];

// ── the turn cache ────────────────────────────────────────────────────────────
// Laid-out chapters, keyed by everything the layout depends on. Highlights,
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
async function warmChunked(): Promise<void> {
  for (let step = 0; ; step++) {
    await yieldTask();
    const more = timedChunk(`warm step ${step}`, () => booted!.engine.warmStep(step));
    if (!more) break;
  }
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
    self.postMessage({ type: "coreReady" });
    await warmChunked();
    if (await willAutoLoadRnd(machineOn, deferRnd)) await loadRndChunked();
  } catch {
    /* offline — the Settings toggle or next boot retries */
  }
}

// ── statics ───────────────────────────────────────────────────────────────────

async function loadFonts(fontUrl: string, italicUrl: string): Promise<void> {
  // Workers have their own FontFaceSet; measurement must see the real
  // Garamond metrics or lines would wrap differently than they paint.
  const scope = self as unknown as { fonts?: FontFaceSet };
  if (!scope.fonts) return; // very old engines: fall back to serif metrics
  for (const [url, style] of [
    [fontUrl, "normal"],
    [italicUrl, "italic"],
  ] as const) {
    try {
      const face = new FontFace("EB Garamond", `url(${url})`, { style, weight: "400 700" });
      await face.load();
      scope.fonts.add(face);
    } catch {
      /* platform-serif metrics still beat a dead worker */
    }
  }
}

function statics(): Record<string, (...a: any[]) => any> {
  const w = booted!.wasm;
  return {
    routeLink: (uri: string) => routeLink(w, uri),
    configLoad: () => configLoad(w),
    configSave: (cfg: unknown) => configSave(w, cfg),
    themePalette: (theme: string) => themePalette(w, theme),
    highlightTones: () => highlightTones(w),
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
        const t0 = performance.now();
        await loadFonts(m.fontUrl, m.italicUrl);
        const fontsMs = Math.round(performance.now() - t0);
        booted = await boot((p) => self.postMessage({ type: "progress", ...p }));
        booted.trace.unshift(["worker fonts", fontsMs]);
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
        const cfg = configLoad(booted.wasm) ?? {};
        const machineOn = cfg.machineAnalysis !== false;
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
        });
        break;
      }
      case "call": {
        const e = booted!.engine as unknown as Record<string, (...a: any[]) => unknown>;
        reply(e[m.method](...m.args));
        // The overlay changes the words, so the turn cache has to know.
        if (m.method === "setAkjvOverlay") akjvOn = m.args[0] === true;
        break;
      }
      case "static": {
        reply(statics()[m.fn](...m.args));
        // Config writes land in the in-memory WASI home; mirror them to
        // IndexedDB like any authoring write. Before this, config persisted
        // only when an authoring write happened to follow — a pure reader's
        // first-run choice (and pane layout) never survived a relaunch.
        if (m.fn === "configSave") void booted!.home.persistUserData();
        break;
      }
      case "layout": {
        reply(layoutChapter(m));
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
      case "bootTrace": {
        reply(booted ? [...booted.trace] : []);
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
