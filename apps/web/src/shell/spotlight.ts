// The weekly feature tile (maintainer, 2026-10-29: "someone who uses this for
// years should be reminded … about all of the features we have"). Once a week,
// the bookmarks row carries one chip, a lightbulb beside a feature's short
// name; tapping it opens a card that says in a few plain sentences what the
// feature does and where it lives, with a button that goes there.
//
// The rules, as settled that day:
//   - It appears the first time the app is opened in a new week and stays until
//     Done, or until the week ends. A week is keyed by its local Monday.
//   - A feature the reader already uses is passed over: nobody needs the Tags
//     tile with twenty tags of their own. Features with no clean usage signal
//     (Search, Present, Hymnal …) always take their turn.
//   - After the last feature the cycle starts again from the top.
//
// The position lives in `config.spotlight` ({week, id, done}) so it rides in
// the backup and a restore does not start the cycle over. The pick itself is
// written in memory and reaches the home with the next ordinary save; Done
// saves at once (PlanChip.svelte says why there is no save at pick time). The
// copy is in the catalogue under `spotlight.<id>.{name,body,where}`.
import { Session } from "../state/session.svelte";
import { dayStamp } from "../engine/StudyEngine";

export interface SpotlightState {
  /** The local Monday of the week this tile was picked for, `YYYY-MM-DD`. */
  week: string;
  id: string;
  done: boolean;
}

/** `null` from a signal means its read has not answered yet; the pick waits. */
type Signal = (s: Session) => boolean | null;

export interface Feature {
  id: string;
  /** True when the reader plainly uses it already; the tile is then skipped. */
  used?: Signal;
  /** Where the card's Open button goes. */
  go: (s: Session) => void;
}

/** Whether a stamp is the reader's own work rather than the stock study set:
 *  later than the day the reader started (`readingBooks.since`). Stock files
 *  carry their repository dates, which precede any reader who installed after
 *  them; an early adopter's stock set reads as theirs, which only costs them a
 *  tile. */
function after(stamp: unknown, since: string): boolean {
  return typeof stamp === "string" && stamp.slice(0, 10) > since;
}

// `q`, never `qStale`: a stale answer is the one that predates the reader's
// write, and a pick made on it names the feature they just used. `q` answers
// null while the fresh read is in flight, and the pick waits.
function sinceOf(s: Session): string | null {
  const q = s.q("readingBooks", dayStamp());
  return q == null ? null : String(q.since ?? "");
}

/** A read that is still in flight answers null, and the pick waits for it. */
function list(s: Session, name: string, key: string, ...args: unknown[]): any[] | null {
  const q = s.q(name, ...args);
  return q == null ? null : (((q[key] ?? []) as any[]) ?? []);
}

const hasOwnTags: Signal = (s) => {
  const since = sinceOf(s);
  const tags = list(s, "tags", "tags");
  if (since == null || tags == null) return null;
  return tags.some((t) => after(t.created, since) || (t.members ?? []).some((m: any) => after(m.added, since)));
};
const ownThreads = (s: Session): any[] | null => {
  const since = sinceOf(s);
  const threads = list(s, "threads", "threads");
  if (since == null || threads == null) return null;
  return threads.filter((t) => after(t.created, since));
};
const hasOwnThreads: Signal = (s) => ownThreads(s)?.some(() => true) ?? null;
const hasOwnSermons: Signal = (s) => ownThreads(s)?.some((t) => (t.outline?.length ?? 0) > 0) ?? null;
const hasOwnWeaves: Signal = (s) => {
  const since = sinceOf(s);
  const weaves = list(s, "weaves", "weaves");
  if (since == null || weaves == null) return null;
  return weaves.some((w) => after(w.created, since));
};
const hasNotes: Signal = (s) => {
  const notes = list(s, "userNotes", "notes");
  return notes == null ? null : notes.length > 0;
};
const hasConceptStudy: Signal = (s) => {
  const running = list(s, "plans", "running", "");
  return running == null ? null : running.some((p) => p.kind === "conceptStudy");
};
const hasPlans: Signal = (s) => {
  const running = list(s, "plans", "running", "");
  const dev = s.devotionals();
  if (running == null || dev == null) return null;
  return running.some((p) => p.kind !== "conceptStudy") || ((dev.running ?? []) as any[]).length > 0;
};
const hasCards: Signal = (s) => {
  const cards = list(s, "memoryCoverage", "cards", dayStamp());
  return cards == null ? null : cards.length > 0;
};
/** The modernized-wording overlay is a reading aid over the King James, so it
 *  exists only where the corpus is the KJV (`akjvAvailable`, the same gate the
 *  Settings dialog uses). Where it does not exist, or is already on, the tile
 *  has nothing to say. */
const overlayOnOrAbsent: Signal = (s) => !s.akjvAvailable || !!s.config.akjvOverlay;

const notesBrowser = (s: Session): void => {
  s.screen = "explore";
  s.panel = { kind: "notesBrowser" };
};
const threads = (s: Session): void => {
  s.screen = "explore";
  s.panel = { kind: "threads" };
};

/** The cycle, in order. The manifest the maintainer reviewed on 2026-10-29. */
export const FEATURES: Feature[] = [
  { id: "wordStudy", go: (s) => s.goRead() },
  {
    id: "search",
    go: (s) => {
      // What the top bar's ⌕ does (Shell.openSearch): a fresh search, not the last one.
      s.searchScope = "all";
      s.searchMatch = Session.DEFAULT_SEARCH_MATCH;
      s.screen = "search";
    },
  },
  { id: "notes", used: hasNotes, go: notesBrowser },
  { id: "tags", used: hasOwnTags, go: (s) => (s.screen = "tags") },
  { id: "threads", used: hasOwnThreads, go: threads },
  { id: "sermons", used: hasOwnSermons, go: (s) => s.openSermons("explore") },
  { id: "weaves", used: hasOwnWeaves, go: (s) => (s.screen = "weaves") },
  { id: "conceptStudy", used: hasConceptStudy, go: (s) => (s.screen = "conceptStudy") },
  {
    id: "memorize",
    used: hasCards,
    go: (s) => {
      s.screen = "memorize";
      s.memorize = { view: "hub" };
    },
  },
  { id: "plans", used: hasPlans, go: (s) => (s.screen = "plans") },
  { id: "readingMap", go: (s) => (s.bookNavFor = s.activePane) },
  { id: "panes", go: (s) => s.goRead() },
  { id: "present", go: (s) => (s.screen = "preach") },
  { id: "share", go: (s) => (s.screen = "share") },
  { id: "hymnal", go: (s) => (s.screen = "hymnal") },
  { id: "backup", go: (s) => (s.showBackup = true) },
  { id: "modernized", used: overlayOnOrAbsent, go: (s) => (s.showSettings = true) },
  { id: "offline", go: (s) => (s.showSettings = true) },
  { id: "history", go: (s) => (s.showHistory = true) },
];

export const FEATURE_BY_ID: Record<string, Feature> = Object.fromEntries(FEATURES.map((f) => [f.id, f]));

/** The local Monday of the week holding `d`, as `YYYY-MM-DD`. Local, because
 *  the reader's week turns at their midnight, not UTC's. */
export function weekKey(d: Date = new Date()): string {
  const t = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  t.setDate(t.getDate() - ((t.getDay() + 6) % 7));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${t.getFullYear()}-${p(t.getMonth() + 1)}-${p(t.getDate())}`;
}

/** The next feature after `prevId` (the first, when there was none) that the
 *  reader does not already use — or null while a usage read is still in
 *  flight, so a week is never pinned to a tile the answer would have skipped.
 *  Wraps, and a cycle in which every feature is in use still yields the next
 *  one: better a known tile than none. */
export function nextFeature(prevId: string | null, s: Session): string | null {
  const start = prevId ? FEATURES.findIndex((f) => f.id === prevId) + 1 : 0;
  for (let k = 0; k < FEATURES.length; k++) {
    const f = FEATURES[(start + k) % FEATURES.length];
    // Three answers, kept apart: no signal (never used), a signal still in
    // flight (null — wait, do not pick), and a verdict. `?? false` here once
    // read "in flight" as "unused" and pinned the Notes tile on a reader with
    // notes (the spotlight spec caught it).
    const used = f.used ? f.used(s) : false;
    if (used === null) return null;
    if (!used) return f.id;
  }
  return FEATURES[start % FEATURES.length].id;
}
