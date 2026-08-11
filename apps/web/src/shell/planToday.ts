// The "today" view of the running schedule plans, shared by the two surfaces
// decision #5 puts in the reader's way: the nav-strip chip (PlanChip.svelte)
// and the passage navigator's today card (BookNav.svelte). Both read the same
// plans wire object (`q("plans", "")`), so the shaping lives once, here.

import { t } from "../lib/i18n.svelte";

export interface TodayChapter {
  book: string;
  chapter: number;
  display: string;
  read: boolean;
}

export interface TodayPlan {
  id: string;
  /** The plan's display name, resolved from the builtin catalogue's nameKey. */
  name: string;
  day: number;
  chapters: TodayChapter[];
}

/** Running schedules that still have a day to read, oldest-declared first.
 *  `today` is null once a plan is finished, so a finished plan simply drops
 *  out. Concept studies never appear — they have no days. */
export function todayPlans(plans: any): TodayPlan[] {
  const builtins = (plans?.builtins ?? []) as any[];
  const nameOf = (id: string): string => {
    const b = builtins.find((x) => x.id === id);
    return b ? t(b.nameKey) : id;
  };
  return ((plans?.running ?? []) as any[])
    .filter((p) => p.kind === "schedule" && p.today)
    .map((p) => ({ id: p.id, name: nameOf(p.id), day: p.today.day, chapters: p.today.chapters }));
}

/** Today's chapters, ranges collapsed the way the spec writes them:
 *  `Gen 30, Gen 31` → "Gen 30–31"; a book change starts a new group. Built
 *  from the wire's `display` names so it is right in German too. */
export function chapterSpan(chapters: TodayChapter[]): string {
  const groups: string[] = [];
  let start: TodayChapter | null = null;
  let prev: TodayChapter | null = null;
  const flush = (): void => {
    if (!start || !prev) return;
    groups.push(start === prev ? start.display : `${start.display}–${prev.chapter}`);
  };
  for (const c of chapters) {
    if (prev && c.book === prev.book && c.chapter === prev.chapter + 1) {
      prev = c;
      continue;
    }
    flush();
    start = prev = c;
  }
  flush();
  return groups.join(" · ");
}

/** Where a tap on a plan's today goes: its first unread chapter, or the first
 *  chapter when everything shown is read (the day is about to roll over;
 *  going to its start is still the plan's text). */
export function firstUnread(plan: TodayPlan): TodayChapter | null {
  return plan.chapters.find((c) => !c.read) ?? plan.chapters[0] ?? null;
}
