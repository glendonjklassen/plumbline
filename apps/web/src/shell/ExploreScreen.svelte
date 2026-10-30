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
  import { dateLocale, lang, plural, t } from "../lib/i18n.svelte";

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

  // Times through the Bible and each part of it, counted by the engine from
  // per-chapter reads: a part has been read as many times as its least-read
  // chapter. The parts are only bundles of books — nothing is stored per part.
  const sections = $derived(((booksQ?.sections ?? []) as any[]) ?? []);
  const bible = $derived(sections.find((x) => x.id === "bible"));

  // The reading map as one number and one bar. CHAPTERS, not a word-weighted
  // percentage: "412 of 1,189" is a thing a reader can hold. It is the time
  // through under way — chapters read more often than the least-read one — so
  // it empties again after each finish instead of sitting full for good. Painted
  // in the map's `readDone` hue, so it follows whichever theme is on.
  const coverage = $derived.by(() => {
    if (!bible || !bible.chapters) return null;
    const read = Number(bible.ahead ?? 0);
    const total = Number(bible.chapters);
    return { read, total, frac: read / total };
  });

  const nf = $derived(new Intl.NumberFormat(lang()));

  // The finish-date forecast, worked out in the core from the map (`reading::forecast`):
  // the words read through since the earliest full read, projected over what is left.
  // Absent until a chapter has been read through, and once every chapter has.
  const forecast = $derived((booksQ?.forecast ?? null) as { start: string; finish: string } | null);

  /** "30 Jul 2026" from the wire's `YYYY-MM-DD`, in the reader's language. Parsed
   *  by parts: `new Date("2026-07-30")` is UTC midnight, which is the 29th in the
   *  Americas. */
  function dayLabel(ymd: string): string {
    const [y, m, d] = ymd.split("-").map(Number);
    if (!y || !m || !d) return ymd;
    return new Date(y, m - 1, d).toLocaleDateString(dateLocale(), { day: "numeric", month: "short", year: "numeric" });
  }

  // The lifetime counter: how many times this reader has been through the whole
  // Bible. `bibleReads` is what they had read BEFORE the map's own count, asked
  // once and added to every part alike; everything after is the map's. -1 is
  // "never said", deliberately not 0: a reader who answers "none" has told us
  // something and must not be asked again.
  const base = $derived(Number(s.config.bibleReads ?? -1));
  const readsSet = $derived(base >= 0);
  const reads = $derived(Math.max(0, base) + Number(bible?.reads ?? 0));

  async function setReads(): Promise<void> {
    // Asked once; no edit path afterwards, on purpose.
    if (readsSet) return;
    const n = await s.askNumber(t("explore.readsAsk"));
    if (n === null) return;
    // The answer is the whole count today, so whatever the map has already
    // counted is part of it, not added on top.
    s.config.bibleReads = Math.max(0, n - Number(bible?.reads ?? 0));
    s.saveConfig();
  }

  // The parts, folded under the counter: a tap opens them. Remembered on this
  // device only — a per-viewer convenience, so a failure to store is harmless.
  const OPEN_KEY = "plumbline:sectionsOpen";
  let sectionsOpen = $state(readOpen());
  function readOpen(): boolean {
    try {
      return localStorage.getItem(OPEN_KEY) === "1";
    } catch {
      return false;
    }
  }
  function toggleSections(): void {
    sectionsOpen = !sectionsOpen;
    try {
      localStorage.setItem(OPEN_KEY, sectionsOpen ? "1" : "0");
    } catch {
      // Private window or blocked storage: it simply starts closed next time.
    }
  }

  function openPlans(): void {
    s.screen = "plans";
  }
  function openMemorize(): void {
    s.screen = "memorize";
    s.memorize = { view: "hub" };
  }
  function openConceptStudy(): void {
    s.screen = "conceptStudy";
  }
  /** The concept study the reader is IN, for the band: the run behind
   *  `config.conceptStudy`, with its sweep count. Null outside the mode — a
   *  run merely on file is not in progress, it is on the Concept Study screen. */
  const activeStudy = $derived.by(() => {
    if (!s.inConceptStudy) return null;
    return (((plansQ?.running ?? []) as any[]).find((p) => p.id === s.conceptStudyId) as any) ?? null;
  });
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
    // Its own door, not a section of Plans (maintainer, 2026-10-29): a sweep for
    // a theme is a study activity in its own right, with its own reader mode.
    { id: "conceptStudy", count: null as number | null, go: openConceptStudy },
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
    // The sermon editor: a thread written top to bottom as headings, points
    // and passages. Counts the threads that have that shape.
    {
      id: "sermons",
      count: ((s.qStale("threads")?.threads ?? []) as any[]).filter((t) => (t.outline?.length ?? 0) > 0).length,
      go: () => s.openSermons("explore"),
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
          <!-- The sweep under way: a tap returns to the text, where the sweep
               happens. Its tag and chapter count are what the banner says. -->
          {#if activeStudy}
            <button class="row" onclick={() => s.goRead()}>
              <span class="row-name">{t("plans.conceptStudyTag", { tag: activeStudy.tag })}</span>
              <span class="row-note">
                {t("plans.sweepProgress", { done: activeStudy.sweepProgress[0], total: activeStudy.sweepProgress[1] })}
              </span>
            </button>
          {/if}
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
          {#if todays.length === 0 && devotionals.length === 0 && !activeStudy && dueCount === 0 && suggestedCount === 0}
            <button class="row invite" onclick={openPlans}>
              <span class="row-note">{t("explore.nothingRunning")}</span>
            </button>
          {/if}

          <!-- The lifetime counter, beside the coverage bar: one says how far
               through this time you are, the other how many times there have
               been. Unset it is an invitation; set, a tap folds out the same
               count for each part of the Bible. The bar itself opens the
               navigator, where the map lives. -->
          {#if readsSet}
            <button class="reads" aria-expanded={sectionsOpen} onclick={toggleSections}>
              <span class="reads-n">{nf.format(reads)}</span>
              <span class="reads-label">{plural("explore.readsTimes.one", "explore.readsTimes.other", reads)}</span>
              <svg class="reads-fold" class:open={sectionsOpen} viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
            {#if sectionsOpen}
              <ul class="sections" aria-label={t("explore.sectionsTitle")}>
                {#each sections.filter((x) => x.id !== "bible" && x.chapters > 0) as sec (sec.id)}
                  <li class="section" class:child={sec.depth > 0}>
                    <span class="section-name">{sec.name}</span>
                    <span class="section-n">{nf.format(base + Number(sec.reads))}</span>
                    <span class="section-next">
                      {t("explore.sectionNext", { ahead: nf.format(sec.ahead), total: nf.format(sec.chapters) })}
                    </span>
                  </li>
                {/each}
              </ul>
            {/if}
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
              {#if forecast}
                <span class="cov-forecast">{t("explore.finishForecast", { date: dayLabel(forecast.finish) })}</span>
                <span class="cov-pace">{t("explore.finishPace", { start: dayLabel(forecast.start) })}</span>
              {/if}
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
  /* The count, and the fold for each part's count under it. */
  .reads {
    width: 100%;
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
  /* Both forms are tap targets now: 44px is the floor the rest of the chrome
     keeps (e2e/touch-targets.spec.ts), restated because the band's other cards
     set `min-height: auto`. */
  .reads {
    min-height: 44px;
  }
  .reads.unset {
    align-items: center;
  }
  .reads:hover {
    border-color: var(--gold, #9e7d38);
  }
  .reads-fold {
    margin-inline-start: auto;
    align-self: center;
    flex: none;
    width: calc(18px * var(--uiScale, 1));
    height: calc(18px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    transition: transform 0.15s ease-out;
  }
  .reads-fold.open {
    transform: rotate(180deg);
  }
  /* Each part as a row: name, count, and the way into the next time through.
     Testaments at the margin, their parts set in under them. */
  .sections {
    list-style: none;
    margin: -4px 0 0;
    padding: 6px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
  }
  .section {
    display: grid;
    grid-template-columns: 1fr auto;
    column-gap: 10px;
    padding: 7px 0;
  }
  .section + .section {
    border-top: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 55%, transparent);
  }
  .section.child {
    padding-inline-start: 16px;
  }
  .section-name {
    font-size: calc(15px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .section.child .section-name {
    font-weight: 500;
  }
  .section-n {
    font-size: calc(16px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--gold, #9e7d38);
    font-variant-numeric: tabular-nums;
  }
  .section-next {
    grid-column: 1 / -1;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
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
  /* The forecast under the bar: the date in ink, its basis quieter. */
  .cov-forecast {
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
  }
  .cov-pace {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    margin-top: -3px;
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
