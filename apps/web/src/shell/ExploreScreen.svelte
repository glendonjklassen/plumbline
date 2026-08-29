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
  import { dayStamp, localDay } from "../engine/StudyEngine";
  import { chapterSpan, firstUnread, remaining, todayPlans } from "./planToday";
  import { lang, plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  // Midday, not now: the stamp is part of the query cache's KEY, so a clock
  // that ticks would mint a fresh entry per read. Same trick BookNav uses.

  // ── what is in flight ───────────────────────────────────────────────────────
  //
  // EVERY running plan gets its own row, in order, each naming the chapters it
  // still wants (maintainer, 2026-08-13). This read goes through `todayPlans`
  // rather than into `running` directly, which is the whole reason that module
  // exists — the chip and the navigator's today card already share it, and
  // reaching past it here got all four of its rules wrong at once: concept
  // studies are not schedules and have no day (their id would have rendered
  // raw, since they are not builtins), a paused plan asks nothing, a finished
  // one has dropped out, and only the FIRST plan was ever shown.
  // The four reads the band is built from, held once so readiness can be told
  // from emptiness. `q` answers null while its fetch is in flight, and null and
  // "nothing running" render identically — so without this the band drew as
  // empty for a frame or two and then GREW, shoving the cards down the page
  // ("they pop in on load so it's a bit jarring", maintainer, 2026-08-13).
  // `qStale`, not `q`: every authoring write and dwell tick invalidates the
  // cache, and a hub opened inside that window redrew empty and popped back
  // one answer at a time — the cards shifting under the reader's thumb
  // ("widgets are spazzy on load", UAT 2026-08-18). A held count self-corrects
  // the moment the fresh answer lands; nothing here aims a tap by ordinal, so
  // a beat of staleness costs nothing.
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
   *  working ahead is invited, not merely permitted (UAT, 2026-08-18) — and
   *  this line above it is the acknowledgment, because saying nothing to
   *  someone who just finished is a little insulting (maintainer, 2026-08-13). */
  const anyDoneToday = $derived(todays.some((p) => p.doneToday));

  const dueCount = $derived(((dueQ?.refs ?? []) as string[]).length);
  const suggestedCount = $derived(((suggestedQ?.suggested ?? []) as any[]).length);

  // ── the reading map, as one number and one bar ──────────────────────────────
  //
  // CHAPTERS, not a word-weighted percentage: "412 of 1,189" is a thing a
  // reader can hold, and the map's own `read` count is exactly chapters that
  // have had a full pass. The bar is painted in the map's own `readDone` hue,
  // so it belongs to whichever of the eighteen themes is on, for free.
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

  // ── the lifetime counter ────────────────────────────────────────────────────
  //
  // How many times this reader has been through the whole Bible. Seeded ONCE by
  // hand — somebody arriving with thirty years behind them should not start at
  // nought — and EARNED after that: nothing here edits it, and the only thing
  // that moves it is finishing the canon (maintainer, 2026-08-13).
  //
  // -1 is "never said", which is deliberately not 0: a reader who answers "none"
  // has told us something, and must not be asked again.
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
    // Asked once. There is no edit path afterwards on purpose: a number you can
    // retype is a number that means nothing.
    if (readsSet) return;
    const n = await s.askNumber(t("explore.readsAsk"));
    if (n === null) return;
    s.config.bibleReads = n;
    // Whatever the canon says right now is the state this answer was given
    // against, so a reader who is ALREADY finished is not immediately credited
    // with a read they just told us about.
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
  // deliberately carry no count: they are activities rather than collections,
  // and the band above already says what they are asking for today.
  const cards = $derived([
    // ONE card for devotionals AND reading plans (maintainer, 2026-08-26): they
    // were two cards onto the same screen, which is a distinction the reader
    // pays for and the app does not keep. First in the grid, and a DOOR rather
    // than a shortcut into today's entry — the screen behind it also has to
    // offer starting a second booklet and stopping this one.
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
    // A DOOR now, like Visualizations: there is more than one thing to do with
    // a tag library (browse, rename, merge) and a card that raised the panel
    // directly had nowhere to put the rest.
    { id: "tags", count: ((s.qStale("tags")?.tags ?? []) as any[]).length, go: () => (s.screen = "tags") },
    // A door for the same reason: the weave library and its suggested-review
    // queue are two views of one collection, and they were two sibling cards
    // out here (maintainer, 2026-08-19). The in-progress band above still
    // surfaces a pending review count directly.
    {
      id: "weaves",
      count: ((s.qStale("weaves")?.weaves ?? []) as any[]).length,
      go: () => (s.screen = "weaves"),
    },
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
      {#if !showReal}
        <!-- A PLACEHOLDER OF THE SAME SHAPE, not a spinner. The band's job here
             is to hold its own height. ONE row and the coverage strip is what
             the band resolves to in the common cases — a reader with one plan
             running, and a reader with none (who gets the invitation row) — so
             the cards below start where they will stay. Sized generously it
             was worse, not better: two ghost rows made the grid jump 49px UP
             when the real band turned out shorter. Hidden from assistive tech;
             there is nothing here to read. -->
        <div class="skeleton" aria-hidden="true">
          <div class="row ghost"></div>
          <!-- The reads line is in the settled band for EVERY reader — as the
               counter once set, as the "how many times" invitation before —
               so the skeleton owes its height too, or the grid still jumped
               one row when the real band landed. -->
          <div class="reads ghost"></div>
          <div class="coverage ghost"></div>
        </div>
      {:else}
        <div class="settled">
          {#if anyDoneToday}
            <div class="row done"><span class="row-note">{t("explore.planDone")}</span></div>
          {/if}
          <!-- Every running plan, ALWAYS with its next portion — after a
               finished day this is the next day's chapters, day-numbered so
               the reader can see the day was banked and keep going. -->
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

          <!-- The visual bonus, and it is the reading map's own colour: how much
               of the canon has had a full pass. Tapping it opens the navigator,
               where the map itself lives. -->
          <!-- The lifetime counter, beside the coverage bar it belongs with:
               one says how far through this pass you are, the other how many
               passes there have been. Unset, it is an invitation; set, it is a
               statement and not a control. -->
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
  /* A ghost is the row's own box with no content: same height, same radius,
     same rule — a shape settling rather than a thing arriving. */
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
  /* The real content arrives by fading UP, never by pushing: the placeholder it
     replaces was the same height. */
  .settled {
    animation: settle 0.18s ease-out both;
  }
  @keyframes settle {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  /* A reader who has asked for less motion gets none of it — the content simply
     is there. The point was never the animation; it was not moving the page. */
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
  /* A quiet statement, not a control — once set it does nothing when tapped,
     and it should not invite one. */
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
  /* A TAP TARGET, and the only one on this row: 44px is the floor the rest of
     the chrome keeps (e2e/touch-targets.spec.ts). `min-height: auto` undid a
     card floor this no longer inherits and left it at 41 — and the test caught
     it only on the runs where the hub's real content had replaced the
     placeholder before it measured, which is why it read as flaky rather than
     as the plain violation it is. */
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
    text-align: start;
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
    /* The cards themselves are static text and are on screen from the first
       frame; only their counts wait on a query, so they fade in rather than
       snapping. Inline, so nothing moves when they land. */
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
  /* The one card that opens a PAGE rather than a panel — the chevron says so,
     the same way a settings row leading somewhere does. */
  .ex-chevron {
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
  }
</style>
