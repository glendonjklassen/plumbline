<script lang="ts">
  // The STUDY hub, as its own SCREEN — the Android twin is ui/ExploreScreen.kt.
  // (File and screen id keep the Explore name; the role the bar sells is Study.)
  //
  // A destination should replace the reader, not hover over it.
  //
  // Every study tool with a sentence saying what it is, because "Suggested" and
  // "Constellation" mean nothing cold. Memorize is a card here, not a bar
  // destination: the bar carries the reader's ROLES (Read · Study · Preach ·
  // Share · Sing) and memorization is a study discipline.
  //
  // WHY THERE IS A BAND ABOVE THE CARDS (maintainer, 2026-08-13: "every time I
  // click study it just doesn't excite me… a bunch of boring brown cards, not
  // like the other pages"). The diagnosis was specific: this screen was eight
  // identical rectangles of FIXED text, so it looked the same on the day you
  // installed the app as after a year of study. Plans, by contrast, tells you
  // today's chapters; the Hymnal is full of actual hymns. They feel alive
  // because they carry STATE.
  //
  // So the hub now opens with what is actually in flight, and every card that
  // holds a collection says how big it has grown. Nothing here is a new engine
  // call: each number is a query some other screen already makes, arriving
  // through the same cache (`q`), so the hub costs a cache read and no round
  // trip once anything else has asked.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { dispatchLink } from "../study/links";
  import { lang, plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  // Midday, not now: the stamp is part of the query cache's KEY, so a clock
  // that ticks would mint a fresh entry per read. Same trick BookNav uses.
  const dayStamp = (): string => new Date().toISOString().slice(0, 10) + "T12:00:00Z";

  // ── what is in flight ───────────────────────────────────────────────────────
  const plans = $derived(s.q("plans", ""));
  const running = $derived((plans?.running ?? []) as any[]);
  const plan = $derived(running[0] ?? null);

  /** A run carries an id, not a name: the label is its builtin's translated
   *  `nameKey`, exactly as the Plans screen resolves it. */
  function planName(id: string): string {
    const b = ((plans?.builtins ?? []) as any[]).find((x) => x.id === id);
    return b ? t(b.nameKey) : id;
  }
  const dueCount = $derived(((s.q("memoryDue", dayStamp())?.refs ?? []) as string[]).length);
  const suggestedCount = $derived(((s.q("suggestedWeaves")?.suggested ?? []) as any[]).length);

  /** The chapters this plan wants today, as the Plans screen words it. A paused
   *  plan asks nothing, which is the whole point of pausing it. */
  const planToday = $derived.by(() => {
    if (!plan || plan.paused) return null;
    const chapters = (plan.today?.chapters ?? []) as any[];
    if (!chapters.length) return null;
    return t("plans.today", { chapters: chapters.map((c) => c.display).join(", ") });
  });

  /** The first chapter of today's reading — where "Today: …" goes. */
  const planFirstRef = $derived.by(() => {
    const c = plan?.today?.chapters?.[0];
    return c ? `${c.book} ${c.chapter}:1` : null;
  });

  // ── the reading map, as one number and one bar ──────────────────────────────
  //
  // CHAPTERS, not a word-weighted percentage: "412 of 1,189" is a thing a
  // reader can hold, and the map's own `read` count is exactly chapters that
  // have had a full pass. The bar is painted in the map's own `readDone` hue,
  // so it belongs to whichever of the eighteen themes is on, for free.
  const coverage = $derived.by(() => {
    const books = (s.q("readingBooks", dayStamp())?.books ?? []) as any[];
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
  function goToday(ev: MouseEvent): void {
    if (planFirstRef) void dispatchLink(s, goUri(planFirstRef), ev);
  }

  // The library tools, each with the count of what is IN it. Plans and Memorize
  // deliberately carry no count: they are activities rather than collections,
  // and the band above already says what they are asking for today.
  const cards = $derived([
    { id: "plans", count: null as number | null, go: openPlans },
    { id: "memorize", count: null as number | null, go: openMemorize },
    {
      id: "notes",
      count: ((s.q("userNotes")?.notes ?? []) as any[]).length,
      go: () => (s.panel = { kind: "notesBrowser" }),
    },
    {
      id: "threads",
      count: ((s.q("threads")?.threads ?? []) as any[]).length,
      go: () => (s.panel = { kind: "threads" }),
    },
    { id: "tags", count: ((s.q("tags")?.tags ?? []) as any[]).length, go: () => (s.panel = { kind: "tags" }) },
    {
      id: "weaves",
      count: ((s.q("weaves")?.weaves ?? []) as any[]).length,
      go: () => (s.panel = { kind: "weaves" }),
    },
    { id: "suggested", count: suggestedCount, go: () => (s.panel = { kind: "suggested" }) },
  ]);

  // The maps live under ONE card (maintainer UAT, 2026-08-12: the weave map
  // "should be one of N subitems of a visualization menu item") — two sibling
  // cards read as two more tools, when they are two views of the same thing.
  // That card is a DOOR, not a branch: it opens a page (shell/VizScreen.svelte)
  // the way Plans and Memorize do. It expanded in place at first, and the tree
  // was the odd one out in a shell where a destination replaces what came
  // before rather than unfolding inside it (maintainer, 2026-08-13).
</script>

<section class="screen" aria-label={t("nav.study")}>
  <ScreenBar title={t("nav.study")} onBack={() => s.goRead()} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <!-- IN PROGRESS. Only rows with something to say are drawn; a hub that
         listed "0 cards due · 0 to review" every day would be the same fixed
         text the cards already were. When nothing is running at all, the band
         is one invitation rather than an empty box. -->
    <section class="band" aria-label={t("explore.inProgress")}>
      <h3>{t("explore.inProgress")}</h3>
      {#if plan}
        <button class="row" onclick={(ev) => (planToday ? goToday(ev) : openPlans())}>
          <span class="row-name">{planName(plan.id)}</span>
          {#if plan.paused}
            <span class="row-note paused">{t("plans.pausedBadge")}</span>
          {:else if planToday}
            <span class="row-note">{planToday}</span>
          {/if}
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
      {#if !plan && dueCount === 0 && suggestedCount === 0}
        <button class="row invite" onclick={openPlans}>
          <span class="row-note">{t("explore.nothingRunning")}</span>
        </button>
      {/if}

      <!-- The visual bonus, and it is the reading map's own colour: how much of
           the canon has had a full pass. Tapping it opens the navigator, where
           the map itself lives. -->
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
    text-align: left;
    padding: 12px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    /* The band reads as the LIVE part of the screen: it sits on the reader's
       own paper while the tool cards below sit on the chrome's, so the two
       halves are told apart by depth rather than by a heading alone. */
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
  .row-note.paused {
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
    text-align: left;
    padding: 12px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
  }
  .coverage:hover {
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
    /* The map's two colours, but the TRACK has to read as empty when it is
       empty: at 22% the unread gold looked like a full bar on a reader who has
       read nothing, which is the opposite of what it says. Faint ground, and a
       hairline so the track still has an edge where the fill has not reached. */
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
    text-align: left;
    /* THE TAP FLOOR MUST NOT SQUASH THE TEXT. `min-height: 44px` (app.css,
       every button) REPLACES the automatic minimum size — the thing that
       otherwise stops a grid or flex item from being laid out shorter than its
       own content. With it in force the grid sized these rows below the
       two-line descriptions and the second line spilled out under the border,
       at every text scale. `auto` restores the content-driven minimum; the
       floor is still met by geometry (one 17px line + 32px of padding is 56px),
       so nothing here can be smaller than a thumb. */

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
  /* How much is in this tool. Absent at zero rather than shown as "0": an empty
     tool should read as quiet, not as a score of nought. */
  .ex-count {
    margin-left: 6px;
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
  /* The one card that opens a PAGE rather than a panel — the chevron says so,
     the same way a settings row leading somewhere does. */
  .ex-chevron {
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
  }
</style>
