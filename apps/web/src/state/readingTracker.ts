// The reading map's shell half on the web: how long a chapter was really read.
//
// The Android twin is ui/ReadingTracker.kt — same three refusals, same
// thresholds (both read them from the core's spec over the ABI, so neither shell
// can drift on what counts as reading):
//
//   * a GRACE period before anything accrues, so paging through to find
//     something never credits what it flew past;
//   * an IDLE cutoff, so a tab left open does not read Leviticus overnight;
//   * HIDDEN stops the clock — a backgrounded tab is not being read.
//
// The core owns what "read" MEANS. This owns only what the core cannot know,
// having no clock and no window: seconds a chapter was actually in front of
// somebody.
//
// Reports are rare on purpose (`spec.tickSeconds`, 30 s). Each one is an engine
// call on the single worker thread that also answers layout and taps, and a
// write into the depot behind it — so the cadence is set by the core, not here.

/** The dial positions the core hands over; these stand in until it answers. */
interface Spec {
  graceSeconds: number;
  idleSeconds: number;
  tickSeconds: number;
}

const FALLBACK: Spec = { graceSeconds: 3, idleSeconds: 120, tickSeconds: 30 };

/** One sample per second: fine enough for grace and idle to land accurately,
 *  and nowhere near often enough to matter. */
const SAMPLE_MS = 1000;

export interface ReadingTargetSource {
  /** The chapter being read, or null when nothing is (a dialog is up, the
   *  reader is in Present, …). */
  target(): { book: string; chapter: number } | null;
  /** Deepest verse reached in the current chapter — the high-water mark the core
   *  pairs with dwell. Scrolling without time credits nothing, and time without
   *  scrolling credits only what was on screen. */
  reached(): number;
  /** Report `seconds` of dwell. Resolves to the core's verdict, or null. */
  record(
    book: string,
    chapter: number,
    reached: number,
    seconds: number,
  ): Promise<{ completed: boolean; pct: number } | null>;
  /** Fetch the core's tuning once. */
  spec(): Promise<Partial<Spec> | null>;
  /** Called when a pass completes, so the shell can say so once. */
  onCompleted?(book: string, chapter: number): void;
}

/**
 * Start tracking. Returns a stop function that banks whatever is pending — the
 * tail of a reading session is real reading and must not be lost because it fell
 * between two ticks.
 */
export function startReadingTracker(src: ReadingTargetSource): () => void {
  let spec = { ...FALLBACK };
  void src.spec().then((s) => {
    if (s) spec = { ...spec, ...s };
  });

  let key = "";
  let onScreen = 0; // seconds this chapter has been up (for grace)
  let sinceInput = 0; // seconds since the reader last did anything (for idle)
  let pending = 0; // banked seconds not yet reported
  let stopped = false;

  /** Hand the banked seconds over for the chapter they were earned in. */
  function flush(book: string, chapter: number): void {
    const secs = pending;
    if (secs <= 0) return;
    pending = 0;
    void src.record(book, chapter, src.reached(), secs).then((out) => {
      if (out?.completed) src.onCompleted?.(book, chapter);
    });
  }

  // Remember which chapter the banked seconds belong to: the reader may have
  // moved on by the time they are flushed, and crediting them to the new
  // chapter would be simply wrong.
  let owner: { book: string; chapter: number } | null = null;

  const bump = (): void => {
    sinceInput = 0;
  };
  // Anything that means "a person is here". Passive so none of this can
  // interfere with scrolling.
  const opts = { passive: true, capture: true } as const;
  for (const ev of ["scroll", "pointerdown", "keydown", "wheel", "touchmove"])
    addEventListener(ev, bump, opts);

  const onHidden = (): void => {
    if (document.visibilityState === "hidden") {
      // Last chance to run: bank now. Coming back re-serves the grace period,
      // so a glance at another tab and back does not bank time.
      if (owner) flush(owner.book, owner.chapter);
      onScreen = 0;
      sinceInput = 0;
    }
  };
  addEventListener("visibilitychange", onHidden);
  // pagehide is the only one that reliably fires on mobile teardown.
  addEventListener("pagehide", onHidden);

  const timer = setInterval(() => {
    if (stopped || document.visibilityState === "hidden") return;
    const t = src.target();
    const nextKey = t ? `${t.book} ${t.chapter}` : "";
    if (nextKey !== key) {
      // Leaving a chapter: bank its tail before the counters reset.
      if (owner) flush(owner.book, owner.chapter);
      key = nextKey;
      owner = t ? { ...t } : null;
      onScreen = 0;
      sinceInput = 0;
      pending = 0;
      return;
    }
    if (!t) return;
    const step = SAMPLE_MS / 1000;
    onScreen += step;
    sinceInput += step;
    // Grace, then presence. Neither is a punishment: both exist so that time
    // nobody spent reading never becomes progress.
    if (onScreen < spec.graceSeconds) return;
    if (sinceInput > spec.idleSeconds) return;
    pending += step;
    if (pending >= spec.tickSeconds) flush(t.book, t.chapter);
  }, SAMPLE_MS);

  return () => {
    stopped = true;
    clearInterval(timer);
    for (const ev of ["scroll", "pointerdown", "keydown", "wheel", "touchmove"])
      removeEventListener(ev, bump, opts);
    removeEventListener("visibilitychange", onHidden);
    removeEventListener("pagehide", onHidden);
    if (owner) flush(owner.book, owner.chapter);
  };
}
