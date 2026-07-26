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

  const s = getSession();

  type Stage = "choose" | "welcome" | "tiers";
  let stage = $state<Stage>("choose");
  let human = $state(true);
  let machine = $state(true);

  // The welcome's verse references (refKeys use OSIS book ids — canon.rs).
  // `keys` are the verses QUOTED inline — the new believer reads scripture
  // itself, not a row of links (product 2026-07-26).
  interface Ref {
    label: string;
    book: string;
    chapter: number;
    verse: number;
    keys: string[];
  }
  const REF: Record<string, Ref> = {
    love: { label: "Romans 5:8", book: "Rom", chapter: 5, verse: 8, keys: ["Rom 5:8"] },
    pure: { label: "Psalm 12:6–7", book: "Ps", chapter: 12, verse: 6, keys: ["Ps 12:6", "Ps 12:7"] },
    church: { label: "Hebrews 10:24–25", book: "Heb", chapter: 10, verse: 24, keys: ["Heb 10:24", "Heb 10:25"] },
    heart: { label: "Psalm 119:11", book: "Ps", chapter: 119, verse: 11, keys: ["Ps 119:11"] },
    loved: { label: "John 3:16", book: "John", chapter: 3, verse: 16, keys: ["John 3:16"] },
    know: { label: "1 John 5:13", book: "1John", chapter: 5, verse: 13, keys: ["1John 5:13"] },
    kept: { label: "John 10:28–29", book: "John", chapter: 10, verse: 28, keys: ["John 10:28"] },
    perfected: { label: "Philippians 1:6", book: "Phil", chapter: 1, verse: 6, keys: ["Phil 1:6"] },
    forgiven: { label: "1 John 1:9", book: "1John", chapter: 1, verse: 9, keys: ["1John 1:9"] },
    wisdom: { label: "2 Timothy 3:16–17", book: "2Tim", chapter: 3, verse: 16, keys: ["2Tim 3:16", "2Tim 3:17"] },
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
  function startInJohn(ref?: Ref): void {
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

  /** Witnessing: straight to Present with the Romans Road, ready to show. */
  function sharing(): void {
    finish(true, true);
    s.presentThreadName = "Romans Road";
    s.showPresent = true;
  }

  function dismiss(): void {
    // Clicking away on the chooser/tiers keeps the old behaviour (defaults);
    // the welcome page asks for an explicit choice.
    if (stage !== "welcome") finish(human, machine);
  }
</script>

{#snippet refchip(r: Ref)}
  <button class="ref" onclick={() => startInJohn(r)} title="Open {r.label} beside John">{r.label}</button>
{/snippet}

{#snippet vquote(refs: Ref[])}
  <blockquote class="vq">
    <span class="vq-text">“{refs
      .flatMap((r) => r.keys)
      .map((k) => s.q("verse", k)?.body ?? "")
      .join(" ")
      .trim()}”</span>
    <span class="vq-refs">{#each refs as r (r.label)}{@render refchip(r)}{/each}</span>
  </blockquote>
{/snippet}

{#if s.showFirstRun}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={dismiss}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    {#if stage === "choose"}
      <h2>Welcome to Plumbline</h2>
      <p class="sub">The 1769 King James text, free and offline. Where would you like to begin?</p>
      <button class="path" onclick={() => (stage = "welcome")}>
        <span class="name">New in the faith</span>
        <span class="desc">I've just put my faith in Jesus — where do I start?</span>
      </button>
      <button class="path" onclick={sharing}>
        <span class="name">Sharing the gospel</span>
        <span class="desc">Walk someone down the Romans Road, right now.</span>
      </button>
      <button class="path" onclick={() => (stage = "tiers")}>
        <span class="name">Established believer</span>
        <span class="desc">Set up which layers of analysis sit alongside the text.</span>
      </button>
    {:else if stage === "welcome"}
      <h2>We're so glad you've put your faith in Jesus</h2>
      <div class="welcome">
        <p>There are some next steps you can take to grow in faith:</p>
        <p>
          <b>Start reading your Bible.</b> The next page will open in the book of John, which is a
          great place to start reading the inspired, inerrant word of God. You've been linked the
          King James Version, which is the closest to the original texts and has been used for
          hundreds of years by millions of believers. If you have trouble with the older English,
          we recommend you read a newer translation like the ESV alongside (not instead of) the
          King James to better understand.
        </p>
        {@render vquote([REF.pure])}
        <p>
          <b>Find a church.</b> Being part of a local church is a great way to grow in your faith
          and connect with believers. If someone shared this app with you, consider reaching out to
          them or attending a Sunday morning service at their church.
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
          We highly recommend you read your Bible as it is rich with wisdom on how to navigate this
          world and how to serve our Lord and Saviour Jesus Christ:
        </p>
        {@render vquote([REF.wisdom])}
        <p>
          May the peace and joy of Christ be with you, and may you share that peace and joy with
          others. God bless you!
        </p>
        <p class="hint">Tap any verse reference to open it beside the book of John.</p>
      </div>
      <button class="start" onclick={() => startInJohn()}>Open the book of John</button>
    {:else}
      <h2>Welcome to Plumbline</h2>
      <p class="sub">
        The 1769 King James text is always on — reading, search, and your own tags, notes, and
        threads. Choose which layers of analysis sit alongside it:
      </p>
      <label class="card">
        <input type="checkbox" bind:checked={human} />
        <span class="body">
          <span class="name">Scholars' analysis <span class="mark human">†</span></span>
          <span class="desc">
            Curated scholarship: how the 1769 renders each original word (<i>agapaō</i> → “love”
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
      <button class="start" onclick={() => finish(human, machine)}>Start reading</button>
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
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    padding: 22px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 12px;
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
