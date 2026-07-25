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
  /** Verse to scroll to + band once the fresh layout paints (manifest §Reader). */
  targetVerse: number | null;
  scrollY: number;
  back: { book: string; chapter: number }[];
  fwd: { book: string; chapter: number }[];
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
  | { kind: "notesBrowser" };

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
  toast = $state<string | null>(null);
  showFirstRun = $state(false);
  showShortcuts = $state(false);

  get full(): boolean {
    return this.config.studyMode === "full";
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
        targetVerse: null,
        scrollY: 0,
        back: [],
        fwd: [],
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

  /** Persist config (debounced) — pane set, zoom, theme, mode, history. */
  saveConfig(): void {
    this.config.openPanes = this.panes.map((p) => ({ book: p.book, chapter: p.chapter }));
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
      scrollY: 0,
      back: [],
      fwd: [],
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
