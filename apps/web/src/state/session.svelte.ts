// The shell's shared state: engine handle, config (the same wire shape every
// shell round-trips), resolved palette, panes, and navigation. Svelte 5 runes
// make the pieces reactive; the engine itself stays a plain synchronous
// object (single-threaded, like the GTK shell's Rc<RefCell> state).

import type { BootResult } from "../engine/boot";
import { configLoad, configSave, themePalette } from "../engine/StudyEngine";
import type { StudyEngine } from "../engine/StudyEngine";

export interface PaneState {
  book: string;
  chapter: number;
  /** Band verse — persists until this pane next navigates (manifest §Reader). */
  targetVerse: number | null;
  /** One-shot: scroll targetVerse into view once the fresh layout lands. */
  pendingScroll: boolean;
  scrollY: number;
  back: { book: string; chapter: number }[];
  fwd: { book: string; chapter: number }[];
  /** Weave-authoring pin: a word span in this pane (manifest §Weave authoring). */
  pinned: { verse: string; anchor: number; lo: number; hi: number } | null;
}

/** Descriptor of what the study surface is showing (sidebar or sheet). */
export type PanelView =
  | { kind: "wordStudy"; refKey: string; tokenIndex: number }
  | { kind: "codeStudy"; code: string; word: string | null }
  | { kind: "concordance"; code: string }
  | { kind: "renderingConcordance"; code: string; rendering: string }
  | { kind: "threads" }
  | { kind: "thread"; index: number }
  | { kind: "tags" }
  | { kind: "tag"; index: number }
  | { kind: "weaves" }
  | { kind: "suggested" }
  | { kind: "compare"; index: number }
  | { kind: "search" }
  | { kind: "guide" }
  | { kind: "about" }
  | { kind: "notesBrowser" }
  | { kind: "explore" };

export type MapPopup =
  | { kind: "chord" }
  | { kind: "constellation" }
  | { kind: "conceptMap"; code: string };

const HISTORY_CAP = 50; // mirrors core config::HISTORY_CAP

export class Session {
  engine: StudyEngine;
  wasm: BootResult["wasm"];
  home: BootResult["home"];

  config = $state<any>({});
  palette = $state<any>({});
  panes = $state<PaneState[]>([]);
  activePane = $state(0);
  panel = $state<PanelView | null>(null);
  mapPopup = $state<MapPopup | null>(null);
  searchQuery = $state("");
  /** Refreshed after any authoring write (engine reloads → shell re-fetches). */
  studyEpoch = $state(0);
  /** Per-pane verse-number geometry (verse → line box, layout coords), kept
   *  fresh by each ReaderPane for the connectors overlay + canon pins. */
  paneVerseGeom = $state<Map<number, { y: number; h: number }>[]>([]);
  toast = $state<string | null>(null);
  showFirstRun = $state(false);
  showShortcuts = $state(false);
  /** Last-used highlight tone — the default for drag ranges. */
  lastTone = $state<{ name: string; hex: string } | null>(null);
  /** Open context menu (verse actions), positioned at client coords. */
  contextMenu = $state<{ x: number; y: number; refKey: string } | null>(null);
  /** Tag-picker sheet target (refKey), Android TagPickerSheet parity. */
  tagPickFor = $state<string | null>(null);
  /** Tag→weave sheet target (tag ordinal) — the makeweave: verb. */
  tagWeaveFor = $state<number | null>(null);
  /** Memorization surface (hub / review drill / coverage+activity stats). */
  memorize = $state<{ view: "hub" } | { view: "review"; only?: string } | { view: "stats" } | null>(null);
  /** Reading-history sheet (recents from the shared config). */
  showHistory = $state(false);
  /** The one Settings dialog (Android IA). */
  showSettings = $state(false);
  /** Passage navigator (OT/NT → book → chapter → verse grids); pane index. */
  bookNavFor = $state<number | null>(null);
  /** Present mode — fullscreen, high-contrast thread presentation. */
  showPresent = $state(false);
  /** Active text prompt (rendered by PromptDialog); resolves null on cancel. */
  promptReq = $state<{
    title: string;
    initial: string;
    multiline: boolean;
    resolve: (v: string | null) => void;
  } | null>(null);

  /** Ask the user for text — the web twin of the desktops' native prompts. */
  askText(title: string, initial = "", multiline = false): Promise<string | null> {
    return new Promise((resolve) => {
      this.promptReq = { title, initial, multiline, resolve };
    });
  }

  /** Per-tier content gates (bit 0 = human/scholars, bit 1 = machine); the
   *  text and the reader's own data are always on (2026-07-25 product). */
  get gates(): number {
    return (this.config.humanAnalysis !== false ? 1 : 0) | (this.config.machineAnalysis !== false ? 2 : 0);
  }

  #saveTimer: ReturnType<typeof setTimeout> | null = null;
  #systemDark = matchMedia("(prefers-color-scheme: dark)");

  constructor(boot: BootResult) {
    this.engine = boot.engine;
    this.wasm = boot.wasm;
    this.home = boot.home;

    const loaded = configLoad(this.wasm) ?? {};
    this.config = loaded;
    this.showFirstRun = !!loaded.firstRun;

    this.panes = (loaded.openPanes?.length ? loaded.openPanes : [{ book: "John", chapter: 3 }])
      .slice(0, 3)
      .map((p: any) => ({
        book: p.book,
        chapter: p.chapter,
        // Reopen mid-chapter: the saved first-visible verse becomes the
        // scroll target once the first layout lands.
        targetVerse: p.verse && p.verse > 1 ? p.verse : null,
        pendingScroll: !!(p.verse && p.verse > 1),
        scrollY: 0,
        back: [],
        fwd: [],
        pinned: null,
      }));
    this.activePane = Math.min(loaded.activePane ?? 0, this.panes.length - 1);

    this.applyTheme();
    this.#systemDark.addEventListener("change", () => {
      if (this.config.theme === "system") this.applyTheme();
    });

    const prevAuthored = this.engine.onAuthored;
    this.engine.onAuthored = () => {
      prevAuthored();
      this.studyEpoch++;
    };

    // Debug handle for the console (and the repo's headless probes).
    (globalThis as any).__plumbline = this;

    // The web twin of Android's ON_PAUSE persist: flush the session (incl.
    // the scroll verse) when the tab hides or unloads.
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "hidden") this.flushConfig();
    });
    addEventListener("pagehide", () => this.flushConfig());
  }

  /** A restore is pending reload — nothing may persist over it. */
  restoring = false;

  /** Save immediately (tab hide/close) — no debounce. */
  flushConfig(): void {
    if (this.restoring) return;
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.config.openPanes = this.panes.map((p, i) => ({
      book: p.book,
      chapter: p.chapter,
      verse: this.#firstVisibleVerse(i),
    }));
    this.config.activePane = this.activePane;
    this.config.firstRun = undefined;
    configSave(this.wasm, this.config);
    void this.home.persistUserData();
  }

  resolvedTheme(): string {
    const t = this.config.theme ?? "system";
    return t === "system" ? (this.#systemDark.matches ? "dark" : "light") : t;
  }

  applyTheme(): void {
    this.palette = themePalette(this.wasm, this.resolvedTheme()) ?? {};
    const root = document.documentElement;
    for (const [k, v] of Object.entries(this.palette))
      if (typeof v === "string") root.style.setProperty(`--${k}`, v);
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", this.palette.paper ?? "#fcf9f4");
  }

  /** A pane's first visible verse (for cross-session scroll restore). */
  #firstVisibleVerse(idx: number): number | undefined {
    const pane = this.panes[idx];
    const geom = this.paneVerseGeom[idx];
    if (!pane || !geom) return undefined;
    let best: number | undefined;
    let bestY = Infinity;
    for (const [v, g] of geom)
      if (g.y + g.h > pane.scrollY && g.y < bestY) {
        bestY = g.y;
        best = v;
      }
    return best && best > 1 ? best : undefined;
  }

  /** Persist config (debounced) — pane set, zoom, theme, gates, history. */
  saveConfig(): void {
    this.config.openPanes = this.panes.map((p, i) => ({
      book: p.book,
      chapter: p.chapter,
      verse: this.#firstVisibleVerse(i),
    }));
    this.config.activePane = this.activePane;
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => {
      this.config.firstRun = undefined;
      configSave(this.wasm, this.config);
      void this.home.persistUserData();
    }, 300);
  }

  #pushHistory(book: string, chapter: number): void {
    const h: any[] = (this.config.history ??= []);
    const without = h.filter((e) => !(e.book === book && e.chapter === chapter));
    this.config.history = [{ book, chapter }, ...without].slice(0, HISTORY_CAP);
  }

  /** Navigate a pane, recording per-pane back/forward + recents history. */
  navigate(
    paneIdx: number,
    book: string,
    chapter: number,
    verse: number | null = null,
    opts: { history?: boolean } = {},
  ): void {
    const pane = this.panes[paneIdx];
    if (!pane) return;
    const count = this.engine.chapterCount(book) || 1;
    chapter = Math.min(Math.max(chapter, 1), count);
    if (opts.history !== false && (pane.book !== book || pane.chapter !== chapter)) {
      pane.back.push({ book: pane.book, chapter: pane.chapter });
      pane.fwd = [];
    }
    pane.book = book;
    pane.chapter = chapter;
    pane.targetVerse = verse;
    pane.pendingScroll = verse != null;
    pane.scrollY = 0;
    this.activePane = paneIdx;
    this.#pushHistory(book, chapter);
    this.saveConfig();
  }

  historyStep(paneIdx: number, dir: -1 | 1): void {
    const pane = this.panes[paneIdx];
    if (!pane) return;
    const from = { book: pane.book, chapter: pane.chapter };
    const entry = dir < 0 ? pane.back.pop() : pane.fwd.pop();
    if (!entry) return;
    (dir < 0 ? pane.fwd : pane.back).push(from);
    pane.book = entry.book;
    pane.chapter = entry.chapter;
    pane.targetVerse = null;
    pane.pendingScroll = false;
    pane.scrollY = 0;
    this.activePane = paneIdx;
    this.saveConfig();
  }

  /** ±1 chapter with cross-book stepping (canon-adjacent, like the desktops). */
  stepChapter(paneIdx: number, dir: -1 | 1): void {
    const pane = this.panes[paneIdx];
    if (!pane) return;
    const count = this.engine.chapterCount(pane.book) || 1;
    let book = pane.book;
    let chapter = pane.chapter + dir;
    if (chapter < 1 || chapter > count) {
      const toc = this.engine.toc();
      const books: any[] = toc.books;
      const i = books.findIndex((b) => b.id === pane.book);
      const adj = books[i + dir];
      if (!adj) return;
      book = adj.id;
      chapter = dir < 0 ? this.engine.chapterCount(book) || 1 : 1;
    }
    this.navigate(paneIdx, book, chapter);
  }

  addPane(afterIdx: number): void {
    if (this.panes.length >= 3) return;
    const src = this.panes[afterIdx];
    this.panes.splice(afterIdx + 1, 0, {
      book: src.book,
      chapter: src.chapter,
      targetVerse: null,
      pendingScroll: false,
      scrollY: 0,
      back: [],
      fwd: [],
      pinned: null,
    });
    this.activePane = afterIdx + 1;
    this.saveConfig();
  }

  closePane(idx: number): void {
    if (this.panes.length <= 1) return;
    this.panes.splice(idx, 1);
    this.activePane = Math.min(this.activePane, this.panes.length - 1);
    this.saveConfig();
  }

  setZoom(size: number): void {
    this.config.bodySize = Math.min(Math.max(size, 12), 48);
    this.saveConfig();
  }

  showToast(msg: string): void {
    this.toast = msg;
    setTimeout(() => (this.toast = null), 2200);
  }
}

let session: Session | null = null;
export function initSession(boot: BootResult): Session {
  session = new Session(boot);
  return session;
}
export function getSession(): Session {
  if (!session) throw new Error("session not initialized");
  return session;
}
