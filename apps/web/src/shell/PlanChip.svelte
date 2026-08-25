<script lang="ts">
  // The nav-strip BOOKMARKS pager, grown out of the plan chip (docs/READING-PLANS.md
  // decision #5; bookmarks per maintainer ask, 2026-08-24). One row above the
  // canon strip, swipeable between tiles: the running plan's "Day 12 ·
  // Gen 30–31" first, then every stored seating bookmark — Last opened, Sunday
  // morning, Sunday evening, Wednesday evening (`config.slots`,
  // core::session_slot). Each tile carries an icon naming its kind; a tap
  // toasts which bookmark it is and where it goes, then navigates the active
  // pane there.
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
  const lead = $derived(plans[0] ?? null);

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
    // flag — the running reading plan
    plan: "M14.4 6L14 4H5v17h2v-7h5.6l.4 2h7V6z",
  };

  /** The seating tiles: Last opened first (the everyday position), then the
   *  named seatings, in the order a week meets them. Only seatings the reader
   *  has actually been in exist in `config.slots`, so nothing is invented. */
  const SLOT_ORDER: { token: string; key: string }[] = [
    { token: "other", key: "bookmarks.lastOpened" },
    { token: "sunday-morning", key: "bookmarks.sundayMorning" },
    { token: "sunday-evening", key: "bookmarks.sundayEvening" },
    { token: "wednesday-evening", key: "bookmarks.wednesdayEvening" },
  ];

  function passageName(book: string, chapter: number, verse?: number | null): string {
    const name = s.q("toc")?.books?.find((b: any) => b.id === book)?.name ?? book;
    return verse && verse > 1 ? `${name} ${chapter}:${verse}` : `${name} ${chapter}`;
  }

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

  function goPlan(): void {
    const target = lead && firstUnread(lead);
    if (!target) return;
    s.showToast(t("bookmarks.going", { name: t("bookmarks.plan") }));
    s.navigate(s.activePane, target.book, target.chapter);
  }

  function goSlot(tile: (typeof slotTiles)[number]): void {
    // The toast names the BOOKMARK, nothing else — the destination is on
    // screen the moment the pane lands there (maintainer, 2026-08-24: a
    // "going to…" sentence was noise).
    s.showToast(t("bookmarks.going", { name: tile.label }));
    s.navigate(s.activePane, tile.book, tile.chapter, tile.verse);
  }
</script>

{#if (lead || slotTiles.length > 0) && !s.inConceptStudy}
  <div class="plan-chip-row">
    <div class="tiles">
      {#if lead}
        <div class="tile">
          <button class="plan-chip" onclick={goPlan} title={t("plans.chipGo")}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d={ICONS.plan} /></svg>
            {t("plans.chip", { day: lead.day, chapters: chapterSpan(remaining(lead)) })}
          </button>
          {#if plans.length > 1}
            <button class="plan-chip more" onclick={() => (s.screen = "plans")}>
              {t("plans.chipMore", { n: plans.length - 1 })}
            </button>
          {/if}
        </div>
      {/if}
      {#each slotTiles as tile (tile.token)}
        <div class="tile">
          <button class="bm-tile" data-slot={tile.token} onclick={() => goSlot(tile)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d={ICONS[tile.token]} /></svg>
            {tile.label} · {tile.passage}
          </button>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  /* A quiet pill row just above the canon strip: present, not campaigning —
     the reading-map philosophy (an invitation, not a debt). */
  .plan-chip-row {
    padding: 4px 8px;
    background: var(--paneNavBg, #efeae1);
    border-top: 1px solid var(--rule, #d8cba8);
  }
  /* The pager: one tile per page, swipe (or trackpad-scroll) between them.
     92% basis leaves the next tile peeking, which is what says "swipeable"
     without spending a row of dots. */
  .tiles {
    display: flex;
    overflow-x: auto;
    scroll-snap-type: x mandatory;
    scrollbar-width: none;
    gap: 6px;
  }
  .tiles::-webkit-scrollbar {
    display: none;
  }
  .tile {
    flex: 0 0 92%;
    scroll-snap-align: center;
    display: flex;
    justify-content: center;
    gap: 6px;
    min-width: 0;
  }
  .tile:only-child {
    flex-basis: 100%;
  }
  .plan-chip,
  .bm-tile {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 999px;
    background: var(--paper, #fcf9f4);
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
  }
  .plan-chip svg,
  .bm-tile svg {
    width: calc(14px * var(--uiScale, 1));
    height: calc(14px * var(--uiScale, 1));
    flex: 0 0 auto;
    fill: currentColor;
  }
  .plan-chip:hover,
  .bm-tile:hover {
    border-color: var(--gold, #9e7d38);
  }
  .plan-chip.more {
    color: var(--faded, #8a8276);
    font-weight: 400;
    flex: 0 0 auto;
  }
</style>
