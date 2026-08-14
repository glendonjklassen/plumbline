// Reading history, as RUNS rather than as one line per chapter.
//
// The recents list is one entry per chapter opened, most-recent-first, so an
// evening spent reading Genesis 1, 2 and 3 filled the sheet with three lines
// that said almost the same thing and pushed everything else off the bottom.
// A run of contiguous chapters in one book is one thing the reader did, and it
// reads as one line: "Genesis 1–3".
//
// WHY THIS IS NOT IN THE CORE, where a view-model would normally live: the
// shells OWN this list. `Session.pushHistory` prepends to `config.history`
// locally on every navigation and the config only reaches the engine on a
// debounced save, so anything the core derived from it would be stale the
// moment the reader turned a page. The Android twin is `historySpans` in
// ui/StudyScreen.kt, and HistorySpansTest holds the two to the same rules.

export interface HistoryEntry {
  readonly book: string;
  readonly chapter: number;
}

export interface HistorySpan {
  readonly book: string;
  /** What a tap opens: the chapter of the run's MOST RECENT entry, which is
   *  where the reader actually was — not the lowest number in the span. */
  readonly open: number;
  lo: number;
  hi: number;
}

/**
 * Collapse each run of adjacent entries in the same book with contiguous
 * chapters into one span.
 *
 * ADJACENT IN THE LIST, not merely similar: `[Gen 3, John 1, Gen 2]` stays
 * three spans, because the reader went somewhere else in between and merging
 * across that would rewrite the order they did things in. Contiguity is checked
 * against either end of the run, so reading forwards (which lands in the list
 * as 3, 2, 1) and reading backwards both collapse.
 */
export function historySpans(history: readonly HistoryEntry[]): HistorySpan[] {
  const out: HistorySpan[] = [];
  for (const h of history) {
    const run = out[out.length - 1];
    if (run && run.book === h.book && (h.chapter === run.lo - 1 || h.chapter === run.hi + 1)) {
      run.lo = Math.min(run.lo, h.chapter);
      run.hi = Math.max(run.hi, h.chapter);
      continue;
    }
    out.push({ book: h.book, open: h.chapter, lo: h.chapter, hi: h.chapter });
  }
  return out;
}

/** "Genesis 1" for a single chapter, "Genesis 1–3" for a run. The dash is an
 *  EN DASH, as everywhere else a range is written in this app. */
export function spanLabel(span: HistorySpan, bookName: (id: string) => string): string {
  const name = bookName(span.book);
  return span.lo === span.hi ? `${name} ${span.lo}` : `${name} ${span.lo}–${span.hi}`;
}
