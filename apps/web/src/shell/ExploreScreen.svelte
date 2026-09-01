<script lang="ts">
  // The STUDY hub as its own screen. (The file and screen id keep the Explore
  // name; the role the bar sells is Study.) Every tool carries a sentence saying
  // what it is, because "Suggested" and "Constellation" mean nothing cold.
  //
  // The band above the cards is what is in flight, and every collection card says
  // how big it has grown — otherwise the screen is fixed text that looks the same
  // after a year of study as on install day. No new engine calls: each number is
  // a query some other screen already makes, arriving through the same cache.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { dispatchLink } from "../study/links";
  import { dayStamp, localDay } from "../engine/StudyEngine";
  import { chapterSpan, firstUnread, remaining, todayPlans } from "./planToday";
  import { lang, plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  // `dayStamp()` is midday, not now: the stamp is part of the query cache's key,
  // so a ticking clock would mint a fresh entry per read.
  //
  // The four reads the band is built from, held once so readiness can be told
  // from emptiness — `q` answers null while a fetch is in flight, and null and
  // "nothing running" render identically, so the band would draw empty and then
  // grow, shoving the cards down the page. `qStale`, not `q`, for the same
  // reason: every authoring write and dwell tick invalidates the cache. A held
  // count self-corrects when the fresh answer lands, and nothing here aims a tap
  // by ordinal, so a beat of staleness costs nothing. Plan rows go through
  // `todayPlans` (concept studies have no day, paused plans ask nothing).
  const plansQ = $derived(s.qStale("plans", ""));
  const dueQ = $derived(s.qStale("memoryDue", dayStamp()));
  const suggestedQ = $derived(s.qStale("suggestedWeaves"));
  const booksQ = $derived(s.qStale("readingBooks", dayStamp()));

  /** Every band read has answered. Not a spinner's flag — it decides whether the
   *  band draws its real rows or a placeholder of the same shape, so the page
   *  never changes height under the reader's thumb. */
  const ready = $derived(plansQ != null && dueQ != null && suggestedQ != null && booksQ != null);

  /** A read that never answers must not leave a skeleton on screen forever: one
   *  failed query would otherwise strand the whole band. After this the band
   *  shows whatever it has, which for a failed read is the honest empty state. */
  let waited = $state(false);
  $effect(() => {
    const timer = setTimeout(() => (waited = true), 3_000);
    return () => clearTimeout(timer);
  });
  const showReal = $derived(ready || waited);

  const todays = $derived(todayPlans(plansQ));
  /** Running devotionals with an entry still on offer today. Retirement is the
   *  difference from a plan row above: a devotional is one entry a day, so a
   *  banked day leaves the band until tomorrow rather than rolling straight on
   *  to the next portion. */
  const devotionals = $derived(
    (((s.qStale("devotionals", lang(), localDay())?.running ?? []) as any[]) ?? []).filter(
      (r) => !r.paused && r.today?.available,
    ),
  );
  /** A full plan-day was banked today. The row still shows the NEXT portion —
   *  working ahead is invited — and this line above it is the acknowledgment. */
  const anyDoneToday = $derived(todays.some((p) => p.doneToday));

  const dueCount = $derived(((dueQ?.refs ?? []) as string[]).length);
  const suggestedCount = $derived(((suggestedQ?.suggested ?? []) as any[]).length);

  // The reading map as one number and one bar. CHAPTERS, not a word-weighted
  // percentage: "412 of 1,189" is a thing a reader can hold, and the map's own
  // `read` count is exactly chapters that have had a full pass. Painted in the
  // map's `readDone` hue, so it follows whichever theme is on.
  const coverage = $derived.by(() => {
    const books = (booksQ?.books ?? []) as any[];
    if (!books.length) return null;
    let read = 0;
    let total = 0;
    for (const b of books) {
      read += Number(b.read ?? 0);
      total += Number(b.chapters ?? 0);
    }
    return total > 0 ? { read, total, frac: read / total } : null;
  });

  const nf = $derived(new Intl.NumberFormat(lang()));

  // The lifetime counter: how many times this reader has been through the whole
  // Bible. Seeded ONCE by hand and earned after that — the only thing that moves
  // it is finishing the canon. -1 is "never said", deliberately not 0: a reader
  // who answers "none" has told us something and must not be asked again.
  const reads = $derived(Number(s.config.bibleReads ?? -1));
  const readsSet = $derived(reads >= 0);

  /** Crediting a finished canon, exactly once. `bibleReadsCredited` marks the
   *  CURRENT complete state as counted, and is cleared if the map ever drops
   *  below full — so the number moves on finishing, not on every visit to this
   *  screen afterwards. */
  $effect(() => {
    if (!showReal || !readsSet || !coverage) return;
    const complete = coverage.read >= coverage.total && coverage.total > 0;
    const credited = s.config.bibleReadsCredited === true;
    if (complete && !credited) {
      s.config.bibleReads = reads + 1;
      s.config.bibleReadsCredited = true;
      s.saveConfig();
    } else if (!complete && credited) {
      s.config.bibleReadsCredited = false;
      s.saveConfig();
    }
  });

  async function setReads(): Promise<void> {
    // Asked once; no edit path afterwards, on purpose.
    if (readsSet) return;
    const n = await s.askNumber(t("explore.readsAsk"));
    if (n === null) return;
    s.config.bibleReads = n;
    // The canon's current state is what this answer was given against, so a
    // reader already finished is not credited with the read they just reported.
    s.config.bibleReadsCredited = !!coverage && coverage.total > 0 && coverage.read >= coverage.total;
    s.saveConfig();
  }

  function openPlans(): void {
    s.screen = "plans";
  }
  function openMemorize(): void {
    s.screen = "memorize";
    s.memorize = { view: "hub" };
  }
  /** refKey → the core's `go:` verb, split on the LAST space, as core `go_uri`
   *  does. Same helper PlansScreen and MemorizeHost carry. */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;
  /** Straight to the first chapter of this plan's day that is still unread —
   *  the same target the nav-strip chip takes. */
  function goPlan(plan: (typeof todays)[number], ev: MouseEvent): void {
    const c = firstUnread(plan);
    if (c) void dispatchLink(s, goUri(`${c.book} ${c.chapter}:1`), ev);
  }

  // The library tools, each with the count of what is IN it. Plans and Memorize
  // carry no count: they are activities rather than collections, and the band
  // above already says what they ask for today.
  const cards = $derived([
    // One card for devotionals AND reading plans — they open the same screen.
    // A door rather than a shortcut into today's entry: that screen also offers
    // starting a second booklet and stopping this one.
    { id: "plans", count: null as number | null, go: openPlans },
    { id: "memorize", count: null as number | null, go: openMemorize },
    {
      id: "notes",
      count: ((s.qStale("userNotes")?.notes ?? []) as any[]).length,
      go: () => (s.panel = { kind: "notesBrowser" }),
    },
    {
      id: "threads",
      count: ((s.qStale("threads")?.threads ?? []) as any[]).length,
      go: () => (s.panel = { kind: "threads" }),
    },
    // A door: there is more than one thing to do with a tag library (browse,
    // rename, merge) and a card raising the panel directly fits only one.
    { id: "tags", count: ((s.qStale("tags")?.tags ?? []) as any[]).length, go: () => (s.screen = "tags") },
    // A door for the same reason: the weave library and its suggested-review
    // queue are two views of one collection. The band above still surfaces the
    // pending review count directly.
    {
      id: "weaves",
      count: ((s.qStale("weaves")?.weaves ?? []) as any[]).length,
      go: () => (s.screen = "weaves"),
    },
  ]);

  // The maps live under ONE card, which is a door onto shell/VizScreen.svelte
  // rather than something that unfolds in place: a destination replaces what
  // came before.
</script>

<section class="screen" aria-label={t("nav.study")}>
  <ScreenBar title={t("nav.study")} onBack={() => s.goRead()} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <!-- Only rows with something to say are drawn; with nothing running at all
         the band is one invitation rather than an empty box. -->
    <section class="band" aria-label={t("explore.inProgress")}>
      <h3>{t("explore.inProgress")}</h3>
      {#if !showReal}
        <!-- A placeholder of the same shape, not a spinner: the band's job is to
             hold its own height. One row plus the coverage strip is what it
             resolves to in the common cases, so the cards below start where they
             will stay — two ghost rows made the grid jump 49px UP instead.
             Hidden from assistive tech; there is nothing here to read. -->
        <div class="skeleton" aria-hidden="true">
          <div class="row ghost"></div>
          <!-- The reads line is in the settled band for every reader (counter or
               invitation), so the skeleton owes its height too. -->
          <div class="reads ghost"></div>
          <div class="coverage ghost"></div>
        </div>
      {:else}
        <div class="settled">
          {#if anyDoneToday}
            <div class="row done"><span class="row-note">{t("explore.planDone")}</span></div>
          {/if}
          <!-- Every running plan, always with its next portion: after a finished
               day this is the next day's chapters, day-numbered. -->
          {#each todays as p (p.id)}
            <button class="row" onclick={(ev) => goPlan(p, ev)}>
              <span class="row-name">{p.name}</span>
              <span class="row-note">{t("plans.chip", { day: p.day, chapters: chapterSpan(remaining(p)) })}</span>
            </button>
          {/each}
          {#each devotionals as d (d.id)}
            <button class="row" onclick={() => s.openDevotional(d.id, d.today.day)}>
              <span class="row-name">{d.name}</span>
              <span class="row-note">{t("devotional.dayOf", { day: d.today.day, total: d.daysTotal })}</span>
            </button>
          {/each}
          {#if dueCount > 0}
            <button class="row" onclick={openMemorize}>
              <span class="row-name">{t("explore.memorize")}</span>
              <span class="row-note">{plural("memorize.reviews.one", "memorize.reviews.other", dueCount)}</span>
            </button>
          {/if}
          {#if suggestedCount > 0}
            <button class="row" onclick={() => (s.panel = { kind: "suggested" })}>
              <span class="row-name">{t("explore.suggested")}</span>
              <span class="row-note">{plural("explore.toReview.one", "explore.toReview.other", suggestedCount)}</span>
            </button>
          {/if}
          {#if todays.length === 0 && devotionals.length === 0 && dueCount === 0 && suggestedCount === 0}
            <button class="row invite" onclick={openPlans}>
              <span class="row-note">{t("explore.nothingRunning")}</span>
            </button>
          {/if}

          <!-- The lifetime counter, beside the coverage bar: one says how far
               through this pass you are, the other how many passes there have
               been. Unset it is an invitation; set it is a statement, not a
               control. The bar itself opens the navigator, where the map lives. -->
          {#if readsSet}
            <div class="reads"><span class="reads-n">{nf.format(reads)}</span>
              <span class="reads-label">{plural("explore.readsTimes.one", "explore.readsTimes.other", reads)}</span>
            </div>
          {:else}
            <button class="reads unset" onclick={setReads}>
              <span class="reads-label">{t("explore.readsSet")}</span>
            </button>
          {/if}

          {#if coverage}
            <button class="coverage" onclick={() => (s.bookNavFor = s.activePane)}>
              <span class="cov-text">
                {t("explore.chaptersRead", { read: nf.format(coverage.read), total: nf.format(coverage.total) })}
              </span>
              <span class="cov-bar" aria-hidden="true">
                <span class="cov-fill" style="width: {(coverage.frac * 100).toFixed(2)}%"></span>
              </span>
            </button>
          {/if}
        </div>
      {/if}
    </section>

    <div class="grid">
      {#each cards as c (c.id)}
        <button class="ex-card" onclick={c.go}>
          <span class="ex-name">
            {t(`explore.${c.id}`)}
            {#if c.count !== null && c.count > 0}<span class="ex-count">{nf.format(c.count)}</span>{/if}
          </span>
          <span class="ex-desc">{t(`explore.${c.id}.desc`)}</span>
        </button>
      {/each}
      <button class="ex-card" onclick={() => (s.screen = "viz")}>
        <span class="ex-name">{t("explore.viz")} <span class="ex-chevron">›</span></span>
        <span class="ex-desc">{t("explore.viz.desc")}</span>
      </button>
    </div>
  </div>
</section>

<style>
  .screen {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  /* The band spans the width; the tools grid flows under it. */
  .band {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* The placeholder and the real thing are the same stack, so swapping one for
     the other moves nothing sideways. */
  .skeleton,
  .settled {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* A ghost is the row's own box with no content: same height, radius, rule. */
  .ghost {
    background: color-mix(in srgb, var(--ink, #211f1a) 4%, var(--paper, #fcf9f4));
    animation: breathe 1.6s ease-in-out infinite;
  }
  .row.ghost {
    min-height: calc(46px * var(--uiScale, 1));
  }
  .coverage.ghost {
    min-height: calc(56px * var(--uiScale, 1));
  }
  .reads.ghost {
    min-height: calc(34px * var(--uiScale, 1));
  }
  @keyframes breathe {
    0%, 100% { opacity: 0.55; }
    50% { opacity: 0.85; }
  }
  /* The real content fades up rather than pushing: the placeholder it replaces
     was the same height. */
  .settled {
    animation: settle 0.18s ease-out both;
  }
  @keyframes settle {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  /* Reduced motion: the content is simply there. The point was never the
     animation, it was not moving the page. */
  @media (prefers-reduced-motion: reduce) {
    .ghost {
      animation: none;
    }
    .settled {
      animation: none;
    }
  }
  .band h3 {
    margin: 0 0 2px;
    font-size: calc(12px * var(--uiScale, 1));
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--section, #776537);
  }
  .row {
    min-height: auto;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
    text-align: start;
    padding: 12px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    /* The band sits on the reader's own paper while the tool cards below sit on
       the chrome's, so the two halves are told apart by depth. */
    background: var(--paper, #fcf9f4);
  }
  .row:hover {
    border-color: var(--gold, #9e7d38);
  }
  .row-name {
    font-size: calc(16px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .row-note {
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
  }
  .row.done .row-note {
    color: var(--faded, #8a8276);
  }
  .invite .row-note {
    color: var(--faded, #8a8276);
  }
  .coverage {
    min-height: auto;
    display: flex;
    flex-direction: column;
    gap: 7px;
    text-align: start;
    padding: 12px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
  }
  .coverage:hover {
    border-color: var(--gold, #9e7d38);
  }
  /* A quiet statement, not a control: once set, a tap does nothing. */
  .reads {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 10px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
    text-align: start;
  }
  .reads-n {
    font-size: calc(22px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .reads-label {
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  /* A tap target: 44px is the floor the rest of the chrome keeps
     (e2e/touch-targets.spec.ts). Restated because the `min-height: auto` above
     removes the card floor this would otherwise inherit. */
  .reads.unset {
    min-height: 44px;
    align-items: center;
    width: 100%;
  }
  .reads.unset:hover {
    border-color: var(--gold, #9e7d38);
  }
  .cov-text {
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .cov-bar {
    display: block;
    height: 8px;
    border-radius: 999px;
    /* The map's two colours, but the track must read as empty when it is: at 22%
       the unread gold looked like a full bar. Faint ground plus a hairline, so
       the track still has an edge where the fill has not reached. */
    background: color-mix(in srgb, var(--readUnread, #c9a227) 12%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--rule, #d8cba8) 60%, transparent);
    overflow: hidden;
  }
  .cov-fill {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--readDone, #6f8f6a);
  }
  .grid {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    align-content: start;
  }
  .ex-card {
    min-height: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: start;
    /* The tap floor must not squash the text: app.css's `min-height: 44px`
       REPLACES the automatic minimum size, so the grid sized these rows below
       their two-line descriptions and the second line spilled under the border.
       `auto` restores the content-driven minimum; the floor is still met by
       geometry (a 17px line + 32px padding is 56px). */

    padding: 16px 18px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--popupPaper, #f2eee6);
  }
  .ex-card:hover {
    border-color: var(--gold, #9e7d38);
  }
  .ex-name {
    font-size: calc(17px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  /* Absent at zero rather than shown as "0". */
  .ex-count {
    /* The cards are on screen from the first frame; only the counts wait on a
       query, so they fade in. Inline, so nothing moves when they land. */
    animation: settle 0.18s ease-out both;
    margin-inline-start: 6px;
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--gold, #9e7d38);
    background: color-mix(in srgb, var(--gold, #9e7d38) 13%, transparent);
    border-radius: 999px;
    padding: 1px 8px;
  }
  .ex-desc {
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.4;
    color: var(--faded, #8a8276);
  }
  /* The one card that opens a page rather than a panel; the chevron says so. */
  .ex-chevron {
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
  }
</style>
