<script lang="ts">
  // The nav-strip BOOKMARKS row, grown out of the plan chip (docs/READING-PLANS.md
  // decision #5; bookmarks per maintainer ask, 2026-08-24). One row above the
  // canon strip of pill chips, each an ICON naming the bookmark's kind beside
  // WHAT IT HOLDS: a flag per running plan ("Genesis 30–31" — what is left of
  // today), a booklet per running devotional ("Day 4" — today's entry, and
  // gone once it is read), then one per stored seating — Sunday morning
  // (`config.slots`, core::session_slot) — reading "Psalms 23:4".
  //
  // Icon + passage, no more (maintainer, 2026-08-25, second turn). The chips
  // went icon-only that morning; the reference came back the same day — a row
  // of four glyphs said WHICH bookmark but not WHERE, and where is the reason
  // to tap. The kind's NAME stays off the face: it rides the aria-label/title
  // ("Sunday morning · Psalms 23:4") and the toast a tap raises ("Sunday
  // morning bookmark"). Several are visible at once — centred while they fit,
  // left-anchored and scrolling the moment they don't, so a chip cut at the
  // edge is what says "more". (It began as a one-tile-per-page pager carrying
  // the kind's name AND the passage; one at a time hid that there were others.)
  //
  // NOT shown in concept-study mode — any of it: the tracker is suspended
  // there, so a chip inviting schedule reading would promise credit the mode
  // deliberately withholds, and a bookmark yanking the reader out of a sweep
  // is a mode exit nobody asked for.
  import { getSession } from "../state/session.svelte";
  import { chapterSpan, firstUnread, remaining, todayPlans } from "./planToday";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  // HOLD the last answer through a refetch. `q()` answers null while a
  // refetch is in flight, and the plans read is now invalidated by every
  // ~30-second dwell report (session.onReadingWrote) and every mark-read
  // write — so rendering `q()` raw made this whole row (border and padding
  // included) unmount for the in-flight frames and remount when the answer
  // landed: a repeating jitter at the bottom of the screen, worst at the
  // moment a chapter completed (the UAT report, 2026-08-11). Only a REAL
  // answer may change what the chip shows; the gap between answers may not.
  let held: any[] = [];
  const plans = $derived.by(() => {
    void s.studyEpoch;
    const q = s.q("plans", "");
    if (q == null) return held;
    // The chip does NOT retire when a day's worth is read: finishing day 12
    // advances `today` to day 13, and the chip shows it — "the next X verses to
    // read", so a reader can work ahead (UAT, 2026-08-18; this reverses the
    // 2026-08-12 stand-down, which readers met as being told to stop). The day
    // number moving is what says the day's worth was banked.
    return (held = todayPlans(q));
  });

  // Material icon paths (24×24), matching the NAV table's idiom.
  const ICONS: Record<string, string> = {
    // history — the everyday "last opened" position
    other:
      "M13 3a9 9 0 0 0-9 9H1l3.89 3.89.07.14L9 12H6c0-3.87 3.13-7 7-7s7 3.13 7 7-3.13 7-7 7c-1.93 0-3.68-.79-4.94-2.06l-1.42 1.42A8.954 8.954 0 0 0 13 21a9 9 0 0 0 0-18zm-1 5v5l4.28 2.54.72-1.21-3.5-2.08V8z",
    // wb_sunny — Sunday morning
    "sunday-morning":
      "M6.76 4.84l-1.8-1.79-1.41 1.41 1.79 1.79 1.42-1.41zM4 10.5H1v2h3v-2zm9-9.95h-2V3.5h2V.55zm7.45 3.91l-1.41-1.41-1.79 1.79 1.41 1.41 1.79-1.79zm-3.21 13.7l1.79 1.8 1.41-1.41-1.8-1.79-1.4 1.4zM20 10.5v2h3v-2h-3zm-8-5c-3.31 0-6 2.69-6 6s2.69 6 6 6 6-2.69 6-6-2.69-6-6-6zm-1 16.95h2V19.5h-2v2.95zm-7.45-3.91l1.41 1.41 1.79-1.8-1.41-1.41-1.79 1.8z",
    // nightlight_round — Sunday evening
    "sunday-evening":
      "M12 3a9 9 0 1 0 9 9c0-.46-.04-.92-.1-1.36a5.389 5.389 0 0 1-4.4 2.26 5.403 5.403 0 0 1-3.14-9.8c-.44-.06-.9-.1-1.36-.1z",
    // group — the midweek meeting
    "wednesday-evening":
      "M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z",
    // flag — a running reading plan
    plan: "M14.4 6L14 4H5v17h2v-7h5.6l.4 2h7V6z",
    // auto_stories — a running devotional, the booklet it is
    devotional:
      "M19 1l-5 5v11l5-4.5V1zM1 6v14.65c0 .25.25.5.5.5.1 0 .15-.05.25-.05C3.1 20.45 5.05 20 6.5 20c1.95 0 4.05.4 5.5 1.5V6c-1.45-1.1-3.55-1.5-5.5-1.5S2.45 4.9 1 6zm22 13.5V6c-.6-.45-1.25-.75-2-1v13.5c-1.1-.35-2.3-.5-3.5-.5-1.7 0-4.15.65-5.5 1.5v2c1.35-.85 3.8-1.5 5.5-1.5 1.65 0 3.35.3 4.75 1.05.1.05.15.05.25.05.25 0 .5-.25.5-.5v-1.1z",
  };

  /** The seating tiles: the named seatings, in the order a week meets them.
   *  Only seatings the reader has actually been in exist in `config.slots`, so
   *  nothing is invented.
   *
   *  Three of the four are OFF for now — Sunday evening and Wednesday evening
   *  because a row of four was more bookmarks than the strip wanted to carry,
   *  and LAST OPENED because it was never a bookmark in the first place
   *  (maintainer, both 2026-08-26): the app already reopens where the reader
   *  left off, so a chip for it named the place they were already standing and
   *  changed every time they turned a page. Nothing else changes:
   *  `core::session_slot` still recognises all three, the engine still stores a
   *  seating for each, and the three lines below are all it takes to bring any
   *  of them back with every reader's position intact. Their icons stay in
   *  [[ICONS]] for the same reason. */
  const SLOT_ORDER: { token: string; key: string }[] = [
    // { token: "other", key: "bookmarks.lastOpened" },
    { token: "sunday-morning", key: "bookmarks.sundayMorning" },
    // { token: "sunday-evening", key: "bookmarks.sundayEvening" },
    // { token: "wednesday-evening", key: "bookmarks.wednesdayEvening" },
  ];

  /** "Psalms 23:4" — the TOC's own book name, as everywhere else the web names
   *  a verse (reader/refname.ts). There is no abbreviation table in the core or
   *  the catalogues, and one written here would be a second copy of 66 names
   *  in three languages; the row scrolls when the full names outgrow it. */
  function passageName(book: string, chapter: number, verse?: number | null): string {
    const name = s.q("toc")?.books?.find((b: any) => b.id === book)?.name ?? book;
    return verse && verse > 1 ? `${name} ${chapter}:${verse}` : `${name} ${chapter}`;
  }

  /** One chip per running schedule — where "+{n} more" used to hang off the
   *  first one — each carrying its own day label, its own first-unread target,
   *  and on its face what is LEFT of today ("Genesis 30–31"). The label is
   *  what a screen reader hears and what a desktop tooltip shows. */
  const planTiles = $derived(
    plans.flatMap((p: any, i: number) => {
      const target = firstUnread(p);
      if (!target) return [];
      const passage = chapterSpan(remaining(p));
      return [
        {
          key: String(p.id ?? i),
          label: t("plans.chip", { day: p.day, chapters: passage }),
          passage,
          target,
        },
      ];
    }),
  );

  /** One chip per running devotional with a day still on offer.
   *
   *  The retirement rule is the DIFFERENCE from the plan chip above, and it is
   *  deliberate: a plan chip keeps showing the next portion so a reader can
   *  work ahead, but a devotional is one entry a day (maintainer, 2026-08-26).
   *  So the chip goes the moment the day is banked and returns at the next
   *  local midnight — which is exactly what `today.available` answers, computed
   *  in the core against the reader's OWN local date. A paused booklet asks
   *  nothing, and a finished one has no `today` at all. */
  const devotionalTiles = $derived(
    (((s.devotionals()?.running ?? []) as any[]) ?? []).flatMap((r: any) => {
      if (r.paused || !r.today?.available) return [];
      return [{ key: String(r.id), id: r.id as string, day: r.today.day as number, name: r.name as string }];
    }),
  );

  const slotTiles = $derived.by(() => {
    const slots = (s.config.slots ?? {}) as Record<string, any>;
    return SLOT_ORDER.flatMap(({ token, key }) => {
      const seat = slots[token];
      if (!seat?.book) return [];
      return [
        {
          token,
          label: t(key),
          passage: passageName(seat.book, seat.chapter, seat.verse),
          book: seat.book as string,
          chapter: Number(seat.chapter),
          verse: seat.verse && seat.verse > 1 ? Number(seat.verse) : null,
        },
      ];
    });
  });

  function goPlan(tile: (typeof planTiles)[number]): void {
    s.showToast(t("bookmarks.going", { name: t("bookmarks.plan") }));
    s.navigate(s.activePane, tile.target.book, tile.target.chapter);
  }

  function goDevotional(tile: (typeof devotionalTiles)[number]): void {
    s.showToast(t("bookmarks.going", { name: tile.name }));
    s.openDevotional(tile.id, tile.day);
  }

  function goSlot(tile: (typeof slotTiles)[number]): void {
    // The toast names the BOOKMARK, nothing else — the kind's name is not on
    // the chip, so this is the confirmation of which one was pressed; the
    // destination is on the face and on screen the moment the pane lands.
    s.showToast(t("bookmarks.going", { name: tile.label }));
    s.navigate(s.activePane, tile.book, tile.chapter, tile.verse);
  }
</script>

{#if (planTiles.length > 0 || devotionalTiles.length > 0 || slotTiles.length > 0) && !s.inConceptStudy}
  <div class="plan-chip-row">
    <div class="tiles">
      {#each planTiles as p (p.key)}
        <button class="plan-chip" aria-label={p.label} title={p.label} onclick={() => goPlan(p)}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d={ICONS.plan} /></svg>
          <span class="ref">{p.passage}</span>
        </button>
      {/each}
      {#each devotionalTiles as d (d.key)}
        <button
          class="plan-chip"
          data-devotional={d.id}
          aria-label="{d.name} · {t('devotional.chip', { day: d.day })}"
          title="{d.name} · {t('devotional.chip', { day: d.day })}"
          onclick={() => goDevotional(d)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d={ICONS.devotional} /></svg>
          <span class="ref">{t("devotional.chip", { day: d.day })}</span>
        </button>
      {/each}
      {#each slotTiles as tile (tile.token)}
        <button
          class="bm-tile"
          data-slot={tile.token}
          aria-label="{tile.label} · {tile.passage}"
          title="{tile.label} · {tile.passage}"
          onclick={() => goSlot(tile)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d={ICONS[tile.token]} /></svg>
          <span class="ref">{tile.passage}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  /* A quiet row of pill chips just above the canon strip: present, not
     campaigning — the reading-map philosophy (an invitation, not a debt). */
  .plan-chip-row {
    padding: 5px 8px;
    background: var(--paneNavBg, #efeae1);
    border-top: 1px solid var(--rule, #d8cba8);
  }
  .tiles {
    display: flex;
    gap: 10px;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 0 2px;
  }
  .tiles::-webkit-scrollbar {
    display: none;
  }
  /* Centred while they fit, left-anchored the moment they don't: auto margins
     on the two ends absorb the free space, and under overflow there is none to
     absorb — which is what keeps the first chip reachable by scrolling. */
  .tiles > :first-child {
    margin-inline-start: auto;
  }
  .tiles > :last-child {
    margin-inline-end: auto;
  }
  .plan-chip,
  .bm-tile {
    flex: 0 0 auto;
    /* The 44px touch floor (app.css), scaled with the chrome; the pill grows
       sideways with its passage and never wraps it. */
    height: calc(44px * var(--uiScale, 1));
    min-width: calc(44px * var(--uiScale, 1));
    padding: 0 calc(14px * var(--uiScale, 1)) 0 calc(11px * var(--uiScale, 1));
    border-radius: 999px;
    border: 1px solid var(--rule, #d8cba8);
    background: var(--paper, #fcf9f4);
    color: var(--gold, #9e7d38);
    display: inline-flex;
    align-items: center;
    gap: calc(6px * var(--uiScale, 1));
    white-space: nowrap;
  }
  .plan-chip svg,
  .bm-tile svg {
    flex: 0 0 auto;
    width: calc(20px * var(--uiScale, 1));
    height: calc(20px * var(--uiScale, 1));
    fill: currentColor;
  }
  /* The passage in ink beside the gold glyph: the icon is the accent, the
     words are what the eye reads. */
  .ref {
    color: var(--ink, #211f1a);
    font-size: calc(13.5px * var(--uiScale, 1));
    font-variant-numeric: tabular-nums;
  }
  .plan-chip:hover,
  .bm-tile:hover {
    border-color: var(--gold, #9e7d38);
  }
</style>
