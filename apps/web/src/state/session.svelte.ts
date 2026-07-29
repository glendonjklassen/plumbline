// The shell's shared state — now over the ENGINE WORKER (TODO #28): the
// engine lives in engine.worker.ts and never blocks this thread. Components
// keep their synchronous `$derived` graphs by reading through `q()`, a
// reactive read-through cache: a miss returns null and fires the async fill;
// the reply bumps `cacheEpoch`, deriveds re-run, and the value is there.
// Panels already tolerate a null block list, so "loading" is just a frame or
// two of empty — never a frozen UI.

import { precacheShell } from "../engine/precache";
import { updateAvailable } from "../engine/update";
import { EngineRpc, type BootInfo } from "../engine/worker-client";
import { cleanChurch, PWA_URL, shareUrl, type Church } from "../shell/church";

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
  /** Deepest verse the reader has scrolled to in THIS chapter — the reading
   *  map's high-water mark (core::reading). Monotonic within a chapter: reading
   *  back up does not un-read anything. Reset on navigate. */
  reached?: number;
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
  /** The RPC to the engine worker — the only line to the engine. */
  rpc: EngineRpc;
  /** e2e/console compatibility: `engine.method(...)` → a Promise via RPC. */
  engine: Record<string, (...args: unknown[]) => Promise<any>>;

  config = $state<any>({});
  palette = $state<any>({});
  panes = $state<PaneState[]>([]);
  activePane = $state(0);
  panel = $state<PanelView | null>(null);
  mapPopup = $state<MapPopup | null>(null);
  searchQuery = $state("");
  /** Refreshed after any authoring write (worker reload → shell re-fetch). */
  studyEpoch = $state(0);
  /** Bumped whenever an async cache fill lands — deriveds re-read q(). */
  cacheEpoch = $state(0);
  /** Per-pane verse-number geometry (verse → line box, layout coords), kept
   *  fresh by each ReaderPane for the connectors overlay + canon pins. */
  paneVerseGeom = $state<Map<number, { y: number; h: number }>[]>([]);
  toast = $state<string | null>(null);
  showFirstRun = $state(false);
  showShortcuts = $state(false);
  /** Open context menu (verse actions), positioned at client coords. */
  contextMenu = $state<{ x: number; y: number; refKey: string } | null>(null);
  /** Tag-picker sheet target (refKey), Android TagPickerSheet parity. */
  tagPickFor = $state<string | null>(null);
  /** Thread-picker sheet target (refKey), Android ThreadPickerSheet parity. */
  threadPickFor = $state<string | null>(null);
  /** Tag→weave sheet target (tag ordinal) — the makeweave: verb. */
  tagWeaveFor = $state<number | null>(null);
  /** Passage-memorize picker: the start verse, whose chapter's later verse
   *  numbers the reader taps to set the end (§Memorization). */
  memorizePassageFrom = $state<string | null>(null);
  /** Memorization surface (hub / review drill / coverage+activity stats). */
  memorize = $state<{ view: "hub" } | { view: "review"; only?: string } | { view: "stats" } | null>(null);
  /** Reading-history sheet (recents from the shared config). */
  showHistory = $state(false);
  /** The one Settings dialog (Android IA). */
  showSettings = $state(false);
  /**
   * Which DESTINATION is on screen — the web twin of Android's `Dest`.
   *
   * A destination replaces the reader; it does not hover over it. Explore used to
   * be a study-panel kind (so, on a phone, a bottom sheet over the verse you were
   * reading) and Memorize a centred modal. Both are screens now (2026-07-29).
   * `"read"` is the absence of a destination rather than one of its own.
   */
  screen = $state<"read" | "explore" | "memorize">("read");

  /** Back to the text from anywhere — what every screen's ‹ does. */
  goRead(): void {
    this.dismissTransient();
    this.screen = "read";
  }

  /** Passage navigator (OT/NT → book → chapter → verse grids); pane index. */
  bookNavFor = $state<number | null>(null);
  /** "Mark chapter read…" dialog target — the by-hand date for a paper-Bible
   *  read (core::reading::mark_read). */
  markReadFor = $state<{ book: string; chapter: number } | null>(null);
  /** Present mode — fullscreen, high-contrast thread presentation. */
  showPresent = $state(false);
  /** Present opens straight into this thread when set (first-run "Sharing
   *  the gospel" → the Romans Road); consumed on open. */
  presentThreadName = $state<string | null>(null);
  /** Whether the bundled stock set is on (worker home state, mirrored). */
  bundledOn = $state(true);
  /** The machine-tier pack's lifecycle this session. Phones boot with the
   *  auto-download deferred ("off") and load it behind an explicit action;
   *  desktops auto-load ("loading" → "ready") when the machine tier is on. */
  rndState = $state<"off" | "loading" | "ready">("off");
  /** Machine-tier download progress (0..1) while rndState === "loading". */
  rndProgress = $state(0);
  /** Downloaded, now being parsed into the engine — the phase where progress
   *  can't advance and the UI must say so rather than sit at a number. */
  rndPreparing = $state(false);
  /** Whether this boot deferred the machine-tier auto-download (phones) —
   *  gates the explicit "load analysis" offers so desktops, where the pack
   *  is already on its way, don't flash a pointless button. */
  rndDeferred = false;
  /** Per-stage boot timings ([label, ms]) from the engine worker — the
   *  on-device numbers behind Settings → boot diagnostics. */
  bootTrace = $state<[string, number][]>([]);
  /** Cost split of the last chapter turn (same diagnostics section). */
  turnTrace = $state<[string, number][]>([]);
  /** The church whose shared link opened this session, if it was opened from
   *  one — the welcome names them. Not persisted: it describes THIS arrival,
   *  while `config.church` is the reader's own (see shell/church.ts). */
  sharedByChurch = $state<Church | null>(null);
  /** Narrow (phone-shaped) viewport — mirrors the CSS 700px breakpoint. ONE
   *  pane only there: splits are hidden and links never open "beside"
   *  (feedback 2026-07-26 — two panes on a phone is jank). */
  narrow = $state(matchMedia("(max-width: 700px)").matches);
  /** Active text prompt (rendered by PromptDialog); resolves null on cancel. */
  promptReq = $state<{
    title: string;
    initial: string;
    multiline: boolean;
    resolve: (v: string | null) => void;
  } | null>(null);

  /**
   * Close every transient surface — dialogs, sheets, pickers, popups, the study
   * panel. What a destination tap does before it opens anything.
   *
   * This lives HERE, next to the declarations, and not in the shell, because the
   * shell's version was a hand-kept list of five and there are thirteen of these
   * (2026-07-29: "when I go into notes, then try to navigate to read, it does not
   * let me — this is likely a class of bug", and it was). Every surface added
   * since that list was written inherited the same trap: its modal covered the
   * screen and the Read tab could not dismiss it. Adding a `$state` surface above
   * without adding it here is now a one-file omission a reviewer can see, rather
   * than a bug in a different file that nobody thinks to look at.
   *
   * `promptReq` is RESOLVED, not just nulled: `askText` handed a promise to a
   * caller that is still awaiting it, and dropping the request on the floor would
   * leave that caller hanging forever.
   */
  dismissTransient(): void {
    this.screen = "read";
    this.cancelPrompt();
    // A pending confirmation resolves NO. Anything else would leave a caller
    // awaiting a promise that never settles, or worse, destroy something because
    // the reader navigated away.
    this.cancelConfirm();
    this.panel = null;
    this.mapPopup = null;
    this.memorize = null;
    this.contextMenu = null;
    this.tagPickFor = null;
    this.threadPickFor = null;
    this.tagWeaveFor = null;
    this.memorizePassageFrom = null;
    this.markReadFor = null;
    this.bookNavFor = null;
    this.reopenIntro = null;
    this.showHistory = false;
    this.showSettings = false;
    this.showShortcuts = false;
    this.showPresent = false;
    // NOT showFirstRun: a reader who has never chosen a path must not be able to
    // tab past the question. It closes by being answered.
  }

  /** An open confirmation. One mechanism for every destructive action, so that
   *  "does this ask first?" is answered in one place instead of per button —
   *  deleting a memorize card asked nothing at all while deleting a thread had its
   *  own bespoke inline prompt (2026-07-29). */
  confirmReq = $state<{
    title: string;
    body: string;
    verb: string;
    resolve: (ok: boolean) => void;
  } | null>(null);

  /**
   * Ask before destroying something. Resolves true only if the reader says so.
   *
   * `verb` is the button's label, and it should name the ACT rather than say
   * "OK" — "Delete thread", "Remove card". A reader who half-read the sentence
   * still knows what the button does.
   */
  askConfirm(title: string, body: string, verb = "Delete"): Promise<boolean> {
    return new Promise((resolve) => {
      this.confirmReq = { title, body, verb, resolve };
    });
  }

  /** Dismiss an open confirmation as a "no" — the promise must always settle. */
  cancelConfirm(): void {
    this.confirmReq?.resolve(false);
    this.confirmReq = null;
  }

  /** Dismiss an open text prompt, resolving its promise so the caller that is
   *  awaiting it does not hang. */
  cancelPrompt(): void {
    this.promptReq?.resolve(null);
    this.promptReq = null;
  }

  /** Ask the user for text — the web twin of the desktops' native prompts. */
  askText(title: string, initial = "", multiline = false): Promise<string | null> {
    return new Promise((resolve) => {
      this.promptReq = { title, initial, multiline, resolve };
    });
  }

  /** Per-tier content gates (bit 0 = human/scholars, bit 1 = machine); the
   *  text and the reader's own data are always on (2026-07-25 product). */
  get gates(): number {
    // `=== true`, not `!== false`: the tiers are opt-in, so absent means off.
    return (this.config.humanAnalysis === true ? 1 : 0) | (this.config.machineAnalysis === true ? 2 : 0);
  }

  #saveTimer: ReturnType<typeof setTimeout> | null = null;
  #systemDark = matchMedia("(prefers-color-scheme: dark)");
  /** Palettes per theme, prefetched at boot (applyTheme stays synchronous). */
  #palettes: Record<string, any>;

  // ── the read-through cache ──────────────────────────────────────────────────

  #cache = new Map<string, any>();
  #pending = new Set<string>();

  /** Read an engine method through the cache: the cached value, or null while
   *  the worker answers (the reply bumps cacheEpoch → callers re-run). */
  q(method: string, ...args: unknown[]): any {
    void this.cacheEpoch; // register the dependency
    const key = `${method}\0${JSON.stringify(args)}`;
    if (this.#cache.has(key)) return this.#cache.get(key);
    if (!this.#pending.has(key)) {
      this.#pending.add(key);
      this.rpc
        .call(method, ...args)
        .then((v) => {
          this.#cache.set(key, v);
          this.#pending.delete(key);
          this.cacheEpoch++;
        })
        .catch((e) => {
          this.#pending.delete(key);
          console.warn(`[plumbline] ${method} failed:`, e);
        });
    }
    return null;
  }

  /** An engine-independent static fn through the same cache (guide/about…). */
  qs(fn: string, ...args: unknown[]): any {
    void this.cacheEpoch;
    const key = `static:${fn} ${JSON.stringify(args)}`;
    if (this.#cache.has(key)) return this.#cache.get(key);
    if (!this.#pending.has(key)) {
      this.#pending.add(key);
      this.rpc
        .static(fn, ...args)
        .then((v) => {
          this.#cache.set(key, v);
          this.#pending.delete(key);
          this.cacheEpoch++;
        })
        .catch(() => this.#pending.delete(key));
    }
    return null;
  }

  /** Await an engine read AND leave it in the cache (prefetch / imperative). */
  async fetchQ(method: string, ...args: unknown[]): Promise<any> {
    const key = `${method}\0${JSON.stringify(args)}`;
    if (this.#cache.has(key)) return this.#cache.get(key);
    const v = await this.rpc.call(method, ...args);
    this.#cache.set(key, v);
    this.cacheEpoch++;
    return v;
  }

  /** An authoring call: resolves to null on success, else the error string
   *  (the worker's `authored` event refreshes study data by itself). */
  author(method: string, ...args: unknown[]): Promise<string | null> {
    return this.rpc.call(method, ...args).then(
      (err) => err,
      (e) => (e instanceof Error ? e.message : String(e)),
    );
  }

  /** Drop cached study reads (authoring landed / R&D pack arrived). The
   *  corpus-derived immutables (toc, canon shape, statics) survive — wiping
   *  them made navigation clamp against an empty TOC mid-refill. */
  invalidate(): void {
    for (const key of [...this.#cache.keys()])
      if (!key.startsWith("toc ") && !key.startsWith("canonSegments ") && !key.startsWith("static:"))
        this.#cache.delete(key);
    this.cacheEpoch++;
  }

  /** Drop cached reads for NAMED engine methods only.
   *
   *  `invalidate()` wipes everything but the immutables, which is right after an
   *  authoring write and wrong on a timer: the reading map reports dwell every 30
   *  seconds while somebody reads, and throwing away their open word study and
   *  every thread/tag read along with it would make the reader pay for a
   *  bookkeeping tick they never asked for. */
  invalidateOnly(...methods: string[]): void {
    for (const key of [...this.#cache.keys()])
      if (methods.some((m) => key.startsWith(`${m}\0`))) this.#cache.delete(key);
    this.cacheEpoch++;
  }

  /** The reader's home church — what their own shared links carry. */
  get church(): Church {
    return cleanChurch(this.config.church);
  }

  /** THE link this reader hands over, wherever they share from — the app plus
   *  their church. Every share surface reads this, so Present and the header
   *  can't drift apart (they did: Present shared a bare link). */
  get shareLink(): string {
    return shareUrl(PWA_URL, this.church);
  }

  /** The link PRESENT hands over. Same as [[shareLink]] plus, by default, a
   *  marker that opens the recipient's welcome on the new-believer path —
   *  Present is the screen you show someone face to face. Settings can turn
   *  that off; the ordinary Share never carries it. */
  get presentShareLink(): string {
    return shareUrl(PWA_URL, this.church, {
      startAsNewBeliever: this.config.presentSharesAsNew !== false,
    });
  }

  /** This session was opened from a link that said "for a new believer". */
  startAsNewBeliever = false;

  /** Re-showing the welcome a reader was given, from the top bar. Holds which
   *  page to show; null when closed. The first-run flow is separate
   *  (`showFirstRun`) — this one changes no settings, it just reads. */
  reopenIntro = $state<"new" | "curious" | null>(null);
  /** Which welcome this reader saw, if any (persisted, so the button is there
   *  on every launch — not only the one where they chose it). */
  get intro(): "new" | "curious" | null {
    const v = this.config.intro;
    return v === "new" || v === "curious" ? v : null;
  }
  setChurch(c: Church): void {
    this.config.church = cleanChurch(c);
    this.saveConfig();
  }

  /** A book's display name ("Hebrews", not the OSIS id "Heb"). Everything the
   *  reader sees goes through this — refKeys and layout calls keep the id,
   *  which is the frozen wire form. Falls back to the id if the TOC hasn't
   *  landed (it is prefetched at boot, so that is a first-frame edge only). */
  bookName(book: string): string {
    return this.q("toc")?.books?.find((b: any) => b.id === book)?.name ?? book;
  }

  /** Chapter count from the cached TOC (prefetched at boot); 0 = unknown. */
  chapterCount(book: string): number {
    const b = this.q("toc")?.books?.find((x: any) => x.id === book);
    return Number(b?.chapters ?? 0) || 0;
  }

  constructor(rpc: EngineRpc, boot: BootInfo, palettes: Record<string, any>, bundledOn: boolean) {
    this.rpc = rpc;
    this.#palettes = palettes;
    this.bundledOn = bundledOn;
    this.packVersion = boot.packVersion;
    this.engineVersion = boot.version;
    this.engine = new Proxy(
      {},
      { get: (_, m: string) => (...args: unknown[]) => rpc.call(m, ...args) },
    ) as Session["engine"];

    const loaded = boot.config ?? {};
    this.config = loaded;
    this.showFirstRun = !!loaded.firstRun;

    const saved = (loaded.openPanes?.length ? loaded.openPanes : [{ book: "John", chapter: 3 }]).slice(0, 3);
    // Phones restore ONE pane — the narrow rule guards addPane, but a config
    // written by a wider window (or an older build) can still carry several.
    // Keep the pane the reader was last in.
    const restored = this.narrow ? [saved[Math.min(loaded.activePane ?? 0, saved.length - 1)]] : saved;
    this.panes = restored.map((p: any) => ({
      book: p.book,
      chapter: p.chapter,
      // Reopen mid-chapter: the saved first-visible verse becomes the
      // scroll target once the first layout lands.
      targetVerse: p.verse && p.verse > 1 ? p.verse : null,
      pendingScroll: !!(p.verse && p.verse > 1),
      scrollY: 0,
      back: [],
      fwd: [],
    }));
    this.activePane = Math.min(loaded.activePane ?? 0, this.panes.length - 1);

    const mq = matchMedia("(max-width: 700px)");
    mq.addEventListener("change", () => (this.narrow = mq.matches));

    this.applyTheme();
    this.#systemDark.addEventListener("change", () => {
      if (this.config.theme === "system") this.applyTheme();
    });

    rpc.onAuthored = () => {
      this.invalidate();
      this.studyEpoch++;
    };
    // A dwell report changed the reading map and nothing else. Without this the
    // navigator kept showing the map from whenever it was first asked — the
    // per-day cache key meant a chapter finished mid-session did not appear until
    // the next launch (2026-07-29).
    rpc.onReadingWrote = () => this.invalidateOnly("readingBooks", "readingChapters");
    rpc.onCoreReady = () => {
      // Strong's + margin notes just arrived — panels re-fetch.
      this.invalidate();
      this.studyEpoch++;
      // The overlay rides in on the same stage. Only now can we say whether
      // this home has one, and only now can the reader's saved preference be
      // handed to the engine — it opens with the overlay off, always.
      void this.rpc.call("akjvAvailable").then((yes) => {
        this.akjvAvailable = !!yes;
        if (yes && this.config.akjvOverlay === true) {
          void this.rpc.call("setAkjvOverlay", true).then(() => {
            this.layoutEpoch++;
            this.invalidate();
            this.studyEpoch++;
          });
        }
      });
    };
    rpc.onWarmReady = () => {
      // A study opened while the warm was still running answered with only the
      // sections whose indexes existed — the engine will not build one inside a
      // tap any more, because doing so froze the worker for 22 seconds on a
      // phone (2026-07-28). They exist now; re-fetch so the panel fills in.
      this.invalidate();
      this.studyEpoch++;
    };
    rpc.onRndPreparing = () => {
      this.rndProgress = 1;
      this.rndPreparing = true;
    };
    rpc.onRndReady = () => {
      // The machine tiers just lit up — anything on screen re-fetches.
      this.rndState = "ready";
      this.rndPreparing = false;
      this.invalidate();
      this.studyEpoch++;
    };
    rpc.onRndProgress = (fraction) => {
      if (this.rndState !== "ready") this.rndState = "loading";
      this.rndProgress = fraction;
    };
    rpc.onRndPreparing = () => {
      // Downloaded; the engine is parsing it now. Progress can't advance
      // through that, so the UI stops showing a number and says what's
      // happening instead.
      this.rndProgress = 1;
      this.rndPreparing = true;
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

  #configSnapshot(): any {
    this.config.openPanes = this.panes.map((p, i) => ({
      book: p.book,
      chapter: p.chapter,
      verse: this.#firstVisibleVerse(i),
    }));
    this.config.activePane = this.activePane;
    this.config.firstRun = undefined;
    return JSON.parse(JSON.stringify(this.config));
  }

  /** Save immediately (tab hide/close) — no debounce. */
  flushConfig(): void {
    if (this.restoring) return;
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    void this.rpc.static("configSave", this.#configSnapshot());
    // The boot snapshot resumes at this scroll (localStorage: synchronous,
    // safe in pagehide).
    try {
      localStorage.setItem("plumbline:lastScroll", String(this.panes[0]?.scrollY ?? 0));
    } catch {
      /* fine — the snapshot starts at the top */
    }
  }

  resolvedTheme(): string {
    const t = this.config.theme ?? "system";
    return t === "system" ? (this.#systemDark.matches ? "dark" : "light") : t;
  }

  applyTheme(): void {
    this.palette = this.#palettes[this.resolvedTheme()] ?? {};
    const root = document.documentElement;
    for (const [k, v] of Object.entries(this.palette))
      if (typeof v === "string") root.style.setProperty(`--${k}`, v);
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", this.palette.paper ?? "#fcf9f4");
    // The boot snapshot paints before the engine exists — it needs last
    // session's palette without asking the worker.
    try {
      localStorage.setItem("plumbline:palette", JSON.stringify(this.palette));
    } catch {
      /* storage full/blocked: the snapshot just paints in default light */
    }
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
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => {
      if (this.restoring) return;
      void this.rpc.static("configSave", this.#configSnapshot());
    }, 300);
  }

  /** Fetch + load the deferred machine-tier pack (the phones' "load analysis"
   *  action and the Settings toggle; desktops auto-load after boot when the
   *  machine tier is on). Idempotent — the worker reuses an in-flight run. */
  ensureRnd(): Promise<void> {
    if (this.rndState === "ready") return Promise.resolve();
    this.rndState = "loading";
    return this.rpc.loadRnd().then(
      () => {
        this.rndState = "ready";
        this.rndPreparing = false;
        this.invalidate();
        this.studyEpoch++;
      },
      () => {
        // Offline / fetch failed — back to the explicit action.
        this.rndState = "off";
        this.rndPreparing = false;
        this.showToast("Couldn't download the analysis pack — check your connection.");
      },
    );
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
    const count = this.chapterCount(book);
    if (count > 0) chapter = Math.min(chapter, count);
    chapter = Math.max(chapter, 1);
    if (opts.history !== false && (pane.book !== book || pane.chapter !== chapter)) {
      pane.back.push({ book: pane.book, chapter: pane.chapter });
      pane.fwd = [];
    }
    pane.book = book;
    pane.chapter = chapter;
    pane.targetVerse = verse;
    pane.pendingScroll = verse != null;
    pane.scrollY = 0;
    pane.reached = 0; // a new chapter is a new reading pass
    // Going to a passage means going to the TEXT. Every route into the reader
    // funnels through here — a weave tapped in Explore, a search hit, a
    // cross-reference, the navigator — so this is the one place that has to know
    // it, rather than each caller remembering (which is how Explore-as-a-screen
    // first shipped a weave that navigated a pane nobody could see).
    this.screen = "read";
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
    pane.reached = 0;
    pane.pendingScroll = false;
    pane.scrollY = 0;
    this.activePane = paneIdx;
    this.saveConfig();
  }

  /** ±1 chapter with cross-book stepping (canon-adjacent, like the desktops). */
  stepChapter(paneIdx: number, dir: -1 | 1): void {
    const pane = this.panes[paneIdx];
    if (!pane) return;
    const count = this.chapterCount(pane.book) || 1;
    let book = pane.book;
    let chapter = pane.chapter + dir;
    if (chapter < 1 || chapter > count) {
      const books: any[] = this.q("toc")?.books ?? [];
      const i = books.findIndex((b) => b.id === pane.book);
      const adj = books[i + dir];
      if (!adj) return;
      book = adj.id;
      chapter = dir < 0 ? Number(adj.chapters) || 1 : 1;
    }
    this.navigate(paneIdx, book, chapter);
  }

  addPane(afterIdx: number): void {
    if (this.panes.length >= 3 || this.narrow) return;
    const src = this.panes[afterIdx];
    this.panes.splice(afterIdx + 1, 0, {
      book: src.book,
      chapter: src.chapter,
      targetVerse: null,
      pendingScroll: false,
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

  /** Whether this home carries a usable overlay — the toggle hides without it
   *  rather than offering a switch that does nothing. Set once stage 2 lands. */
  akjvAvailable = $state(false);
  /** Bumped when something that changes the WORDS on the page changes. The
   *  reader's layout effect tracks it; `studyEpoch` only refreshes panels. */
  layoutEpoch = $state(0);

  /** Turn the plain-English overlay on or off. Engine state, so two panes can
   *  never disagree; persisted like any other reader preference; and the layout
   *  is dropped so the chapter re-lays with the new words. Reader ONLY —
   *  memorize, Present, copy and share stay KJV (core::akjv). */
  async setAkjvOverlay(on: boolean): Promise<void> {
    this.config.akjvOverlay = on;
    this.saveConfig();
    // ORDER MATTERS. The engine flag has to be set before anything re-lays, or
    // the new layout is measured against the old setting and the page keeps the
    // words it already had (feedback 2026-07-27 — the toggle "wasn't live").
    // The reader deliberately does NOT track `config.akjvOverlay`: doing that
    // would fire a layout the instant the line above runs, which is one RPC
    // AHEAD of the call below, and it would race to the worker first.
    await this.rpc.call("setAkjvOverlay", on);
    this.layoutEpoch++;
    this.invalidate();
    this.studyEpoch++;
  }

  /** The data pack version this session booted on — half of what `?v=` stamps
   *  mean, and what the cache sweep keeps. Shown in About: the DATA moves
   *  independently of the code, so a bug report needs both. */
  packVersion = "";
  /** The wasm engine's own version string (About / boot diagnostics). */
  engineVersion = "";

  /** Re-store this build's shell and sweep every superseded version. Runs at
   *  idle after boot; also the seam the e2e sweep test drives. */
  async sweepCaches(): Promise<void> {
    // Store the shell first, then let the WORKER reclaim: it holds the pin, which
    // is the authority on which pack files this device should still have.
    const shell = await precacheShell();
    if (shell.length) await this.rpc.prune(shell);
  }

  /** A newer build is deployed and this session is still on the old one. Shown
   *  as a toast the reader can act on or ignore — never an automatic reload,
   *  which would yank the page out from under someone mid-verse. */
  updateReady = $state(false);
  /** Don't re-ask within this long of the last check (visibility fires often). */
  #lastUpdateCheck = 0;

  async checkForUpdate(force = false): Promise<boolean> {
    const now = performance.now();
    if (!force && this.#lastUpdateCheck && now - this.#lastUpdateCheck < 15 * 60_000) {
      return this.updateReady;
    }
    this.#lastUpdateCheck = now;
    if (await updateAvailable()) this.updateReady = true;
    return this.updateReady;
  }

  /** Take the update: the new index.html and its bundles are fetched on the
   *  way back up (index.html is network-first), so a plain reload is enough. */
  applyUpdate(): void {
    this.updateReady = false;
    location.reload();
  }
}

let session: Session | null = null;
export function initSession(
  rpc: EngineRpc,
  boot: BootInfo,
  palettes: Record<string, any>,
  bundledOn: boolean,
): Session {
  session = new Session(rpc, boot, palettes, bundledOn);
  return session;
}
export function getSession(): Session {
  if (!session) throw new Error("session not initialized");
  return session;
}
