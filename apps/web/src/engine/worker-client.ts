// The main-thread side of the engine worker (TODO #28): a tiny promise RPC.
// Every engine read the shell does goes through `call`/`static`/`layout`;
// nothing here ever blocks — the reactive read-through cache lives in
// session.svelte.ts, this module only moves messages.

import { READER_FONT_FILES } from "./fonts.generated";
import type { PackFileTrace } from "./pack";

export interface BootInfo {
  packVersion: string;
  config: any;
  version: string;
  bundledOn: boolean;
  /** This session fetches the machine tier by itself — don't offer a button. */
  rndAuto: boolean;
  /** Reader faces the WORKER actually loaded (expected 2). A failed load is
   *  silent otherwise, and it degrades to platform-serif METRICS while the main
   *  thread paints real Garamond — wrong wrap points, no error. */
  fontFaces: number;
  /** The three theme palettes (light/dark/night), so `applyTheme()` is
   *  synchronous from the first frame without three more round trips. */
  palettes: Record<string, any>;
  /** The table of contents. Handed over here rather than fetched, and then
   *  served back out of [[BOOT_READS]] — see `call`. */
  toc: any;
}

/** Engine reads the BOOT REPLY already carries, by the method name the shell
 *  asks for them under. Answered without a message at all (see `call`).
 *
 *  Only session-IMMUTABLES may be listed here. The TOC qualifies for the same
 *  reason session.svelte.ts pins it: it is derived from a corpus that cannot
 *  change while a session runs, so a value handed over at boot can never go
 *  stale underneath a later read. Anything the engine can rewrite — study data,
 *  config, the reading map — must keep going to the worker. */
const BOOT_READS = ["toc"] as const;

/** One snapshot of everything the engine worker measured about this boot. */
export interface WorkerDiagnostics {
  trace: [string, number][];
  turn: [string, number][];
  /** How long this thread was UNAVAILABLE — the number that separates "the
   *  network was slow" from "the download was queued behind our own arithmetic".
   *  See the stall meter in engine.worker.ts. */
  stall: { totalMs: number; worstMs: number; count: number; hiddenMs: number };
  /** The most expensive ENGINE CALLS this session, worst first. Every request
   *  the shell makes used to be untimed, so a single one could freeze the worker
   *  for half a minute and leave no trace entry to find it by. */
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

/** How long boot may go with NO word from the worker before we call it dead.
 *  Every message the worker sends rearms it, so this is a silence budget for one
 *  stage — not for the whole boot. A cold first visit downloads ~19 MB and then
 *  opens the text, minutes of work on a slow phone, and reports progress the
 *  whole way; being slow must never be mistaken for being gone. */
const BOOT_SILENCE_MS = 60_000;

/** Where boot had got to, in the reader's words — the same stages the splash
 *  names, so the error can say which one went quiet. */
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
  /** Why the worker is gone. Set once; every later call rejects with it rather
   *  than queueing into a corpse. */
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
  /** A warm pass finished, so the indexes a study opened mid-warm had to skip
   *  are available now — anything on screen should re-fetch. */
  onWarmReady: () => void = () => {};
  /** The deferred R&D pack finished loading — machine tiers just lit up. */
  onRndReady: () => void = () => {};
  /** R&D pack download progress (0..1) — drives the "load analysis" UI. */
  onRndProgress: (fraction: number) => void = () => {};
  /** Download finished; the engine is now parsing it (seconds on a phone). */
  onRndPreparing: () => void = () => {};
  /** A save to this device's storage did not land — quota, blocked storage, a
   *  browser that dropped the database. DELIBERATELY NOT the fatal path below: a
   *  failed save is recoverable (the bytes are still in the engine's home, and a
   *  retry or a freed megabyte lands them), while a dead worker never comes back.
   *  Routing this through `#die` would kill a session over a full disk. */
  onPersistFailed: (info: { detail: string; retrying: boolean }) => void = () => {};
  /** A save that had been failing succeeded — the notice can go. */
  onPersistOk: () => void = () => {};
  /** The worker is gone — crashed, out of memory, or silent through a whole boot
   *  stage. Every pending call has already been rejected with this error, which
   *  is how the splash learns about a death during boot (App.svelte awaits
   *  `boot()`); this hook is for saying so AFTER boot, when nothing may be in
   *  flight to carry the news. */
  onFatal: (e: Error) => void = () => {};

  /** `opts` are test seams, and the shell passes none: `workerUrl` points the
   *  client at a worker that dies for real, `bootSilenceMs` shortens the boot
   *  watchdog so a test need not wait a minute for it. */
  constructor(opts: { workerUrl?: string | URL; bootSilenceMs?: number } = {}) {
    this.#silenceMs = opts.bootSilenceMs ?? BOOT_SILENCE_MS;
    // The literal `new Worker(new URL(...))` stays intact: that exact shape is
    // what Vite matches to bundle the worker.
    this.#w = opts.workerUrl
      ? new Worker(opts.workerUrl, { type: "module" })
      : new Worker(new URL("./engine.worker.ts", import.meta.url), { type: "module" });
    // The worker has no `document`, so it cannot tell "I was busy" from "the
    // phone was asleep" — and its stall meter billed the second as the first
    // until it was told (2026-07-28). Send the current state now and on every
    // change; `pagehide` covers the screen going off, which does not always
    // raise visibilitychange first.
    if (typeof document !== "undefined") {
      const send = () => this.#w.postMessage({ op: "visibility", hidden: document.hidden });
      send();
      document.addEventListener("visibilitychange", send);
      addEventListener("pagehide", () => this.#w.postMessage({ op: "visibility", hidden: true }));
      addEventListener("pageshow", () => this.#w.postMessage({ op: "visibility", hidden: false }));
    }
    // A dead worker was the quietest failure in the app: `#waiting` simply never
    // settled, so the splash sat on its last phase — or the reader on a spinner —
    // for as long as they were willing to wait, with no error and nothing to
    // retry. An uncaught throw in the worker, an OOM kill on a phone and a reply
    // that will not structured-clone all land here instead.
    this.#w.onerror = (ev: ErrorEvent) => {
      this.#die(new Error(`The study engine stopped unexpectedly — ${ev.message || "no reason given"}.`));
    };
    this.#w.onmessageerror = () => {
      this.#die(new Error("The study engine sent a reply this browser could not read."));
    };
    this.#w.onmessage = (ev: MessageEvent) => {
      // Any word at all proves the thread is alive and pumping, so it buys boot
      // another silence window.
      if (this.#watchdog) this.#rearm();
      const m = ev.data;
      if (m.type === "progress") {
        this.#stage = m.phase;
        return this.onProgress(m);
      }
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
    // Nothing will ever answer, so say so now instead of adding another promise
    // that can only hang.
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
    // Rejecting the boot promise IS the report while the splash is up — App.svelte
    // renders whatever `boot()` throws, with a Retry.
    for (const p of pending) p.reject(e);
    console.error("[plumbline]", e.message);
    this.onFatal(e);
  }

  /** `deferRnd` skips the automatic machine-tier download (phones: the shell
   *  offers an explicit "load analysis" action instead — 2026-07-26). */
  boot(opts: { deferRnd?: boolean } = {}): Promise<BootInfo> {
    const base = new URL(import.meta.env.BASE_URL, location.href).href;
    // Armed for the whole boot, rearmed by every message, dropped the moment boot
    // settles either way. A boot that never comes back is otherwise indistinguishable
    // from one that is nearly there.
    this.#rearm();
    return this.#send({
      op: "boot",
      base,
      // From the generated module, so the face the worker MEASURES with is by
      // construction the same file public/fonts.css gives the document to PAINT
      // with. Two hardcoded paths could drift, and the symptom would be lines
      // wrapping where they are not drawn.
      fontUrl: new URL(READER_FONT_FILES.normal, base).href,
      italicUrl: new URL(READER_FONT_FILES.italic, base).href,
      deferRnd: opts.deferRnd === true,
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
  /** A StudyEngine method by name — reads AND authoring calls alike.
   *
   *  A [[BOOT_READS]] method rides back on the boot reply, so it is answered from
   *  here with no message and no queue hop — the shell's own read path, minus the
   *  round trip. Only for the no-argument form: the boot reply carries one value,
   *  and an argument means a different question. */
  call(method: string, ...args: unknown[]): Promise<any> {
    if (!this.#dead && !args.length && this.#bootReads.has(method)) {
      return Promise.resolve(this.#bootReads.get(method));
    }
    return this.#send({ op: "call", method, args });
  }
  /** An engine-independent fn (configLoad/Save, themePalette, guide…). */
  static(fn: string, ...args: unknown[]): Promise<any> {
    return this.#send({ op: "static", fn, args });
  }
  /** Chapter layout: the display-list JSON ({items, height}), measured in the
   *  worker with the real Garamond metrics. */
  layout(
    book: string,
    chapter: number,
    o: { font: number; width: number; lineSpacing: number; versePerLine: boolean },
  ): Promise<any> {
    return this.#send({ op: "layout", book, chapter, ...o });
  }
  /** Lay a chapter out into the worker's turn cache without shipping it back
   *  — called at idle for the chapters on either side of the reader. */
  prefetch(
    book: string,
    chapter: number,
    o: { font: number; width: number; lineSpacing: number; versePerLine: boolean },
  ): Promise<void> {
    return this.#send({ op: "prefetch", book, chapter, ...o });
  }
  fontExtent(px: number): Promise<number> {
    return this.#send({ op: "fontExtent", px });
  }
  loadRnd(): Promise<void> {
    return this.#send({ op: "loadRnd" });
  }
  /** Reclaim superseded packs and shell assets. Runs in the WORKER, because that
   *  is where the pin lives, and the pin is the authority on what to keep. */
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
  /** Everything the worker knows about this boot, in ONE round trip — so a
   *  report can't be stitched together from readings taken seconds apart. */
  diagnostics(): Promise<WorkerDiagnostics> {
    return this.#send({ op: "diagnostics" });
  }
  exportUserData(): Promise<[string, Uint8Array][]> {
    return this.#send({ op: "export" });
  }
  /** Persist pending authored data NOW, and resolve once it has landed.
   *
   *  The worker debounces authoring writes by 50 ms, which is invisible until the
   *  reader writes a note and immediately switches apps: a hidden page has its
   *  timers frozen and may be discarded outright, so that callback can simply
   *  never run. Called on `visibilitychange`-hidden and on `pagehide`, and by the
   *  failure notice's "Try again".
   *
   *  HONESTLY LIMITED. On a real unload the page can be torn down before an async
   *  round trip completes, so `visibilitychange` is the one that reliably lands —
   *  which is also the one mobile fires before freezing or discarding a tab.
   *  There is no synchronous way to write IndexedDB from a worker, so this is the
   *  best the platform allows. */
  flush(): Promise<void> {
    return this.#send({ op: "flush" });
  }
  freeze(): Promise<void> {
    return this.#send({ op: "freeze" });
  }
  setBundled(on: boolean): Promise<void> {
    return this.#send({ op: "setBundled", on });
  }
}
