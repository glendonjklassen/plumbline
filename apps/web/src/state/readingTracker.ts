// The reading map's shell half on the web: how long a chapter was really read.
//
// The dwell state machine lives in the core. Grace, idle, the interaction
// reset, the tail-banking and the report cadence all live in
// `core::reading::DwellTracker`, reached through one endpoint — so this shell
// and `ui/ReadingTracker.kt` each make one `readingTick` call per second rather
// than hardcoding the thresholds they claim to fetch.
//
// The three refusals the core enforces:
//
//   * a GRACE period before anything accrues, so paging through to find
//     something never credits what it flew past;
//   * an IDLE cutoff, so a tab left open does not read Leviticus overnight;
//   * HIDDEN stops the clock — a backgrounded tab is not being read.
//
// This file now owns only what the core cannot know, having no clock and no
// window: that a second passed, that a person touched something, and which
// chapter was in front of them. A `book` of null is how it says "nothing is
// being read", which covers a dialog, Present, a hidden tab and teardown alike.

/** One sample per second: fine enough for grace and idle to land accurately,
 *  and nowhere near often enough to matter. The core clamps a step it does not
 *  believe, so a throttled background timer cannot bank an hour in one tick. */
const SAMPLE_MS = 1000;

export interface ReadingTargetSource {
  /** The chapter being read, or null when nothing is (a dialog is up, the
   *  reader is in Present, …). */
  target(): { book: string; chapter: number } | null;
  /** Deepest verse reached in the current chapter — the high-water mark the core
   *  pairs with dwell. Scrolling without time credits nothing, and time without
   *  scrolling credits only what was on screen. */
  reached(): number;
  /** One sample into the core's tracker. It answers only when it decided to
   *  bank a report, in which case the answer is the same verdict
   *  `readingRecord` gives; otherwise null. */
  tick(
    book: string | null,
    chapter: number,
    reached: number,
    stepSeconds: number,
    interacted: boolean,
  ): Promise<{ completed: boolean; pct: number; book?: string; chapter?: number } | null>;
  /** Called when a pass completes, so the shell can say so once. */
  onCompleted?(book: string, chapter: number): void;
}

/**
 * Start tracking. Returns a stop function that tells the core the reading has
 * ended, so it can bank the tail — the last stretch of a session is real reading
 * and must not be lost because it fell between two reports.
 */
export function startReadingTracker(src: ReadingTargetSource): () => void {
  let stopped = false;
  // Set by any sign of a person and cleared by the sample that reports it, so
  // the core sees "was there interaction during this second?" rather than a
  // count this side would have to interpret.
  let interacted = false;

  const bump = (): void => {
    interacted = true;
  };
  // Anything that means "a person is here". Passive so none of this can
  // interfere with scrolling.
  const opts = { passive: true, capture: true } as const;
  for (const ev of ["scroll", "pointerdown", "keydown", "wheel", "touchmove"])
    addEventListener(ev, bump, opts);

  /** Hand one sample to the core and surface a completion if it banked one. */
  function sample(book: string | null, chapter: number, step: number): void {
    const was = interacted;
    interacted = false;
    void src.tick(book, chapter, src.reached(), step, was).then((out) => {
      if (out?.completed && out.book) src.onCompleted?.(out.book, out.chapter ?? chapter);
    });
  }

  const onHidden = (): void => {
    if (document.visibilityState !== "hidden") return;
    // Last chance to run. A null book banks the tail and re-arms the grace
    // period, so a glance at another tab and back does not bank the time away.
    sample(null, 0, 0);
  };
  addEventListener("visibilitychange", onHidden);
  // pagehide is the only one that reliably fires on mobile teardown.
  addEventListener("pagehide", onHidden);

  const timer = setInterval(() => {
    if (stopped || document.visibilityState === "hidden") return;
    const t = src.target();
    sample(t?.book ?? null, t?.chapter ?? 0, SAMPLE_MS / 1000);
  }, SAMPLE_MS);

  return () => {
    stopped = true;
    clearInterval(timer);
    for (const ev of ["scroll", "pointerdown", "keydown", "wheel", "touchmove"])
      removeEventListener(ev, bump, opts);
    removeEventListener("visibilitychange", onHidden);
    removeEventListener("pagehide", onHidden);
    sample(null, 0, 0);
  };
}
