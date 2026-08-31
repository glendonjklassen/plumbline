// The main-thread side of the engine worker: a tiny promise RPC. Every engine read
// the shell does goes through `call`/`static`/`layout`; nothing here blocks. The
// reactive read-through cache lives in session.svelte.ts — this only moves messages.

import { DEFAULT_FONT } from "./fonts.generated";
import type { PackFileTrace } from "./pack";

export interface BootInfo {
  packVersion: string;
  config: any;
  version: string;
  bundledOn: boolean;
  /** This session fetches the machine tier by itself — don't offer a button. */
  rndAuto: boolean;
  /** Reader faces the worker actually loaded — 2 for a family with an italic, 1 for
   *  one without. A failed load is otherwise silent: the worker measures in platform
   *  metrics while the main thread paints the real face, so lines wrap where they are
   *  not drawn. */
  fontFaces: number;
  /** Every theme's palette, keyed by token, so `applyTheme()` is synchronous
   *  from the first frame without a round trip per theme. */
  palettes: Record<string, any>;
  /** The table of contents; served back out of [[BOOT_READS]] — see `call`. */
  toc: any;
  /** Every string the shell paints, in the language the core resolved from the
   *  reader's setting and the device's locale. Here for the palettes' reason: no
   *  screen can be painted without it, so it must not be one more queue hop. */
  i18n: { lang: string; strings: Record<string, string>; languages: { code: string; endonym: string }[] };
}

/** Engine reads the boot reply already carries, by the method name the shell asks
 *  for them under; answered without a message at all (see `call`).
 *
 *  Only session-immutables may be listed here: the TOC is corpus-derived and a corpus
 *  cannot change while a session runs. Anything the engine can rewrite — study data,
 *  config, the reading map — must keep going to the worker. */
const BOOT_READS = ["toc"] as const;

/** One snapshot of everything the engine worker measured about this boot. */
export interface WorkerDiagnostics {
  trace: [string, number][];
  turn: [string, number][];
  /** How long the worker thread was unavailable — separates "the network was slow"
   *  from "the download was queued behind our own arithmetic". See engine.worker.ts. */
  stall: { totalMs: number; worstMs: number; count: number; hiddenMs: number };
  /** The most expensive engine calls this session, worst first — otherwise an untimed
   *  request can freeze the worker and leave no trace entry to find it by. */
  slowCalls: [string, number][];
  packFiles: PackFileTrace[];
  packVersion: string | null;
  /** Whether stage 1 came entirely off this device, with no request at all. */
  fromPin: boolean | null;
}

export interface WorkerProgress {
  phase: "download" | "prepare" | "open" | "warm";
  fraction?: number;
  detail?: string;
}

/** How long boot may go with no word from the worker before we call it dead. Every
 *  message rearms it, so this is a silence budget for one stage, not for the whole
 *  boot: a cold first visit is minutes of work on a slow phone and reports progress
 *  the whole way. */
const BOOT_SILENCE_MS = 60_000;

/** Where boot had got to, in the reader's words, so the error can name it. */
const STAGE_WORDS: Record<WorkerProgress["phase"], string> = {
  download: "fetching the scripture data",
  prepare: "preparing the study engine",
  open: "opening the text",
  warm: "building the analytics",
};

export class EngineRpc {
  #w: Worker;
  #next = 1;
  #waiting = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
  /** Why the worker is gone. Set once; later calls reject with it rather than
   *  queueing into a dead thread. */
  #dead: Error | null = null;
  #watchdog: ReturnType<typeof setTimeout> | null = null;
  #silenceMs: number;
  /** The last boot stage the worker reported, for the watchdog's message. */
  #stage: WorkerProgress["phase"] | null = null;
  /** [[BOOT_READS]] as the boot reply delivered them, keyed by method name. */
  #bootReads = new Map<string, unknown>();
  /** Boot progress (drives the splash). */
  onProgress: (p: WorkerProgress) => void = () => {};
  /** An authoring write landed (worker persisted it) — re-fetch study data. */
  onAuthored: () => void = () => {};
  /** A dwell report landed — the reading map changed and nothing else did. */
  onReadingWrote: () => void = () => {};
  /** Stage-2 core data landed (Strong's, margin notes, cross-refs). */
  onCoreReady: () => void = () => {};
  /** A warm pass finished: indexes a study opened mid-warm had to skip exist now, so
   *  anything on screen should re-fetch. */
  onWarmReady: () => void = () => {};
  /** The deferred R&D pack finished loading — machine tiers just lit up. */
  onRndReady: () => void = () => {};
  /** R&D pack download progress (0..1) — drives the "load analysis" UI. */
  onRndProgress: (fraction: number) => void = () => {};
  /** How far the language-pack download has got, 0..1. */
  onLangPackProgress: (fraction: number) => void = () => {};
  /** Download progress for a language a PANE asked for (0..1). */
  onPaneLangProgress: (code: string, fraction: number) => void = () => {};
  /** Download finished; the engine is now parsing it (seconds on a phone). */
  onRndPreparing: () => void = () => {};
  /** A save to this device's storage did not land — quota, blocked storage, a browser
   *  that dropped the database. Deliberately not the fatal path below: a failed save is
   *  recoverable, so routing it through `#die` would kill a session over a full disk. */
  onPersistFailed: (info: { detail: string; retrying: boolean }) => void = () => {};
  /** A save that had been failing succeeded — the notice can go. */
  onPersistOk: () => void = () => {};
  /** The worker is gone — crashed, out of memory, or silent through a whole boot
   *  stage. Pending calls are already rejected with this error, which is how the splash
   *  learns of a death during boot; this hook is for saying so after boot, when nothing
   *  may be in flight to carry the news. */
  onFatal: (e: Error) => void = () => {};

  /** `opts` are test seams, and the shell passes none: `workerUrl` points the
   *  client at a worker that dies for real, `bootSilenceMs` shortens the boot
   *  watchdog so a test need not wait a minute for it. */
  constructor(opts: { workerUrl?: string | URL; bootSilenceMs?: number } = {}) {
    this.#silenceMs = opts.bootSilenceMs ?? BOOT_SILENCE_MS;
    // The literal `new Worker(new URL(...))` stays intact — that exact shape is what
    // Vite matches to bundle the worker.
    this.#w = opts.workerUrl
      ? new Worker(opts.workerUrl, { type: "module" })
      : new Worker(new URL("./engine.worker.ts", import.meta.url), { type: "module" });
    // The worker has no `document`, so its stall meter cannot tell "I was busy" from
    // "the phone was asleep". Send the state now and on every change; `pagehide` covers
    // the screen going off, which does not always raise visibilitychange first.
    if (typeof document !== "undefined") {
      const send = () => this.#w.postMessage({ op: "visibility", hidden: document.hidden });
      send();
      document.addEventListener("visibilitychange", send);
      addEventListener("pagehide", () => this.#w.postMessage({ op: "visibility", hidden: true }));
      addEventListener("pageshow", () => this.#w.postMessage({ op: "visibility", hidden: false }));
    }
    // Without this a dead worker is the quietest failure in the app: `#waiting` never
    // settles and the splash sits on its last phase with no error and nothing to retry.
    // An uncaught throw, an OOM kill on a phone and a reply that will not
    // structured-clone all land here.
    this.#w.onerror = (ev: ErrorEvent) => {
      this.#die(new Error(`The study engine stopped unexpectedly — ${ev.message || "no reason given"}.`));
    };
    this.#w.onmessageerror = () => {
      this.#die(new Error("The study engine sent a reply this browser could not read."));
    };
    this.#w.onmessage = (ev: MessageEvent) => {
      // Any word proves the thread is alive and pumping: another silence window.
      if (this.#watchdog) this.#rearm();
      const m = ev.data;
      if (m.type === "progress") {
        this.#stage = m.phase;
        return this.onProgress(m);
      }
      if (m.type === "langPackProgress") return this.onLangPackProgress(m.fraction ?? 0);
      if (m.type === "paneLangProgress") return this.onPaneLangProgress(m.code ?? "", m.fraction ?? 0);
      if (m.type === "authored") return this.onAuthored();
      if (m.type === "readingWrote") return this.onReadingWrote();
      if (m.type === "coreReady") return this.onCoreReady();
      if (m.type === "warmReady") return this.onWarmReady();
      if (m.type === "rndReady") return this.onRndReady();
      if (m.type === "rndProgress") return this.onRndProgress(m.fraction ?? 0);
      if (m.type === "rndPreparing") return this.onRndPreparing();
      if (m.type === "persistFailed")
        return this.onPersistFailed({ detail: String(m.detail ?? ""), retrying: m.retrying === true });
      if (m.type === "persistOk") return this.onPersistOk();
      const p = this.#waiting.get(m.id);
      if (!p) return;
      this.#waiting.delete(m.id);
      if (m.error != null) p.reject(new Error(m.error));
      else p.resolve(m.result);
    };
  }

  #send(msg: Record<string, unknown>): Promise<any> {
    // Nothing will ever answer, so say so now rather than add a promise that can
    // only hang.
    if (this.#dead) return Promise.reject(this.#dead);
    const id = this.#next++;
    return new Promise((resolve, reject) => {
      this.#waiting.set(id, { resolve, reject });
      this.#w.postMessage({ id, ...msg });
    });
  }

  /** (Re)start the boot silence timer. */
  #rearm(): void {
    if (this.#silenceMs <= 0) return;
    if (this.#watchdog) clearTimeout(this.#watchdog);
    this.#watchdog = setTimeout(() => {
      this.#watchdog = null;
      const where = this.#stage ? ` It got as far as ${STAGE_WORDS[this.#stage]}.` : "";
      this.#die(
        new Error(
          `The study engine went quiet for ${Math.round(this.#silenceMs / 1000)}s ` +
            `and never finished starting.${where}`,
        ),
      );
    }, this.#silenceMs);
  }

  #disarm(): void {
    if (this.#watchdog) clearTimeout(this.#watchdog);
    this.#watchdog = null;
  }

  /** The worker is gone: settle every pending call and refuse new ones. */
  #die(e: Error): void {
    if (this.#dead) return;
    this.#dead = e;
    this.#disarm();
    const pending = [...this.#waiting.values()];
    this.#waiting.clear();
    // Rejecting the boot promise is the report while the splash is up: App.svelte
    // renders whatever `boot()` throws, with a Retry.
    for (const p of pending) p.reject(e);
    console.error("[plumbline]", e.message);
    this.onFatal(e);
  }

  /** `deferRnd` skips the automatic machine-tier download (phones offer an explicit
   *  "load analysis" action instead).
   *
   *  `lang` is what this device resolved last time (`localStorage`); `locale` is the
   *  device's language, which decides only when the reader has not chosen one. Both
   *  are needed and neither is enough: `lang` is empty on a first visit, so a corpus
   *  chosen from it alone opens the KJV for a phone we are about to speak Arabic to.
   *  See `corpusRoleFor`. */
  boot(
    opts: { deferRnd?: boolean; locale?: string; lang?: string; textFont?: string } = {},
  ): Promise<BootInfo> {
    const base = new URL(import.meta.env.BASE_URL, location.href).href;
    // Armed for the whole boot, rearmed by every message, dropped when boot settles
    // either way — a boot that never comes back otherwise looks like a slow one.
    this.#rearm();
    return this.#send({
      op: "boot",
      base,
      // The face token only: the worker resolves it through the same generated module
      // public/fonts.css was written from, so it measures with the file the document
      // paints with. It rides the boot message because the worker must have the face
      // before the first layout, and the first layout is answered the moment boot
      // replies.
      textFont: opts.textFont ?? DEFAULT_FONT,
      deferRnd: opts.deferRnd === true,
      locale: opts.locale ?? "",
      // From `localStorage`, which only this thread can read. Stage 1 uses it to
      // inflate one corpus instead of every one; see `corpusRoleFor`.
      lang: opts.lang ?? "",
    })
      .then((info: BootInfo) => {
        for (const m of BOOT_READS) {
          const v = info?.[m];
          if (v != null) this.#bootReads.set(m, v);
        }
        return info;
      })
      .finally(() => this.#disarm());
  }
  /** A StudyEngine method by name — reads and authoring calls alike. A [[BOOT_READS]]
   *  method rides back on the boot reply and is answered here with no message and no
   *  queue hop; only in its no-argument form, since the reply carries one value. */
  call(method: string, ...args: unknown[]): Promise<any> {
    if (!this.#dead && !args.length && this.#bootReads.has(method)) {
      return Promise.resolve(this.#bootReads.get(method));
    }
    return this.#send({ op: "call", method, args });
  }
  /** The same read against another language's text — a pane reading German beside an
   *  English one. Falsy `lang` is the reader's own text, so callers need no branch.
   *  Reads only: authoring is the primary engine's alone. */
  callIn(lang: string | null | undefined, method: string, ...args: unknown[]): Promise<any> {
    if (!lang) return this.call(method, ...args);
    return this.#send({ op: "callIn", lang, method, args });
  }
  /** An engine-independent fn (configLoad/Save, themePalette, guide…). */
  static(fn: string, ...args: unknown[]): Promise<any> {
    return this.#send({ op: "static", fn, args });
  }
  /** Chapter layout: the display-list JSON ({items, height}), measured in the worker
   *  against the real face metrics. */
  layout(
    book: string,
    chapter: number,
    o: {
      font: number;
      width: number;
      lineSpacing: number;
      versePerLine: boolean;
      verseNumbers: boolean;
      /** The pane's text language; absent = the reader's own. */
      lang?: string | null;
    },
  ): Promise<any> {
    return this.#send({ op: "layout", book, chapter, ...o });
  }
  /** Lay a chapter out into the worker's turn cache without shipping it back — called
   *  at idle for the chapters on either side of the reader. */
  prefetch(
    book: string,
    chapter: number,
    o: {
      font: number;
      width: number;
      lineSpacing: number;
      versePerLine: boolean;
      verseNumbers: boolean;
      lang?: string | null;
    },
  ): Promise<void> {
    return this.#send({ op: "prefetch", book, chapter, ...o });
  }
  fontExtent(px: number): Promise<number> {
    return this.#send({ op: "fontExtent", px });
  }

  /** Point the worker at a scripture face and wait for it to hold the real metrics;
   *  resolves to the number of faces it now has. The caller must not relayout until
   *  this settles, or the layout is measured in the fallback and painted in the
   *  chosen face. */
  setTextFont(token: string): Promise<number> {
    const base = new URL(import.meta.env.BASE_URL, location.href).href;
    return this.#send({ op: "setTextFont", base, token });
  }
  loadRnd(): Promise<void> {
    return this.#send({ op: "loadRnd" });
  }
  /** Reclaim superseded packs and shell assets. Runs in the worker, where the pin
   *  lives — the pin is the authority on what to keep. */
  prune(shell: string[]): Promise<number> {
    return this.#send({ op: "prune", shell });
  }
  /** Per-stage boot timings ([label, ms]) measured on-device. */
  bootTrace(): Promise<[string, number][]> {
    return this.#send({ op: "bootTrace" });
  }
  /** Cost split of the most recent chapter layout, measured on-device. */
  layoutTrace(): Promise<[string, number][]> {
    return this.#send({ op: "layoutTrace" });
  }
  /** Everything the worker knows about this boot, in one round trip, so a report is
   *  not stitched together from readings taken seconds apart. */
  diagnostics(): Promise<WorkerDiagnostics> {
    return this.#send({ op: "diagnostics" });
  }
  exportUserData(): Promise<[string, Uint8Array][]> {
    return this.#send({ op: "export" });
  }
  /** Persist pending authored data now, resolving once it has landed. The worker
   *  debounces authoring writes by 50 ms, and a hidden page has its timers frozen and
   *  may be discarded outright, so that callback can never run. Called on
   *  `visibilitychange`-hidden, on `pagehide`, and by the failure notice's "Try again".
   *
   *  Limited by the platform: on a real unload the page can be torn down before an
   *  async round trip completes, so `visibilitychange` is the one that reliably lands.
   *  There is no synchronous way to write IndexedDB from a worker. */
  flush(): Promise<void> {
    return this.#send({ op: "flush" });
  }
  freeze(): Promise<void> {
    return this.#send({ op: "freeze" });
  }
  setBundled(on: boolean): Promise<void> {
    return this.#send({ op: "setBundled", on });
  }
  /** Whether this pack offers the suggested weaves, whether this device already
   *  has them, and what the download costs. */
  suggestedState(): Promise<{ available: boolean; installed: boolean; gzBytes: number }> {
    return this.#send({ op: "suggestedState" });
  }
  /** Download and install the suggested-weave set; resolves with how many files
   *  were written. */
  installSuggested(): Promise<number> {
    return this.#send({ op: "installSuggested" });
  }

  /** Free the engines for languages no pane reads any more; answers how many were
   *  released. */
  releaseLangs(keep: string[]): Promise<number> {
    return this.#send({ op: "releaseLangs", keep });
  }
  /** The wasm instance's heap size in bytes — what an extra open corpus costs,
   *  measured rather than guessed. */
  wasmMemoryBytes(): Promise<number> {
    return this.#send({ op: "wasmMemoryBytes" });
  }
  langPackState(code: string): Promise<{ available: boolean; installed: boolean; gzBytes: number }> {
    return this.#send({ op: "langPackState", code });
  }

  /** Make a language readable in a pane: download its text if this device has not got
   *  it, then open an engine on it. No reload — the pane beside it keeps reading.
   *  Progress arrives as `onPaneLangProgress`. */
  openPaneLang(code: string): Promise<{ ready: boolean }> {
    return this.#send({ op: "openPaneLang", code });
  }
  /** Download and store a language's corpus. The caller reloads afterwards: the
   *  corpus is chosen when the engine opens, so nothing changes until it does. */
  installLangPack(code: string): Promise<boolean> {
    return this.#send({ op: "installLangPack", code });
  }
}
