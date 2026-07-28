// The main-thread side of the engine worker (TODO #28): a tiny promise RPC.
// Every engine read the shell does goes through `call`/`static`/`layout`;
// nothing here ever blocks — the reactive read-through cache lives in
// session.svelte.ts, this module only moves messages.

export interface BootInfo {
  packVersion: string;
  config: any;
  version: string;
  bundledOn: boolean;
  /** This session fetches the machine tier by itself — don't offer a button. */
  rndAuto: boolean;
}

export interface WorkerProgress {
  phase: "download" | "prepare" | "open" | "warm";
  fraction?: number;
  detail?: string;
}

export class EngineRpc {
  #w: Worker;
  #next = 1;
  #waiting = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
  /** Boot progress (drives the splash). */
  onProgress: (p: WorkerProgress) => void = () => {};
  /** An authoring write landed (worker persisted it) — re-fetch study data. */
  onAuthored: () => void = () => {};
  /** Stage-2 core data landed (Strong's, margin notes, cross-refs). */
  onCoreReady: () => void = () => {};
  /** The deferred R&D pack finished loading — machine tiers just lit up. */
  onRndReady: () => void = () => {};
  /** R&D pack download progress (0..1) — drives the "load analysis" UI. */
  onRndProgress: (fraction: number) => void = () => {};
  /** Download finished; the engine is now parsing it (seconds on a phone). */
  onRndPreparing: () => void = () => {};

  constructor() {
    this.#w = new Worker(new URL("./engine.worker.ts", import.meta.url), { type: "module" });
    this.#w.onmessage = (ev: MessageEvent) => {
      const m = ev.data;
      if (m.type === "progress") return this.onProgress(m);
      if (m.type === "authored") return this.onAuthored();
      if (m.type === "coreReady") return this.onCoreReady();
      if (m.type === "rndReady") return this.onRndReady();
      if (m.type === "rndProgress") return this.onRndProgress(m.fraction ?? 0);
      if (m.type === "rndPreparing") return this.onRndPreparing();
      const p = this.#waiting.get(m.id);
      if (!p) return;
      this.#waiting.delete(m.id);
      if (m.error != null) p.reject(new Error(m.error));
      else p.resolve(m.result);
    };
  }

  #send(msg: Record<string, unknown>): Promise<any> {
    const id = this.#next++;
    return new Promise((resolve, reject) => {
      this.#waiting.set(id, { resolve, reject });
      this.#w.postMessage({ id, ...msg });
    });
  }

  /** `deferRnd` skips the automatic machine-tier download (phones: the shell
   *  offers an explicit "load analysis" action instead — 2026-07-26). */
  boot(opts: { deferRnd?: boolean } = {}): Promise<BootInfo> {
    const base = new URL(import.meta.env.BASE_URL, location.href).href;
    return this.#send({
      op: "boot",
      base,
      fontUrl: new URL("fonts/EBGaramond.ttf", base).href,
      italicUrl: new URL("fonts/EBGaramond-Italic.ttf", base).href,
      deferRnd: opts.deferRnd === true,
    });
  }
  /** A StudyEngine method by name — reads AND authoring calls alike. */
  call(method: string, ...args: unknown[]): Promise<any> {
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
  /** Per-stage boot timings ([label, ms]) measured on-device. */
  bootTrace(): Promise<[string, number][]> {
    return this.#send({ op: "bootTrace" });
  }
  /** Cost split of the most recent chapter layout, measured on-device. */
  layoutTrace(): Promise<[string, number][]> {
    return this.#send({ op: "layoutTrace" });
  }
  exportUserData(): Promise<[string, Uint8Array][]> {
    return this.#send({ op: "export" });
  }
  freeze(): Promise<void> {
    return this.#send({ op: "freeze" });
  }
  setBundled(on: boolean): Promise<void> {
    return this.#send({ op: "setBundled", on });
  }
}
