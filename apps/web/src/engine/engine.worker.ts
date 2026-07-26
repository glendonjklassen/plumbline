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
import { fetchRndPack, fetchStage2Pack, setAssetBase } from "./pack";
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
    if (!booted!.manifest.files.some((f) => f.rnd)) return;
    const t0 = performance.now();
    const files = await fetchRndPack(booted!.manifest, (p) =>
      self.postMessage({ type: "rndProgress", fraction: p.fraction }),
    );
    booted!.trace.push(["rnd fetch+gunzip", Math.round(performance.now() - t0)]);
    await yieldTask();
    timedChunk("rnd load (embeddings + morphology)", () => {
      booted!.home.addFiles(files);
      booted!.engine.loadRndData();
    });
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
    if (machineOn && !deferRnd) await loadRndChunked();
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
        void backgroundLoad(cfg.machineAnalysis !== false, m.deferRnd === true);
        reply({
          packVersion: booted.packVersion,
          config: cfg,
          version: engineVersion(booted.wasm),
          bundledOn: booted.home.bundledOn,
        });
        break;
      }
      case "call": {
        const e = booted!.engine as unknown as Record<string, (...a: any[]) => unknown>;
        reply(e[m.method](...m.args));
        break;
      }
      case "static": {
        reply(statics()[m.fn](...m.args));
        break;
      }
      case "layout": {
        const eng = booted!.engine;
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
        const dl = eng.layoutChapter(m.book, m.chapter, cfg);
        firstLayoutServed?.();
        firstLayoutServed = null;
        if (!dl) {
          reply(null);
          break;
        }
        const raw = dl.raw as { items: unknown[]; height: number };
        dl.free();
        reply(raw);
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
