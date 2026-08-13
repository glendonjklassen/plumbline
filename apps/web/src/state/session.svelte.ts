// The shell's shared state — now over the ENGINE WORKER (TODO #28): the
// engine lives in engine.worker.ts and never blocks this thread. Components
// keep their synchronous `$derived` graphs by reading through `q()`, a
// reactive read-through cache: a miss returns null and fires the async fill;
// the reply bumps `cacheEpoch`, deriveds re-run, and the value is there.
// Panels already tolerate a null block list, so "loading" is just a frame or
// two of empty — never a frozen UI.

import { precacheShell } from "../engine/precache";
import { updateAvailable } from "../engine/update";
import { DEFAULT_FONT, FONT_CSS_FAMILY, FONT_FILES } from "../engine/fonts.generated";
import { EngineRpc, type BootInfo } from "../engine/worker-client";
import { nowStamp } from "../engine/StudyEngine";
import { fontStackFor, setReaderFont } from "../reader/measure";
import { cleanChurch, PWA_URL, shareUrl, type Church } from "../shell/church";
import { lang, t } from "../lib/i18n.svelte";

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

/** The analytical map popups. Both are weave visualisations, not
 *  embedding-derived. */
export type MapPopup = { kind: "chord" } | { kind: "constellation" };

const HISTORY_CAP = 50; // mirrors core config::HISTORY_CAP

/**
 * The delimiter between an engine method name and its arguments in a cache key.
 *
 * Spelled ONCE, on purpose: every key builder and every prefix-exemption must
 * use this exact separator. When `invalidate()` hand-wrote its exempt prefix as
 * `"toc "` — with a SPACE — while `q()` built keys with a NUL, the exemption
 * matched no key, so every core-ready, warm-ready, rnd-ready and authored event
 * dropped the TOC and the canon segments: mid-refill the canon strip painted
 * nothing, a click on it did nothing, and stepping across a book boundary had no
 * book list to step into.
 */
const KEY_SEP = "\0";
/** Namespace for `qs()` keys — engine-independent statics, never invalidated. */
const STATIC_NS = "static:";

/** The cache key for an engine read. The one builder; see [[KEY_SEP]]. */
function cacheKey(method: string, args: unknown[]): string {
  return `${method}${KEY_SEP}${JSON.stringify(args)}`;
}

/**
 * Reads that are pinned in the cache: no invalidation drops them and no
 * eviction may take them.
 *
 * They are corpus-derived — the TOC and the canon shape cannot change while a
 * session runs, because the text does not. Keeping them is not an optimisation:
 * `navigate()` clamps the chapter against `chapterCount()` and `stepChapter()`
 * finds the adjacent book in the TOC, so a session that momentarily has no TOC
 * clamps against nothing and steps nowhere.
 */
const PINNED_READS = ["toc", "canonSegments"];

function isPinned(key: string): boolean {
  return key.startsWith(STATIC_NS) || PINNED_READS.some((m) => key.startsWith(m + KEY_SEP));
}

/**
 * How many chapters' gutter marks are remembered (per kind).
 *
 * Six are live at the very most — three panes × weave dots + note marks — and
 * the rest are chapters the reader has walked away from. Sixteen lets a pane
 * step through several chapters without evicting another pane's marks; going
 * deeper would only remember chapters nobody is looking at, and the cost of an
 * eviction is one extra repaint of a chapter being reopened.
 */
const MARKS_CAP = 16;

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

  // ── search: what is typed, and what the engine is asked ─────────────────────
  //
  // Two fields, because they are two different things. The web shell searches
  // LIVE, so without a debounce every keystroke is a full query: the four ranked
  // tiers over the whole corpus, then a block list of up to 200 hits with their
  // verse text, then JSON across the worker boundary — and the engine lives in
  // ONE thread, so the eighth keystroke's answer queues behind seven answers
  // nobody would ever read, in front of the layout and tap RPCs of the chapter
  // underneath. Android has never had this: its search runs on the IME Search
  // action, once (StudyScreen.kt). The debounce is the web coming closer to
  // that, not away from it.

  /**
   * How long the field waits after the last keystroke before the engine hears
   * about it.
   *
   * 180 ms. Above the ~120–160 ms a fast typist leaves between characters, so a
   * word typed straight through is ONE query rather than eight; below the ~200 ms
   * at which a pause starts to read as the app lagging. Ordinary typing therefore
   * asks nothing at all until the reader stops.
   */
  static readonly SEARCH_DEBOUNCE_MS = 180;

  /** What the reader has typed. The field shows THIS, so it never lags a
   *  keystroke behind the keyboard. */
  searchDraft = $state("");
  /** The query the study panel actually asks the engine for: [[searchDraft]]
   *  once it has stopped moving. While the wait runs, the panel keeps showing
   *  the last answer rather than blanking. */
  searchQuery = $state("");

  #searchTimer: ReturnType<typeof setTimeout> | null = null;

  /** A keystroke in the search field. */
  setSearch(text: string): void {
    this.searchDraft = text;
    if (this.#searchTimer) clearTimeout(this.#searchTimer);
    this.#searchTimer = null;
    // Emptying the field is a dismissal, not a query — nothing to wait for, and
    // waiting would leave the old hits up for a fifth of a second after the
    // reader wiped the field.
    if (!text.trim()) {
      this.searchQuery = text;
      return;
    }
    this.#searchTimer = setTimeout(() => {
      this.#searchTimer = null;
      this.searchQuery = this.searchDraft;
    }, Session.SEARCH_DEBOUNCE_MS);
  }

  /** Close the search field — both halves, at once. A pending wait is dropped:
   *  its query would otherwise land after the field it came from is gone. */
  clearSearch(): void {
    if (this.#searchTimer) clearTimeout(this.#searchTimer);
    this.#searchTimer = null;
    this.searchDraft = "";
    this.searchQuery = "";
  }

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
  /** The ≡ utilities menu (History · Guide & about · shortcuts · Settings).
   *  Session state, not Shell-local, so every destination's ScreenBar can
   *  raise the same menu — Settings must not cost a trip back to Read. */
  menuOpen = $state(false);
  /**
   * Which DESTINATION is on screen — the web twin of Android's `Dest`.
   *
   * A destination replaces the reader; it does not hover over it — Explore and
   * Memorize are full screens, not a bottom sheet or modal over the verse you
   * were reading. `"read"` is the absence of a destination rather than one of
   * its own.
   */
  screen = $state<"read" | "explore" | "memorize" | "plans" | "preach" | "hymnal" | "share">("read");

  // ── the hymnal ──────────────────────────────────────────────────────────────

  /** The hymn being read, and how far its chords are transposed. `semis` lives
   *  with the id because it is about THIS hymn: a singer who dropped one hymn a
   *  tone has said nothing about the next one, and carrying the offset across
   *  would silently rewrite a chart they never asked to change. */
  hymn = $state<{ id: string; semis: number } | null>(null);
  /** Which language the hymnal shows where a hymn has more than one. A
   *  PREFERENCE, not a promise — a German-only hymn still shows German.
   *
   *  IT STARTS AS THE APP'S LANGUAGE, and that is the whole point of seeding it
   *  from `lang()` rather than from `"en"`. This field predates i18n and was a
   *  hard-coded English default, so a German reader would have opened a German
   *  interface onto English hymn texts and had to say "Deutsch" again on every
   *  hymn. Two ideas of what language this reader wants is exactly the drift
   *  there is no reason to have.
   *
   *  It is still its OWN field, because the chips do a different job from the
   *  language setting: a bilingual singer picking the German text of one hymn
   *  has not asked for a German interface, and must not get one. */
  hymnLang = $state(lang());
  /** Whether chords are drawn above the words. Off by default: most people
   *  singing are not playing, and a chart over every line is noise to them. */
  hymnChords = $state(false);
  /** Sing mode: the fullscreen sunlight surface. */
  hymnSinging = $state(false);
  /** Auto-scroll speed in sing mode, 0 (hold) to 9. */
  hymnScroll = $state(0);

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
   *  pane only there: splits are hidden and links never open "beside" (two panes
   *  on a phone is jank). */
  narrow = $state(matchMedia("(max-width: 700px)").matches);
  /** Wide enough for a THIRD reading pane. Between this and `narrow` sits the
   *  foldable/small-laptop band, where the shell is the desktop one but there is
   *  only room for two: an unfolded Pixel Fold is ~840 px, which three panes cut
   *  into 280px columns — and that is before the 380px study sidebar, which is
   *  now allowed to sit beside the text at these widths. */
  roomy = $state(matchMedia("(min-width: 1100px)").matches);
  /** Active text prompt (rendered by PromptDialog); resolves null on cancel. */
  promptReq = $state<{
    title: string;
    initial: string;
    multiline: boolean;
    resolve: (v: string | null) => void;
  } | null>(null);

  /**
   * Every transient surface, as field → the value that means CLOSED.
   *
   * This lives HERE, next to the declarations, and not in the shell, because the
   * shell's version was a hand-kept list of five and there are thirteen of these.
   * Every surface added since that list was written inherited the same trap: its
   * modal covered the screen and the Read tab could not dismiss it. Adding a
   * `$state` surface above
   * without adding it here is now a one-file omission a reviewer can see, rather
   * than a bug in a different file that nobody thinks to look at.
   *
   * A TABLE rather than a run of assignments, because there are now two questions
   * to ask about this set and not one: [[dismissTransient]] closes them all, and
   * [[transientOpen]] asks whether any is open — which is what the phone's Back
   * button needs to know. Two hand-kept lists of sixteen fields is the same trap
   * one level up.
   *
   * `keyof Session` is the point of the `satisfies`: a typo'd field name would
   * otherwise assign a brand-new property and silently stop closing a surface.
   *
   * NOT showFirstRun: a reader who has never chosen a path must not be able to
   * tab past the question. It closes by being answered.
   */
  static readonly TRANSIENT = [
    ["screen", "read"],
    ["panel", null],
    ["mapPopup", null],
    ["memorize", null],
    ["hymn", null],
    ["hymnSinging", false],
    ["contextMenu", null],
    ["tagPickFor", null],
    ["threadPickFor", null],
    ["tagWeaveFor", null],
    ["memorizePassageFrom", null],
    ["markReadFor", null],
    ["bookNavFor", null],
    ["reopenIntro", null],
    ["showHistory", false],
    ["showSettings", false],
    ["showShortcuts", false],
    ["showPresent", false],
    ["menuOpen", false],
  ] as const satisfies readonly (readonly [keyof Session, unknown])[];

  /**
   * Close every transient surface — dialogs, sheets, pickers, popups, the study
   * panel. What a destination tap does before it opens anything, and what the
   * Back button does on a phone.
   *
   * `promptReq` is RESOLVED, not just nulled: `askText` handed a promise to a
   * caller that is still awaiting it, and dropping the request on the floor would
   * leave that caller hanging forever.
   */
  dismissTransient(): void {
    this.cancelPrompt();
    // A pending confirmation resolves NO. Anything else would leave a caller
    // awaiting a promise that never settles, or worse, destroy something because
    // the reader navigated away.
    this.cancelConfirm();
    for (const [field, closed] of Session.TRANSIENT) {
      (this as unknown as Record<string, unknown>)[field] = closed;
    }
  }

  /** Whether anything [[dismissTransient]] would close is on screen — the
   *  question the phone's Back button asks (see [[installRouter]]). */
  get transientOpen(): boolean {
    if (this.promptReq || this.confirmReq) return true;
    return Session.TRANSIENT.some(
      ([field, closed]) => (this as unknown as Record<string, unknown>)[field] !== closed,
    );
  }

  /** An open confirmation. One mechanism for every destructive action, so that
   *  "does this ask first?" is answered in one place instead of per button —
   *  deleting a memorize card asked nothing at all while deleting a thread had its
   *  own bespoke inline prompt. */
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
  askConfirm(title: string, body: string, verb?: string): Promise<boolean> {
    // One question at a time: a second ask while one is open answers the first
    // with "no". Overwriting it silently left the first caller awaiting a
    // promise nothing could ever settle.
    this.confirmReq?.resolve(false);
    return new Promise((resolve) => {
      this.confirmReq = { title, body, verb: verb ?? t("common.delete"), resolve };
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
   *  text and the reader's own data are always on. */
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

  /**
   * How many engine reads the cache may hold at once.
   *
   * It held everything, for the life of the tab, and several call sites mint a
   * key per interaction rather than per screen: `searchBlocks` is keyed by the
   * query string (one entry per keystroke), `wordStudyBlocks` by the tapped
   * word, `verse` by refKey — the passage-memorize preview asks for one per
   * verse, so Psalm 119 alone is 176 — and `memoryDue`/`memoryCoverage` by a
   * second-granularity stamp taken afresh each time the hub opens.
   *
   * The bound is derived from the largest working set the app can legitimately
   * have LIVE: that 176-verse preview, plus the navigator's day-keyed reading
   * map (1 books read + up to 66 chapter reads), plus three panes and an open
   * study panel — under 300. 512 is that with room, so eviction cannot take an
   * answer out from under something on screen, which would cost a null frame.
   *
   * Honest limit: this bounds the COUNT, not the bytes. One `concordanceBlocks`
   * for a common Strong's code is orders of magnitude bigger than one `verse`,
   * and a byte bound would need a per-entry size estimate. Capping the count is
   * what stops the unbounded growth; it is not a memory budget.
   */
  static readonly CACHE_CAP = 512;

  /**
   * Reads whose answers are too big to keep more than a few of, method → how
   * many.
   *
   * [[CACHE_CAP]] bounds the COUNT and says so; this is the exemption for the
   * entries where that is not enough. A `searchBlocks` answer is up to 200 hits
   * carrying their verse text — the largest single thing this cache holds — and
   * its key is the query string, so a reader typing a word left one behind per
   * keystroke. Only the query on screen can be read, so only the query on screen
   * is kept; the rest were evicting other panels' answers to hold results for
   * fragments of a word nobody will type again.
   */
  static readonly PER_METHOD_CAP: Record<string, number> = { searchBlocks: 1 };

  /** Live cache size — what the e2e bound test measures. */
  get cacheSize(): number {
    return this.#cache.size;
  }

  /** The engine method a cache key belongs to; null for a `qs()` static. */
  #methodOf(key: string): string | null {
    const at = key.indexOf(KEY_SEP);
    return at < 0 ? null : key.slice(0, at);
  }

  /** Drop this method's oldest answers until only `cap` of them are left. */
  #trimMethod(method: string, cap: number): void {
    const mine = [...this.#cache.keys()].filter((k) => this.#methodOf(k) === method);
    for (const k of mine.slice(0, Math.max(mine.length - cap, 0))) this.#cache.delete(k);
  }

  /** Read a hit and move it to the young end of the LRU order. A Map iterates
   *  in insertion order, so re-inserting IS the reordering. */
  #touch(key: string): any {
    const v = this.#cache.get(key);
    this.#cache.delete(key);
    this.#cache.set(key, v);
    return v;
  }

  /** Store a fresh answer, then evict from the old end until the cache is back
   *  inside [[CACHE_CAP]]. Pinned reads are skipped: evicting the TOC breaks
   *  navigation exactly the way dropping it in `invalidate()` did. */
  #store(key: string, value: any): void {
    this.#cache.set(key, value);
    const method = this.#methodOf(key);
    const own = method === null ? undefined : Session.PER_METHOD_CAP[method];
    if (method !== null && own !== undefined) this.#trimMethod(method, own);
    if (this.#cache.size <= Session.CACHE_CAP) return;
    for (const k of this.#cache.keys()) {
      if (this.#cache.size <= Session.CACHE_CAP) break;
      if (!isPinned(k)) this.#cache.delete(k);
    }
  }

  /** Read an engine method through the cache: the cached value, or null while
   *  the worker answers (the reply bumps cacheEpoch → callers re-run). */
  q(method: string, ...args: unknown[]): any {
    void this.cacheEpoch; // register the dependency
    const key = cacheKey(method, args);
    if (this.#cache.has(key)) return this.#touch(key);
    if (!this.#pending.has(key)) {
      this.#pending.add(key);
      this.rpc
        .call(method, ...args)
        .then((v) => {
          this.#store(key, v);
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
    const key = `${STATIC_NS}${fn} ${JSON.stringify(args)}`;
    if (this.#cache.has(key)) return this.#touch(key);
    if (!this.#pending.has(key)) {
      this.#pending.add(key);
      this.rpc
        .static(fn, ...args)
        .then((v) => {
          this.#store(key, v);
          this.#pending.delete(key);
          this.cacheEpoch++;
        })
        .catch(() => this.#pending.delete(key));
    }
    return null;
  }

  /** Await an engine read AND leave it in the cache (prefetch / imperative). */
  async fetchQ(method: string, ...args: unknown[]): Promise<any> {
    const key = cacheKey(method, args);
    if (this.#cache.has(key)) return this.#touch(key);
    const v = await this.rpc.call(method, ...args);
    this.#store(key, v);
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

  /** Mirror the due-card count onto the installed icon (the Badging API).
   *
   *  Feature-detected: browsers without `setAppBadge` — and every un-installed
   *  tab, where the call resolves but paints nothing — skip out or no-op.
   *  Fire-and-forget, errors swallowed: a badge the OS refuses is not a state
   *  the reader can act on. The count can only move while the app is running
   *  (a card falling due at midnight badges on the next launch or resume —
   *  there is no server to push from), so the call sites are boot, resume and
   *  every authoring write. */
  refreshAppBadge(): void {
    const nav = navigator as Navigator & {
      setAppBadge?: (n: number) => Promise<void>;
      clearAppBadge?: () => Promise<void>;
    };
    if (typeof nav.setAppBadge !== "function") return;
    this.rpc
      .call("memoryDue", nowStamp())
      .then((due) => {
        const n = ((due?.refs ?? []) as string[]).length;
        return n > 0 ? nav.setAppBadge!(n) : nav.clearAppBadge?.();
      })
      .catch(() => {});
  }

  /** Drop cached study reads (authoring landed / R&D pack arrived). The
   *  corpus-derived immutables (toc, canon shape, statics) survive — wiping
   *  them made navigation clamp against an empty TOC mid-refill. Which reads
   *  those are, and the delimiter that decides it, live in [[PINNED_READS]] and
   *  [[KEY_SEP]]: hand-writing the prefix here is what broke it. */
  invalidate(): void {
    for (const key of [...this.#cache.keys()]) if (!isPinned(key)) this.#cache.delete(key);
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
      if (methods.some((m) => key.startsWith(m + KEY_SEP))) this.#cache.delete(key);
    this.cacheEpoch++;
  }

  // ── gutter marks, memoized by CONTENT ───────────────────────────────────────
  //
  // The reader pane derives the weave dots and the note marks and its paint
  // effect tracks them, so what it actually reacts to is the IDENTITY of those
  // sets: a freshly built Set holding the same verse numbers repaints the whole
  // canvas to draw the marks that were already on it. Study data is invalidated
  // on core-ready, warm-ready, rnd-ready and every authoring write, and each of
  // those bumps `studyEpoch` too — so a reader scrolling while the background
  // pipeline settles paid a full repaint per event for nothing.
  //
  // Memoizing here rather than in the pane is deliberate: it is one memo for
  // however many panes are open, and the epoch dependency travels with the
  // mechanism instead of having to be remembered at each call site.

  /** Last set returned per (kind, book, chapter) — LRU, [[MARKS_CAP]] deep. */
  #marks = new Map<string, Set<number>>();

  /** Read a memoized set, moving it to the young end of the LRU order. */
  #markHit(key: string): Set<number> | undefined {
    const prev = this.#marks.get(key);
    if (prev === undefined) return undefined;
    this.#marks.delete(key);
    this.#marks.set(key, prev);
    return prev;
  }

  #putMarks(key: string, set: Set<number>): Set<number> {
    this.#marks.set(key, set);
    for (const k of this.#marks.keys()) {
      if (this.#marks.size <= MARKS_CAP) break;
      this.#marks.delete(k);
    }
    return set;
  }

  /** The marks last drawn for this key — a stable empty set when there are none
   *  yet, so the frames before the read lands share one identity as well. */
  #heldMarks(key: string): Set<number> {
    return this.#markHit(key) ?? this.#putMarks(key, new Set());
  }

  /** `next`, or the previous set when it holds exactly the same verses. */
  #memoMarks(key: string, next: Set<number>): Set<number> {
    const prev = this.#markHit(key);
    if (prev && prev.size === next.size) {
      let same = true;
      for (const v of next)
        if (!prev.has(v)) {
          same = false;
          break;
        }
      if (same) return prev;
    }
    return this.#putMarks(key, next);
  }

  /**
   * Verses in a chapter that have a weave partner — the gold gutter dot.
   *
   * The SAME Set comes back while the content is unchanged, including across the
   * gap where an invalidation has dropped `linkPairs` and the refetch has not
   * landed: the marks last drawn are held rather than blinked off and drawn again
   * a frame later.
   *
   * READ-ONLY. It is shared between callers and across frames; a mutation would
   * be a mutation of every pane's dots at once.
   */
  weaveDots(book: string, chapter: number): Set<number> {
    void this.studyEpoch;
    const key = `weaveDots${KEY_SEP}${book} ${chapter}`;
    const pairs = this.q("linkPairs")?.pairs;
    if (!pairs) return this.#heldMarks(key);
    const set = new Set<number>();
    for (const p of pairs) {
      if (p.aBook === book && p.aChapter === chapter) set.add(p.aVerse);
      if (p.bBook === book && p.bChapter === chapter) set.add(p.bVerse);
    }
    return this.#memoMarks(key, set);
  }

  /** Verses in a chapter carrying one of the reader's own notes — the square
   *  gutter mark. Content-memoized and read-only, exactly like [[weaveDots]]. */
  noteVerses(book: string, chapter: number): Set<number> {
    void this.studyEpoch;
    const key = `noteVerses${KEY_SEP}${book} ${chapter}`;
    const notes = this.q("userNotes")?.notes;
    if (!notes) return this.#heldMarks(key);
    const prefix = `${book} ${chapter}:`;
    const set = new Set<number>();
    for (const n of notes)
      if (typeof n.verse === "string" && n.verse.startsWith(prefix))
        set.add(Number(n.verse.slice(n.verse.lastIndexOf(":") + 1)) || 0);
    return this.#memoMarks(key, set);
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
    // Trust the close-safe theme cache over the home config: if the last change
    // never reached the worker's IndexedDB write (a frozen debounce on a
    // backgrounded tab), the home still says the old theme while localStorage
    // holds the reader's actual last choice. Re-persist so the home and the
    // backup catch up. Cleared on restore (SettingsDialog), so a restored
    // backup's theme still wins over a stale cache.
    try {
      const cachedTheme = localStorage.getItem("plumbline:themeChoice");
      if (cachedTheme && cachedTheme !== this.config.theme) {
        this.config.theme = cachedTheme;
        this.saveConfig();
      }
    } catch {
      /* no storage: the home config stands as loaded */
    }
    this.showFirstRun = !!loaded.firstRun;

    const saved = loaded.openPanes?.length ? loaded.openPanes : [{ book: "John", chapter: 3 }];
    // Restore no more panes than fit — `addPane` guards the button, but a config
    // written by a wider window (or an older build) can still carry more than
    // this window has room for. When they all fit, restore them untouched; when
    // they do not, keep the one the reader was last in and fill forward, so a
    // phone opening a three-pane desktop config lands on what they were reading
    // rather than on whatever happened to be leftmost.
    const wasActive = Math.min(loaded.activePane ?? 0, saved.length - 1);
    const from = saved.length <= this.maxPanes ? 0 : Math.min(wasActive, saved.length - this.maxPanes);
    const restored = saved.slice(from, from + this.maxPanes);
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
    this.activePane = Math.min(Math.max(wasActive - from, 0), this.panes.length - 1);

    const mq = matchMedia("(max-width: 700px)");
    mq.addEventListener("change", () => (this.narrow = mq.matches));
    const wide = matchMedia("(min-width: 1100px)");
    wide.addEventListener("change", () => (this.roomy = wide.matches));

    this.applyTheme();
    this.#systemDark.addEventListener("change", () => {
      if (this.config.theme === "system") this.applyTheme();
    });

    rpc.onAuthored = () => {
      this.invalidate();
      this.studyEpoch++;
      // Any write can change what is due (a card added, graded, or removed —
      // and the rest cost one cheap read on a path that just paid a file write).
      this.refreshAppBadge();
    };
    // A dwell report changed the reading map and nothing else. Without this the
    // navigator kept showing the map from whenever it was first asked — the
    // per-day cache key meant a chapter finished mid-session did not appear until
    // the next launch.
    // "plans" rides along because PLAN COMPLETION IS DERIVED FROM THE READING
    // STORE (READING-PLANS.md decision #2): a chapter finishing is exactly the
    // event that moves a plan's day on. Without it the cached plans answer kept
    // its stale `read` flags, so the chip and the today card still pointed at
    // the chapter you had just finished — tap it and you were sent back to
    // Genesis 1 all evening (the maintainer's UAT report, 2026-08-11).
    rpc.onReadingWrote = () => this.invalidateOnly("readingBooks", "readingChapters", "plans");
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
      // phone. They exist now; re-fetch so the panel fills in.
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

    rpc.onPersistFailed = (info) => (this.persistFailed = info);
    // Whatever went wrong has stopped going wrong — take the notice down rather
    // than leaving the reader to wonder whether their note is safe.
    rpc.onPersistOk = () => (this.persistFailed = null);

    // The web twin of Android's ON_PAUSE persist: flush the session (incl.
    // the scroll verse) when the tab hides or unloads.
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "hidden") this.flushSession();
    });
    addEventListener("pagehide", () => this.flushSession());
  }

  /** A save to this device's storage failed. Sticky: the reader has to hear it,
   *  and NOT from a toast that fades while they are looking away — their note
   *  exists only in this tab until it lands. Deliberately absent from
   *  `dismissTransient`: it is a warning about their data, not a surface they
   *  navigated into, so switching destinations must not silence it. */
  persistFailed = $state<{ detail: string; retrying: boolean } | null>(null);

  /** Try the failed save again — what the notice's button does. Resets the
   *  worker's backoff ladder, so a reader who has just freed some space gets a
   *  full set of attempts rather than the tail of the last set. */
  retryPersist(): void {
    this.rpc.flush().catch(() => {
      /* a dead worker already has its own report; this is not the place */
    });
  }

  /** Everything that must reach the disk before this tab may be frozen: the
   *  config snapshot, then the authored data the worker is still holding behind
   *  its 50 ms debounce. `flushConfig` alone left a note written moments ago in
   *  memory only — a hidden page's timers are frozen, so the debounce may never
   *  fire, and a discarded tab takes the note with it. */
  flushSession(): void {
    this.flushConfig();
    // AFTER flushConfig, and it matters: the RPC is ordered, so the configSave
    // message is already queued when the flush arrives, and the flush's persist
    // carries it too.
    if (!this.restoring) this.retryPersist();
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
      // The theme CHOICE, close-safe. The config save that carries it to the
      // home is debounced and posted to the worker, so a mobile background that
      // freezes the timer and then discards the tab can lose it — the reader
      // picks a theme and it is gone next launch. localStorage is synchronous
      // and survives pagehide, so this write always lands; the
      // boot reconcile trusts it over a home config that may not have caught up.
      localStorage.setItem("plumbline:themeChoice", this.config.theme ?? "system");
    } catch {
      /* storage full/blocked: the snapshot just paints in default light */
    }
  }

  /** Point the DOCUMENT at the chrome face and THIS THREAD's canvas at the
   *  scripture face. Synchronous and safe to call before the faces have
   *  downloaded — CSS swaps the chrome in when it lands, and the reader canvas
   *  is only painted after [[setTextFont]] has awaited the face.
   *
   *  Type and colour are independent axes: this is the twin of [[applyTheme]]
   *  and neither one consults the other. */
  applyFonts(): void {
    const chrome = this.config.chromeFont ?? DEFAULT_FONT;
    document.documentElement.style.setProperty("--chrome-font", fontStackFor(chrome));
    setReaderFont(this.config.textFont ?? DEFAULT_FONT);
    try {
      // The boot snapshot paints chrome before the engine exists, for the same
      // reason the palette is cached here; and the NEXT boot guesses the
      // scripture face from this, so the worker can start downloading it
      // alongside the pack instead of after the config arrives (App.svelte
      // `hintedTextFont`).
      localStorage.setItem("plumbline:chromeFont", chrome);
      localStorage.setItem("plumbline:textFont", this.config.textFont ?? DEFAULT_FONT);
    } catch {
      /* storage full/blocked: the snapshot paints in the default face */
    }
  }

  /** The scripture face. Both threads have to hold it BEFORE anything re-lays:
   *  the worker measures and this thread paints, and a layout measured against
   *  the fallback and painted in the chosen face wraps where it is not drawn. */
  async setTextFont(token: string): Promise<void> {
    this.config.textFont = token;
    this.saveConfig();
    setReaderFont(token);
    // ORDER MATTERS, as in `setAkjvOverlay`: both sides first, relayout after.
    // `document.fonts.load` is what makes the main thread's canvas paint the
    // real face rather than the fallback it would otherwise have cached.
    const family = FONT_CSS_FAMILY[token] ?? FONT_CSS_FAMILY[DEFAULT_FONT];
    await Promise.all([
      this.rpc.setTextFont(token),
      document.fonts.load(`18px "${family}"`),
      document.fonts.load(`bold 18px "${family}"`),
      FONT_FILES[token]?.italic ? document.fonts.load(`italic 18px "${family}"`) : Promise.resolve(),
    ]);
    this.layoutEpoch++;
    this.invalidate();
  }

  /** The chrome face. No relayout: nothing the ENGINE measured changes, only
   *  what CSS paints the controls with. */
  setChromeFont(token: string): void {
    this.config.chromeFont = token;
    this.applyFonts();
    this.saveConfig();
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
        this.showToast(t("rnd.downloadFailed"));
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
    // Concept-study sweeps are generous by design (docs/READING-PLANS.md §Concept Study):
    // opening a chapter in the mode marks it swept — no dwell, any order — so
    // progress is breadth of the sweep, not time in it. Pane 0 only, the one
    // the reader is actually paging through.
    if (paneIdx === 0 && this.inConceptStudy) this.#sweepCurrent(book, chapter);
    this.saveConfig();
  }

  // ── the concept study (a concept sweep with its own reader mode) ────────────────
  /** Whether the reader is in concept-study mode — verse taps tag, the reading
   *  tracker is suspended (Shell.svelte's `target` guards on this). */
  get inConceptStudy(): boolean {
    return !!this.config.conceptStudy;
  }
  get conceptStudyId(): string {
    return this.config.conceptStudy ?? "";
  }
  /** The active concept study's preset tag, from the plans view-model — null when
   *  not in the mode, or before the plans read has landed. */
  get conceptStudyTag(): string | null {
    const id = this.conceptStudyId;
    if (!id) return null;
    const run = (this.q("plans", "")?.running ?? []).find((p: any) => p.id === id);
    return run?.tag ?? null;
  }

  #sweepCurrent(book: string, chapter: number): void {
    void this.rpc.call("conceptStudySweep", this.conceptStudyId, book, chapter).catch(() => {});
  }

  /** Start (or resume) a concept study for `tag` and enter the mode. */
  async startConceptStudy(tag: string): Promise<void> {
    const id = await this.rpc.call("conceptStudyStart", tag, nowStamp());
    if (typeof id !== "string" || id.startsWith("!")) {
      this.showToast(t("conceptStudy.startFailed"));
      return;
    }
    this.config.conceptStudy = id;
    this.saveConfig();
    this.invalidate();
    this.studyEpoch++;
    // Into the text, where the sweep happens; the current chapter counts.
    this.goRead();
    if (this.panes[0]) this.#sweepCurrent(this.panes[0].book, this.panes[0].chapter);
    this.showToast(t("conceptStudy.entered", { tag }));
  }

  /** Re-enter an existing concept study (from the Plans screen) without re-seeding. */
  enterConceptStudy(id: string): void {
    this.config.conceptStudy = id;
    this.saveConfig();
    // Drop the cached plans read like startConceptStudy does: the epoch alone
    // re-ran deriveds into the SAME cached object, so a resume could show the
    // run's stale tag and swept count until some other write invalidated.
    this.invalidate();
    this.studyEpoch++;
    this.goRead();
    if (this.panes[0]) this.#sweepCurrent(this.panes[0].book, this.panes[0].chapter);
  }

  /** Leave concept-study mode — the run and its gathered tag stay; taps go back to
   *  word study and the reading tracker resumes. */
  exitConceptStudy(): void {
    this.config.conceptStudy = "";
    this.saveConfig();
  }

  /** The active run's preset tag, awaited past a cold cache. A relaunch lands
   *  straight in the mode with the plans query unfetched, and a tap that
   *  silently did nothing until it warmed would swallow the reader's first
   *  gather — so this asks the engine rather than trusting the cache. */
  async #conceptStudyTagAwaited(): Promise<string | null> {
    const cached = this.conceptStudyTag;
    if (cached) return cached;
    const id = this.conceptStudyId;
    if (!id) return null;
    const plans = await this.rpc.call("plans", "").catch(() => null);
    return (plans as any)?.running?.find((p: any) => p.id === id)?.tag ?? null;
  }

  /** A verse tapped in concept-study mode: confirm, then tag it with the preset tag
   *  (creating the tag on the first one). The chapter is already swept by
   *  navigation; this is the gather. */
  async conceptStudyTagVerse(refKey: string): Promise<void> {
    if (!refKey) return;
    const tag = await this.#conceptStudyTagAwaited();
    if (!tag) {
      // A tap that silently does nothing reads as a broken mode. This happens
      // when the run behind config.conceptStudy has no record (removed
      // out-of-band, or the plans read failed) — say so and point at the fix.
      this.showToast(t("conceptStudy.noTag"));
      return;
    }
    const ok = await this.askConfirm(t("conceptStudy.tagAsk", { tag, verse: refKey }), "", t("conceptStudy.tagVerb", { tag }));
    if (!ok) return;
    const err = await this.author("tagAdd", tag, "verse", refKey, null, nowStamp());
    this.showToast(err ?? t("conceptStudy.tagged", { tag, verse: refKey }));
  }

  /** Start a built-in schedule. Its class holds one plan at a time, so a
   *  conflicting one is confirmed-then-replaced (the FFI replaces; the ask is
   *  the shell's, per the house rule about destroying a running plan). */
  async startPlan(b: { id: string; class: string; name: string }): Promise<void> {
    const running = (this.q("plans", "")?.running ?? []) as any[];
    const conflict = running.find((p) => p.class === b.class && p.id !== b.id);
    if (conflict) {
      const ok = await this.askConfirm(t("plans.replaceAsk", { name: b.name }), t("plans.replaceBody"), t("plans.replaceVerb"));
      if (!ok) return;
    }
    const err = await this.author("planStart", b.id, nowStamp());
    this.showToast(err ?? t("plans.started", { name: b.name }));
  }

  /** Pause or resume a plan. No confirm: nothing is lost either way, and the
   *  Plans screen states the plan's standing right where the button sits. */
  async setPlanPaused(id: string, paused: boolean, name: string): Promise<void> {
    const err = await this.author("planSetPaused", id, paused);
    this.showToast(err ?? t(paused ? "plans.pausedToast" : "plans.resumedToast", { name }));
  }

  /** Stop a plan (schedule or concept study) — confirmed, since it removes the
   *  plan's record. A concept study's gathered tag is untouched. */
  async stopPlan(id: string, name: string): Promise<void> {
    const ok = await this.askConfirm(t("plans.stopAsk", { name }), t("plans.stopBody"), t("plans.stopVerb"));
    if (!ok) return;
    // Leaving the mode too, if this is the concept study we are in.
    if (this.conceptStudyId === id) this.exitConceptStudy();
    const err = await this.author("planStop", id);
    this.showToast(err ?? t("plans.stopped", { name }));
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
    // History is navigation too: arriving at a chapter sweeps it, same as
    // navigate() — mouse-button 4/5 steps were the one route into a chapter
    // that left no mark on a concept study's coverage.
    if (paneIdx === 0 && this.inConceptStudy) this.#sweepCurrent(pane.book, pane.chapter);
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

  // ── the address bar: one bookmarkable address per chapter ───────────────────
  //
  // Nothing was bookmarkable before this (audit item D-05): every chapter in the
  // Bible lived at `/`, so a reader could not send anyone a passage without
  // going through the verse-share menu, and a browser reload was the only way
  // back to where they were.

  /** Pane 0's chapter as a hash route — `#/John/3`.
   *
   *  The book travels as its OSIS ID (`John`, `1John`, `Song`), never as the
   *  display name: the id is the frozen wire form, it is a single word so nothing
   *  in it needs escaping, and it is the form `public/404.html` already forwards
   *  intact (e2e/entry.spec.ts). */
  static hashRoute(book: string, chapter: number): string {
    return `#/${book}/${chapter}`;
  }

  /**
   * The chapter an address asks for, or null when it asks for nothing we have.
   *
   * Liberal on the way IN — `#/john/3` and `#/1 John/3` both resolve, because
   * these get hand-typed and forwarded through mail clients — and strict about
   * the answer: an unknown book or an out-of-range chapter returns null so every
   * caller falls back to the restored session. A blank reader is the one thing a
   * bad link must never produce.
   *
   * Resolved against the TOC, so callers need it loaded — it is prefetched at
   * boot and pinned in the cache ([[PINNED_READS]]).
   */
  routeFromHash(hash: string): { book: string; chapter: number } | null {
    let raw = hash;
    try {
      raw = decodeURIComponent(hash);
    } catch {
      /* a stray % is simply an address we don't understand */
    }
    const m = /^#\/([^/]+)\/(\d{1,3})$/.exec(raw);
    if (!m) return null;
    const want = m[1].toLowerCase().replace(/\s+/g, "");
    const books: any[] = this.q("toc")?.books ?? [];
    const hit =
      books.find((b) => String(b.id).toLowerCase() === want) ??
      books.find((b) => String(b.name).toLowerCase().replace(/\s+/g, "") === want);
    if (!hit) return null;
    const chapter = Number(m[2]);
    if (chapter < 1 || chapter > (Number(hit.chapters) || 0)) return null;
    return { book: String(hit.id), chapter };
  }

  /**
   * Mirror pane 0 into `location.hash`.
   *
   * REPLACE, never push. A reader flicking through Psalms would otherwise need
   * forty Back presses to get out of the app; pushing is reserved for surfaces,
   * where Back means "close this" ([[pushSurfaceEntry]]).
   *
   * The search string is carried through untouched: App.svelte strips `?at=` /
   * `?church=` on purpose, and this must neither resurrect them nor drop a query
   * it was not asked to drop. `history.state` is carried through for the same
   * reason — see [[pushSurfaceEntry]].
   */
  syncUrl(): void {
    const pane = this.panes[0];
    if (!pane) return;
    const url = location.pathname + location.search + Session.hashRoute(pane.book, pane.chapter);
    if (url === location.pathname + location.search + location.hash) return;
    history.replaceState(history.state, "", url);
  }

  /**
   * Whether a history entry we pushed for an open surface is still on the stack.
   *
   * An instance flag rather than a marker in `history.state`, because
   * `history.state` outlives a reload and an open sheet does not: a reader who
   * reloads while the marker is current would come back to a claim that a
   * surface is open when the app has just booted with nothing on screen.
   */
  #surfaceEntry = false;
  /** A `history.back()` WE asked for is in flight, to hand back an entry whose
   *  surface has already closed some other way. It must not be mistaken for the
   *  reader pressing Back — see [[dropSurfaceEntry]]. */
  #spending = false;

  /** Give an open surface its own history entry, so the phone's Back button
   *  closes the surface instead of leaving the PWA — the behaviour Android has
   *  had since it shipped (BackHandler in StudyScreen / Memorize / Present). One
   *  entry for the whole stack, because `dismissTransient` closes the stack. */
  pushSurfaceEntry(): void {
    // Never while a spend is in flight: `history.back()` is queued as a delta, so
    // pushing under it would send the traversal somewhere nobody asked for. The
    // spend lands in a task and re-pushes then if a surface has re-opened.
    if (this.#surfaceEntry || this.#spending) return;
    this.#surfaceEntry = true;
    history.pushState(null, "", location.href);
  }

  /** The surface closed some other way (Escape, its own ✕, a destination tap):
   *  hand the entry back now, or the reader's next Back press does nothing at
   *  all. Not `#surfaceEntry = false` on the spot — the popstate handler has to
   *  be able to tell our own traversal from the reader's. */
  dropSurfaceEntry(): void {
    if (!this.#surfaceEntry || this.#spending) return;
    this.#spending = true;
    history.back();
  }

  /**
   * Wire the address bar to this session. Called once, after the TOC is in.
   */
  installRouter(): void {
    this.syncUrl();
    addEventListener("popstate", () => {
      // Neither branch below ROUTES, and that is the whole reason the flags
      // exist. Because chapter turns replace instead of pushing, the entry under
      // a surface entry still holds whatever address it was last stamped with,
      // which can be several chapters stale — a weave tapped in Explore moves the
      // reader while the surface entry is the current one. Routing from that
      // stale address would drag them back to the chapter they left, so both
      // branches re-stamp the entry they land on instead.
      if (this.#spending) {
        this.#spending = false;
        this.#surfaceEntry = false;
        this.syncUrl();
        // Something re-opened while the traversal was in flight (a menu action
        // that closes itself and then awaits the engine before raising a prompt).
        // It gets its own entry now rather than inheriting a spent one.
        if (this.transientOpen) this.pushSurfaceEntry();
        return;
      }
      if (this.#surfaceEntry) {
        this.#surfaceEntry = false;
        this.dismissTransient();
        this.syncUrl();
        return;
      }
      this.#routeFromUrl();
    });
    // Editing the fragment in the address bar is a navigation rather than a
    // traversal, so it arrives here and not as a popstate.
    addEventListener("hashchange", () => this.#routeFromUrl());
  }

  /** Take the current address as a navigation request. An address we cannot read
   *  is answered by putting the real one back, never by moving the reader. */
  #routeFromUrl(): void {
    const pane = this.panes[0];
    if (!pane) return;
    const to = this.routeFromHash(location.hash);
    if (!to) {
      this.syncUrl();
      return;
    }
    // Already there: do NOTHING. `navigate` resets the scroll offset and the
    // banded verse, so answering a no-op traversal with it would throw a reader
    // who is halfway down a chapter back to verse 1.
    if (to.book === pane.book && to.chapter === pane.chapter) return;
    this.navigate(0, to.book, to.chapter);
  }

  /** How many reading panes fit: one on a phone, two on a foldable or a small
   *  laptop, three when there is real room. The split control reads the same
   *  number, so what the button offers and what `addPane` allows cannot drift. */
  get maxPanes(): number {
    return this.narrow ? 1 : this.roomy ? 3 : 2;
  }

  addPane(afterIdx: number): void {
    if (this.panes.length >= this.maxPanes) return;
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
    // words it already had.
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
