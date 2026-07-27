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
  import { cleanChurch, hasChurch, safeChurchUrl } from "./church";

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
  let human = $state(true);
  let machine = $state(true);

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
    // Clicking away on the chooser/tiers keeps the old behaviour (defaults);
    // the welcome page asks for an explicit choice, and so does the church
    // step — dismissing it would silently skip a question just asked.
    if (stage !== "welcome" && stage !== "church") finish(human, machine);
  }
</script>

{#snippet refchip(r: Ref)}
  <button class="ref" onclick={() => openRef(r)} title="Open {r.label}">{r.label}</button>
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
      <span class="fc-lead">Shared with you by</span>
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
  <p class="ch-why">
    Optional. If you add your church, the links and QR codes you share carry it, so whoever you
    hand the Bible to can also find your church. It stays on your device otherwise — nothing is
    sent anywhere.
  </p>
  <input class="ch-field" placeholder="Church name" bind:value={churchName} />
  <input class="ch-field" placeholder="When and where — e.g. Sundays 10am, 12 Long Street" bind:value={churchInfo} />
  <input class="ch-field" placeholder="Website" bind:value={churchUrl} />
{/snippet}

{#if s.showFirstRun || s.reopenIntro}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={dismiss}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    {#if stage === "choose"}
      <h2>Welcome to Plumbline</h2>
      {@render sharedBy()}
      <p class="sub">The Holy Bible, free and offline. Where would you like to begin?</p>
      <button class="path" onclick={() => (stage = "welcome")}>
        <span class="name">New in the faith</span>
        <span class="desc">I've just put my faith in Jesus — where do I start?</span>
      </button>
      <button class="path" onclick={() => (stage = "curious")}>
        <span class="name">Curious about the Bible</span>
        <span class="desc">I'm not sure what I believe — where do I start?</span>
      </button>
      <!-- A link shared from Present was handed to someone in person, so it
           offers only the two paths it was meant for: the rest is setup for a
           reader who already has a Bible habit (2026-07-27). -->
      {#if !s.startAsNewBeliever}
      <button class="path" onclick={sharing}>
        <span class="name">Sharing the gospel</span>
        <span class="desc">Walk someone down the Romans Road, right now.</span>
      </button>
      <button class="path" onclick={() => (stage = "tiers")}>
        <span class="name">Established believer</span>
        <span class="desc">
          Set up your Bible for study and memorization, and prepare to share the good news with
          others.
        </span>
      </button>
      {/if}
    {:else if stage === "welcome"}
      <h2>I'm so glad you've put your faith in Jesus!</h2>
      {@render sharedBy()}
      <div class="welcome">
        <p>There are some next steps you can take to grow in faith:</p>
        <p>
          <b>Start reading your Bible.</b> The next page will open in the book of John, which is a
          great place to start reading the inspired, inerrant word of God. You've been linked the
          King James Version, which is the closest to the original texts and has been used for
          hundreds of years by millions of believers. If you have trouble with the older English,
          I recommend you read a newer translation like the ESV alongside (not instead of) the
          King James to better understand.
        </p>
        {@render vquote([REF.pure])}
        <p>
          <b>Find a church.</b> Being part of a local church is a great way to grow in your faith
          and connect with believers.
          {#if hasChurch(fromChurch)}
            This Bible was shared with you by <b>{fromChurch.name}</b>{fromChurch.info
              ? ` — ${fromChurch.info}`
              : ""}. Start there: they would be glad to see you, and whoever gave you this can
            introduce you.
            {#if safeChurchUrl(fromChurch.url)}
              <a class="ref-link" href={safeChurchUrl(fromChurch.url)} target="_blank" rel="noopener noreferrer">
                Visit {fromChurch.name}
              </a>
            {/if}
          {:else}
            If someone shared this app with you, consider reaching out to them or attending a
            Sunday morning service at their church.
          {/if}
        </p>
        {@render vquote([REF.church])}
        <p>
          <b>Memorize.</b> This app can also help you memorize scripture — hiding the word in your
          heart is a wise and helpful thing to do.
        </p>
        {@render vquote([REF.heart])}
        <p>
          Know that Jesus loves you, and if you trust in him for your salvation, then you have
          eternal life:
        </p>
        {@render vquote([REF.love, REF.loved])}
        <p>No one can take it away from you, and you can know that for certain:</p>
        {@render vquote([REF.kept, REF.know])}
        <p>
          One day you will be perfected, but not yet — and so while you are here, you are imperfect
          but you are forgiven:
        </p>
        {@render vquote([REF.perfected, REF.forgiven])}
        <p>
          I highly recommend you read your Bible as it is rich with wisdom on how to navigate this
          world and how to serve our Lord and Saviour Jesus Christ:
        </p>
        {@render vquote([REF.wisdom])}
        <p>
          If you are in a difficult place in your life, ask God to help you with your struggles:
        </p>
        {@render vquote([REF.struggle])}
        <p>
          May the peace and joy of Christ be with you, and may you share that peace and joy with
          others. God bless you!
        </p>
        <p class="hint">Tap any verse reference to open it beside the book of John.</p>
      </div>
      <button class="start" onclick={() => (rereading ? (s.reopenIntro = null) : startInJohn())}>
        {rereading ? "Close" : "Open the book of John"}
      </button>
    {:else if stage === "curious"}
      <h2>I'm glad you're curious about the Bible.</h2>
      {@render sharedBy()}
      <div class="welcome">
        <p>
          For thousands of years this text has been the foundation of civilizations and of the
          lives of individuals. People have been killed for reading it and for sharing it.
        </p>
        <p>
          It contains the history of our world from its creation to the incarnation of its Creator
          here on earth with us. He came to save us because he loves us:
        </p>
        {@render vquote([REF.loved])}
        <p>
          Whether you are just curious or returning to faith after a long time, there is treasure
          here for you:
        </p>
        {@render vquote([REF.treasure])}
        <p>
          If you are having trouble believing, you're not alone — someone said exactly that to
          Jesus himself:
        </p>
        {@render vquote([REF.unbelief])}
        <p>
          I encourage you to read this book starting with the book of John, and to pray that if God
          is real, he would reveal himself to you. I've known many people for whom that prayer has
          been answered:
        </p>
        {@render vquote([REF.ask, REF.seek])}
        <p>If you are in a difficult place in your life, ask God to help you with your struggles:</p>
        {@render vquote([REF.struggle])}
        <p class="hint">Tap any verse reference to open it beside the book of John.</p>
      </div>
      <button class="start" onclick={() => (rereading ? (s.reopenIntro = null) : startInJohn())}>
        {rereading ? "Close" : "Open the book of John"}
      </button>
    {:else if stage === "church"}
      <h2>Before you share it</h2>
      <p class="sub">
        You're about to walk someone down the Romans Road. If they keep the app afterwards, this
        is how they find their way back to you.
      </p>
      {@render churchFields()}
      <button class="start" onclick={toRomansRoad}>Open the Romans Road</button>
      <button class="ch-skip" onclick={toRomansRoad}>Skip for now</button>
    {:else}
      <h2>Welcome to Plumbline</h2>
      <p class="ch-title">Your church</p>
      {@render churchFields()}
      <hr class="ch-rule" />
      <p class="sub">
        The Holy Bible is always on — reading, search, and your own tags, notes, and
        threads. Choose which layers of analysis sit alongside it:
      </p>
      <label class="card">
        <input type="checkbox" bind:checked={human} />
        <span class="body">
          <span class="name">Scholars' analysis <span class="mark human">†</span></span>
          <span class="desc">
            Curated scholarship: how the text renders each original word (<i>agapaō</i> → “love”
            ×27 · “beloved” ×13…), word grammar, the same root traced across the testaments, and
            the Treasury's cross-references.
          </span>
        </span>
      </label>
      <label class="card">
        <input type="checkbox" bind:checked={machine} />
        <span class="body">
          <span class="name">Machine analysis <span class="mark machine">≈</span></span>
          <span class="desc">
            Statistical patterns to weigh for yourself: similar concepts, words that appear
            alongside, verses like this one, and the concept maps.
          </span>
        </span>
      </label>
      <p class="note">Every piece of evidence is marked with where it comes from — ✝ the text · † scholarship · ≈ machine.</p>
      <button
        class="start"
        onclick={() => {
          saveChurchIfGiven();
          finish(human, machine);
        }}
      >Start reading</button>
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
    font-size: 20px;
    font-weight: 500;
    text-align: center;
  }
  .sub {
    font-size: 14px;
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
    font-size: 15px;
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
    font-size: 15.5px;
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
    font-size: 13.5px;
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
    font-size: 12.5px;
    color: var(--faded, #8a8276);
    font-style: italic;
  }
  .ch-title {
    font-weight: 600;
    text-align: center;
  }
  .ch-why {
    font-size: 12.5px;
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
    font-size: 15px;
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
    font-size: 13.5px;
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
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faded, #8a8276);
  }
  .fc-name {
    font-size: 17px;
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .fc-info {
    font-size: 13.5px;
  }
  .fc-url {
    font-size: 13px;
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
    font-size: 13.5px;
    color: var(--faded, #8a8276);
    line-height: 1.4;
  }
  .note {
    font-size: 12px;
    color: var(--faded, #8a8276);
    text-align: center;
  }
  .start {
    align-self: center;
    padding: 8px 26px;
    background: var(--gold, #9e7d38);
    color: #fff;
    border-radius: 8px;
    font-size: 16px;
  }
</style>
