// The engine worker (TODO #28): the WHOLE engine life runs here — pack fetch,
// IndexedDB home, wasm instantiate, engine open, warm, the deferred R&D load,
// authoring writes and their persistence. The main thread never blocks on an
// engine call again; it speaks the tiny RPC below and paints.
//
// Protocol (structured-clone JSON):
//   in:  { id, op: "boot", base, fontUrl }          → boots; progress streams
//   in:  { id, op: "call", method, args }           → StudyEngine[method](...args)
//   in:  { id, op: "static", fn, args }             → engine-independent fns
//   in:  { id, op: "layout", book, chapter, cfg, font } → display-list JSON
//   in:  { id, op: "loadRnd" }                      → deferred pack + re-warm
//   in:  { id, op: "export" } / { id, op: "freeze" } / { id, op: "setBundled", on }
//   out: { id, result } | { id, error }
//   out: { type: "progress", phase, fraction?, detail? }   (during boot)
//   out: { type: "authored" }                       (any authoring write landed)
//
// Layout measure runs HERE over an OffscreenCanvas (measure.ts adapts), so
// EB Garamond must be loaded into self.fonts before the first layout — the
// boot message carries the resolved font URL.

import { boot, loadRndPack, type BootResult } from "./boot";
import { setAssetBase } from "./pack";
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
        await loadFonts(m.fontUrl, m.italicUrl);
        booted = await boot((p) => self.postMessage({ type: "progress", ...p }));
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
        if (cfg.machineAnalysis !== false) {
          // Warm AFTER handing the UI over (below) — the reader paints those
          // seconds earlier, and a first analytics tap simply waits for the
          // OnceLock build here, off-thread. Then the deferred R&D pack, the
          // same way.
          setTimeout(() => {
            booted!.engine.warmIndexes();
            loadRndPack(booted!)
              .then(() => self.postMessage({ type: "rndReady" }))
              .catch(() => {
                /* offline — the Settings toggle or next boot retries */
              });
          }, 50);
        }
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
        await loadRndPack(booted!);
        reply(null);
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
