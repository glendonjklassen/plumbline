// The reading map's shell half: how long a chapter was really read.
//
// The dwell state machine — grace, idle, interaction reset, tail-banking, report
// cadence — lives in `core::reading::DwellTracker` behind one endpoint, so this
// file hardcodes no thresholds. It owns only what the core cannot know, having
// no clock and no window: that a second passed, that a person touched something,
// and which chapter was in front of them. A `book` of null means "nothing is
// being read" — a dialog, Present, a hidden tab, teardown.

/** One sample per second. The core clamps a step it does not believe, so a
 *  throttled background timer cannot bank an hour in one tick. */
const SAMPLE_MS = 1000;

export interface ReadingTargetSource {
  /** The chapter being read, or null when nothing is (a dialog is up, the
   *  reader is in Present, …). */
  target(): { book: string; chapter: number } | null;
  /** Deepest verse reached in the current chapter — the high-water mark the core
   *  pairs with dwell. */
  reached(): number;
  /** One sample into the core's tracker. Answers only when it banked a report,
   *  with the same verdict `readingRecord` gives; otherwise null. */
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
 * Start tracking. Returns a stop function that tells the core the reading ended,
 * so it can bank the tail rather than lose it between two reports.
 */
export function startReadingTracker(src: ReadingTargetSource): () => void {
  let stopped = false;
  // Set by any sign of a person, cleared by the sample that reports it, so the
  // core sees "was there interaction during this second?".
  let interacted = false;

  const bump = (): void => {
    interacted = true;
  };
  // Passive so none of this can interfere with scrolling.
  const opts = { passive: true, capture: true } as const;
  for (const ev of ["scroll", "pointerdown", "keydown", "wheel", "touchmove"])
    addEventListener(ev, bump, opts);

  /** One sample to the core; surface a completion if it banked one. */
  function sample(book: string | null, chapter: number, step: number): void {
    const was = interacted;
    interacted = false;
    void src.tick(book, chapter, src.reached(), step, was).then((out) => {
      if (out?.completed && out.book) src.onCompleted?.(out.book, out.chapter ?? chapter);
    });
  }

  const onHidden = (): void => {
    if (document.visibilityState !== "hidden") return;
    // A null book banks the tail and re-arms grace, so a glance at another tab
    // and back does not bank the time away.
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
