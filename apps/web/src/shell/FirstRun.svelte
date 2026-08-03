<script lang="ts">
  // First run: who is opening the Book? (product 2026-07-26, both shells)
  //  - New in the faith → a welcome from the maintainer with next steps
  //    (references tappable — they open BESIDE John), then lands in John 1
  //    with both analysis tiers off: just the text.
  //  - Sharing the gospel → straight into Present with the Romans Road,
  //    ready to hand across.
  //  - Established believer → the analysis-tier picker (with examples).
  //    The text is always on; tiers can be changed any time in Settings.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { cleanChurch, hasChurch, safeChurchUrl } from "./church";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  type Stage = "choose" | "welcome" | "curious" | "tiers" | "church";
  // A link shared from Present says who it was meant for, so the person
  // holding it is not asked to classify themselves (2026-07-27).
  let stage = $state<Stage>("choose");
  // Re-reading is the same page without the setup: no path is chosen, no
  // settings move, and the button at the bottom just closes it.
  const rereading = $derived(s.reopenIntro !== null);
  /** Whoever's church we know about — the link that brought this reader here,
   *  or the one already saved (so re-reading later still names them). */
  const fromChurch = $derived(hasChurch(s.sharedByChurch) ? s.sharedByChurch : s.church);
  $effect(() => {
    if (s.reopenIntro) stage = s.reopenIntro === "curious" ? "curious" : "welcome";
  });
  // Unchecked to begin with: the tiers are opt-in, so this screen ASKS rather
  // than confirming something already decided (2026-07-28).
  let human = $state(false);
  let machine = $state(false);

  // The home church, asked of the people likely to hand this on: the
  // established believer setting the app up, and whoever is about to walk
  // someone down the Romans Road (2026-07-27). Optional, and the screen says
  // plainly why it is collected — it travels in the links they share, and
  // nowhere else.
  let churchName = $state("");
  let churchInfo = $state("");
  let churchUrl = $state("");
  function saveChurchIfGiven(): void {
    const c = cleanChurch({ name: churchName, info: churchInfo, url: churchUrl });
    if (hasChurch(c)) s.setChurch(c);
  }

  // The welcome's verses (refKeys use OSIS book ids — canon.rs). The text is
  // WRITTEN HERE, not fetched: this screen is the first thing a new believer
  // sees, and asking the engine for ten verses one at a time made the quotes
  // pop in a beat after the page (feedback 2026-07-27). The 1769 text is
  // frozen, so a copy of thirteen verses cannot drift — each was taken
  // verbatim from data/kjv.jsonl, rendered exactly as Verse::body() does.
  interface Ref {
    label: string;
    book: string;
    chapter: number;
    verse: number;
    text: string;
  }
  const REF: Record<string, Ref> = {
    love: {
      label: "Romans 5:8", book: "Rom", chapter: 5, verse: 8,
      text: "But God commendeth his love toward us, in that, while we were yet sinners, Christ died for us.",
    },
    pure: {
      label: "Psalm 12:6–7", book: "Ps", chapter: 12, verse: 6,
      text:
        "The words of the LORD are pure words: as silver tried in a furnace of earth, purified seven times. " +
        "Thou shalt keep them, O LORD, thou shalt preserve them from this generation for ever.",
    },
    church: {
      label: "Hebrews 10:24–25", book: "Heb", chapter: 10, verse: 24,
      text:
        "And let us consider one another to provoke unto love and to good works: " +
        "Not forsaking the assembling of ourselves together, as the manner of some is; but exhorting one another: " +
        "and so much the more, as ye see the day approaching.",
    },
    heart: {
      label: "Psalm 119:11", book: "Ps", chapter: 119, verse: 11,
      text: "Thy word have I hid in mine heart, that I might not sin against thee.",
    },
    loved: {
      label: "John 3:16", book: "John", chapter: 3, verse: 16,
      text:
        "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him " +
        "should not perish, but have everlasting life.",
    },
    know: {
      label: "1 John 5:13", book: "1John", chapter: 5, verse: 13,
      text:
        "These things have I written unto you that believe on the name of the Son of God; that ye may know " +
        "that ye have eternal life, and that ye may believe on the name of the Son of God.",
    },
    kept: {
      label: "John 10:28–29", book: "John", chapter: 10, verse: 28,
      text:
        "And I give unto them eternal life; and they shall never perish, neither shall any man pluck them " +
        "out of my hand.",
    },
    perfected: {
      label: "Philippians 1:6", book: "Phil", chapter: 1, verse: 6,
      text:
        "Being confident of this very thing, that he which hath begun a good work in you will perform it " +
        "until the day of Jesus Christ:",
    },
    forgiven: {
      label: "1 John 1:9", book: "1John", chapter: 1, verse: 9,
      text:
        "If we confess our sins, he is faithful and just to forgive us our sins, and to cleanse us from all " +
        "unrighteousness.",
    },
    treasure: {
      label: "Proverbs 2:4–5", book: "Prov", chapter: 2, verse: 4,
      text:
        "If thou seekest her as silver, and searchest for her as for hid treasures; " +
        "Then shalt thou understand the fear of the LORD, and find the knowledge of God.",
    },
    unbelief: {
      label: "Mark 9:24", book: "Mark", chapter: 9, verse: 24,
      text:
        "And straightway the father of the child cried out, and said with tears, Lord, I believe; " +
        "help thou mine unbelief.",
    },
    ask: {
      label: "Matthew 7:7", book: "Matt", chapter: 7, verse: 7,
      text: "Ask, and it shall be given you; seek, and ye shall find; knock, and it shall be opened unto you:",
    },
    seek: {
      label: "Jeremiah 29:13", book: "Jer", chapter: 29, verse: 13,
      text: "And ye shall seek me, and find me, when ye shall search for me with all your heart.",
    },
    struggle: {
      label: "Psalm 34:18", book: "Ps", chapter: 34, verse: 18,
      text:
        "The LORD is nigh unto them that are of a broken heart; and saveth such as be of a contrite spirit.",
    },
    wisdom: {
      label: "2 Timothy 3:16–17", book: "2Tim", chapter: 3, verse: 16,
      text:
        "All scripture is given by inspiration of God, and is profitable for doctrine, for reproof, for " +
        "correction, for instruction in righteousness: That the man of God may be perfect, throughly " +
        "furnished unto all good works.",
    },
  };

  function finish(h: boolean, m: boolean): void {
    s.config.humanAnalysis = h;
    s.config.machineAnalysis = m;
    // studyMode round-trips for older readers of the shared config.
    s.config.studyMode = h || m ? "full" : "simple";
    s.showFirstRun = false;
    // Flush, not the debounced save: whoever picks a path and closes the app
    // straight away must not meet the intro again (the pagehide flush posts
    // to a worker that dies with the page).
    s.flushConfig();
    if (m) void s.ensureRnd();
  }

  const pane = (book: string, chapter: number, verse: number | null = null) => ({
    book,
    chapter,
    targetVerse: verse,
    pendingScroll: verse != null,
    scrollY: 0,
    back: [] as { book: string; chapter: number }[],
    fwd: [] as { book: string; chapter: number }[],
  });

  /** New-believer landing: John 1 — a tapped reference opens beside it
   *  (phones: the passage opens with John 1 one back-step away). */
  function openRef(ref: Ref): void {
    if (!rereading) {
      startInJohn(ref);
      return;
    }
    s.reopenIntro = null;
    s.navigate(s.activePane, ref.book, ref.chapter, ref.verse);
  }

  function startInJohn(ref?: Ref): void {
    // Remember which welcome they read: the top bar offers it again, and a
    // reader shouldn't have to reinstall to see it twice (2026-07-27).
    s.config.intro = stage === "curious" ? "curious" : "new";
    finish(false, false);
    if (ref && s.narrow) {
      const p = pane(ref.book, ref.chapter, ref.verse);
      p.back = [{ book: "John", chapter: 1 }];
      s.panes = [p];
    } else {
      s.panes = ref ? [pane("John", 1), pane(ref.book, ref.chapter, ref.verse)] : [pane("John", 1)];
    }
    s.activePane = 0;
    s.flushConfig(); // the landing panes must survive an immediate close too
  }

  /** Witnessing: ask for the church first — this is the reader most likely to
   *  hand the app to someone — then straight into the Romans Road. */
  function sharing(): void {
    stage = "church";
  }
  function toRomansRoad(): void {
    saveChurchIfGiven();
    finish(true, true);
    s.presentThreadName = "Romans Road";
    s.showPresent = true;
  }

  function dismiss(): void {
    if (rereading) {
      s.reopenIntro = null;
      return;
    }
    // A STRAY TAP MUST NOT END ONBOARDING (audit D-08).
    //
    // `finish()` closes first run for good — `showFirstRun = false`, flushed —
    // but `config.intro` is written only by `startInJohn()`. So a tap on the
    // backdrop of the chooser both answered a question nobody had answered AND
    // left `intro` null, which is what the top bar's Welcome button keys off
    // (`session.intro`, `Shell.svelte`). The welcome was then unreachable
    // forever: no way back short of erasing the reader's data.
    //
    // choose / tiers / church are QUESTIONS. A tap outside them is a miss, and a
    // miss answers nothing — the card stays.
    //
    // welcome / curious are READ-AND-GO. There is nothing left to answer, so a
    // tap there means "got it": record which welcome they read and land them in
    // John, exactly as the page's own Start button does.
    if (stage === "welcome" || stage === "curious") startInJohn();
  }
</script>

{#snippet refchip(r: Ref)}
  <button class="ref" onclick={() => openRef(r)} title={t("intro.openRef", { passage: r.label })}>{r.label}</button>
{/snippet}

{#snippet vquote(refs: Ref[])}
  <blockquote class="vq">
    <span class="vq-text">“{refs.map((r) => r.text).join(" ")}”</span>
    <span class="vq-refs">{#each refs as r (r.label)}{@render refchip(r)}{/each}</span>
  </blockquote>
{/snippet}

{#snippet sharedBy()}
  {#if hasChurch(s.sharedByChurch)}
    <!-- Someone handed this over. Say who, before anything else — a QR on a
         card at a service should lead back to that service. -->
    <div class="from-church">
      <span class="fc-lead">{t("intro.sharedBy")}</span>
      <span class="fc-name">{s.sharedByChurch.name}</span>
      {#if s.sharedByChurch.info}<span class="fc-info">{s.sharedByChurch.info}</span>{/if}
      {#if safeChurchUrl(s.sharedByChurch.url)}
        <a class="fc-url" href={safeChurchUrl(s.sharedByChurch.url)} target="_blank" rel="noopener noreferrer">
          {s.sharedByChurch.url}
        </a>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet churchFields()}
  <p class="ch-why">{t("intro.churchWhy")}</p>
  <input class="ch-field" placeholder={t("settings.churchName")} bind:value={churchName} />
  <input class="ch-field" placeholder={t("settings.churchInfo")} bind:value={churchInfo} />
  <input class="ch-field" placeholder={t("settings.churchUrl")} bind:value={churchUrl} />
{/snippet}

{#if s.showFirstRun || s.reopenIntro}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={dismiss}></div>
  <!-- Escape goes through `dismiss()`, the same path as a tap on the backdrop,
       so it inherits the D-08 rule for free: while this screen is ASKING
       something, a stray key answers nothing and the card stays. -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={t("intro.title")} use:modal={{ close: dismiss }}>
    {#if stage === "choose"}
      <h2>{t("intro.title")}</h2>
      {@render sharedBy()}
      <p class="sub">{t("intro.sub")}</p>
      <!-- Curious leads (2026-07-28): a stranger to the Bible is the likelier
           first-time reader of the two, and the path that asks the least of
           someone should be the one they see first. -->
      <button class="path" onclick={() => (stage = "curious")}>
        <span class="name">{t("intro.pathCurious")}</span>
        <span class="desc">{t("intro.pathCuriousDesc")}</span>
      </button>
      <button class="path" onclick={() => (stage = "welcome")}>
        <span class="name">{t("intro.pathNew")}</span>
        <span class="desc">{t("intro.pathNewDesc")}</span>
      </button>
      <!-- A link shared from Present was handed to someone in person, so it
           offers only the two paths it was meant for: the rest is setup for a
           reader who already has a Bible habit (2026-07-27). -->
      {#if !s.startAsNewBeliever}
      <button class="path" onclick={sharing}>
        <span class="name">{t("intro.pathSharing")}</span>
        <span class="desc">{t("intro.pathSharingDesc")}</span>
      </button>
      <button class="path" onclick={() => (stage = "tiers")}>
        <span class="name">{t("intro.pathEstablished")}</span>
        <span class="desc">{t("intro.pathEstablishedDesc")}</span>
      </button>
      {/if}
    {:else if stage === "welcome"}
      <h2>{t("intro.welcome.title")}</h2>
      {@render sharedBy()}
      <div class="welcome">
        <p>{t("intro.welcome.lead")}</p>
        <p><b>{t("intro.welcome.readLead")}</b> {t("intro.welcome.read")}</p>
        {@render vquote([REF.pure])}
        <p>
          <b>{t("intro.welcome.churchLead")}</b>
          {t("intro.welcome.church")}
          {#if hasChurch(fromChurch)}
            {t("intro.welcome.churchShared", {
              church: fromChurch.info ? `${fromChurch.name} — ${fromChurch.info}` : fromChurch.name,
            })}
            {#if safeChurchUrl(fromChurch.url)}
              <a class="ref-link" href={safeChurchUrl(fromChurch.url)} target="_blank" rel="noopener noreferrer">
                {t("intro.visitChurch", { church: fromChurch.name })}
              </a>
            {/if}
          {:else}
            {t("intro.welcome.churchNone")}
          {/if}
        </p>
        {@render vquote([REF.church])}
        <p><b>{t("intro.welcome.memorizeLead")}</b> {t("intro.welcome.memorize")}</p>
        {@render vquote([REF.heart])}
        <p>{t("intro.welcome.loved")}</p>
        {@render vquote([REF.love, REF.loved])}
        <p>{t("intro.welcome.kept")}</p>
        {@render vquote([REF.kept, REF.know])}
        <p>{t("intro.welcome.forgiven")}</p>
        {@render vquote([REF.perfected, REF.forgiven])}
        <p>{t("intro.welcome.wisdom")}</p>
        {@render vquote([REF.wisdom])}
        <p>{t("intro.welcome.struggle")}</p>
        {@render vquote([REF.struggle])}
        <p>{t("intro.welcome.blessing")}</p>
        <p class="hint">{t("intro.tapHint")}</p>
      </div>
      <button class="start" onclick={() => (rereading ? (s.reopenIntro = null) : startInJohn())}>
        {rereading ? t("common.close") : t("intro.open")}
      </button>
    {:else if stage === "curious"}
      <h2>{t("intro.curious.title")}</h2>
      {@render sharedBy()}
      <div class="welcome">
        <p>{t("intro.curious.p1")}</p>
        <p>{t("intro.curious.p2")}</p>
        {@render vquote([REF.loved])}
        <p>{t("intro.curious.p3")}</p>
        {@render vquote([REF.treasure])}
        <p>{t("intro.curious.p4")}</p>
        {@render vquote([REF.unbelief])}
        <p>{t("intro.curious.p5")}</p>
        {@render vquote([REF.ask, REF.seek])}
        <p>{t("intro.curious.struggle")}</p>
        {@render vquote([REF.struggle])}
        <p class="hint">{t("intro.tapHint")}</p>
      </div>
      <button class="start" onclick={() => (rereading ? (s.reopenIntro = null) : startInJohn())}>
        {rereading ? t("common.close") : t("intro.open")}
      </button>
    {:else if stage === "church"}
      <h2>{t("intro.beforeShare")}</h2>
      <p class="sub">{t("intro.beforeShareSub")}</p>
      {@render churchFields()}
      <button class="start" onclick={toRomansRoad}>{t("intro.openPresent")}</button>
      <button class="ch-skip" onclick={toRomansRoad}>{t("intro.skip")}</button>
    {:else}
      <h2>{t("intro.title")}</h2>
      <p class="ch-title">{t("intro.yourChurch")}</p>
      {@render churchFields()}
      <hr class="ch-rule" />
      <p class="sub">{t("intro.tiersSub")}</p>
      <label class="card">
        <input type="checkbox" bind:checked={human} />
        <span class="body">
          <span class="name">{t("settings.human")} <span class="mark human">†</span></span>
          <span class="desc">{t("intro.humanDesc")}</span>
        </span>
      </label>
      <label class="card">
        <input type="checkbox" bind:checked={machine} />
        <span class="body">
          <span class="name">{t("settings.machine")} <span class="mark machine">≈</span></span>
          <span class="desc">{t("intro.machineDesc")}</span>
        </span>
      </label>
      <p class="note">{t("intro.provenance")}</p>
      <button
        class="start"
        onclick={() => {
          saveChurchIfGiven();
          finish(human, machine);
        }}
      >{t("intro.start")}</button>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 40;
  }
  .dialog {
    position: fixed;
    z-index: 41;
    top: 10vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(540px, 94vw);
    max-height: 82vh;
    overflow-y: auto;
    /* The reader's scrollbars are hidden everywhere else; a grey gutter down
       the side of a welcome is the same eyesore (feedback 2026-07-27). */
    /* The reader's scrollbars are hidden everywhere else; a grey gutter down
       the side of a welcome is the same eyesore (feedback 2026-07-27). */
    scrollbar-width: none;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    padding: 22px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .dialog::-webkit-scrollbar {
    display: none;
  }
  .ref-link {
    color: var(--gold, #9e7d38);
    text-decoration: underline;
    white-space: nowrap;
  }
  h2 {
    font-size: calc(23px * var(--uiScale, 1));
    font-weight: 500;
    text-align: center;
  }
  .sub {
    font-size: calc(16px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    text-align: center;
  }
  .path {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    text-align: left;
    padding: 14px 16px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
  }
  .path:hover {
    border-color: var(--gold, #9e7d38);
  }
  .welcome {
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-size: calc(17px * var(--uiScale, 1));
    line-height: 1.55;
  }
  .vq {
    margin: -2px 6px 0;
    padding: 6px 12px;
    border-left: 2px solid var(--gold, #9e7d38);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .vq-text {
    font-family: "EB Garamond", Georgia, serif;
    font-size: calc(17.5px * var(--uiScale, 1));
    line-height: 1.5;
    font-style: italic;
  }
  .vq-refs {
    align-self: flex-end;
  }
  .ref {
    display: inline;
    color: var(--gold, #9e7d38);
    font-weight: 600;
    font-size: calc(15.5px * var(--uiScale, 1));
    padding: 0 2px;
  }
  .ref:hover {
    text-decoration: underline;
  }
  .ref + .ref::before {
    content: "· ";
    color: var(--faded, #8a8276);
    font-weight: 400;
  }
  .hint {
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    font-style: italic;
  }
  .ch-title {
    font-weight: 600;
    text-align: center;
  }
  .ch-why {
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    line-height: 1.45;
    text-align: center;
  }
  .ch-field {
    width: 100%;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    padding: 8px 10px;
    font-size: calc(17px * var(--uiScale, 1));
    box-sizing: border-box;
  }
  .ch-rule {
    border: none;
    border-top: 1px solid var(--rule, #d8cba8);
    width: 100%;
    margin: 4px 0 0;
  }
  .ch-skip {
    align-self: center;
    font-size: calc(15.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    text-decoration: underline;
  }
  .from-church {
    display: flex;
    flex-direction: column;
    gap: 2px;
    align-items: center;
    text-align: center;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 10px;
    padding: 10px 14px;
  }
  .fc-lead {
    font-size: calc(14px * var(--uiScale, 1));
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faded, #8a8276);
  }
  .fc-name {
    font-size: calc(19.5px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .fc-info {
    font-size: calc(15.5px * var(--uiScale, 1));
  }
  .fc-url {
    font-size: calc(15px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
    text-decoration: underline;
    word-break: break-all;
  }
  .card {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
    cursor: pointer;
  }
  .card:hover {
    border-color: var(--gold, #9e7d38);
  }
  .card input {
    margin-top: 4px;
    accent-color: var(--gold, #9e7d38);
    width: 17px;
    height: 17px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .name {
    font-weight: 600;
  }
  .mark.human {
    color: var(--tierHuman, #6f8f6a);
  }
  .mark.machine {
    color: var(--tierMachine, #999);
  }
  .desc {
    font-size: calc(15.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    line-height: 1.4;
  }
  .note {
    font-size: calc(14px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    text-align: center;
  }
  .start {
    align-self: center;
    padding: 8px 26px;
    background: var(--gold, #9e7d38);
    color: #fff;
    border-radius: 8px;
    font-size: calc(18.5px * var(--uiScale, 1));
  }
</style>
