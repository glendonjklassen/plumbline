<script lang="ts">
  // Explore, as its own SCREEN — the Android twin is ui/ExploreScreen.kt.
  //
  // It used to be a `kind: "explore"` inside the study panel, which on a phone is
  // a bottom sheet: you asked for the app's whole toolbox and got a card deck
  // sliding up over the verse you were reading, at 62% of the height, with the
  // reader still scrolling behind it. Feedback 2026-07-29, and fair — "its weird
  // as a swipe up. See Android." A destination should replace the reader, not
  // hover over it.
  //
  // The cards themselves are unchanged: each study tool with a sentence saying
  // what it is, because "Suggested" and "Constellation" mean nothing cold.
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  const cards = [
    { label: "Notes", desc: "Everything you've written about a verse.", go: () => (s.panel = { kind: "notesBrowser" }) },
    { label: "Threads", desc: "Passages you have linked together for sermons or study themes.", go: () => (s.panel = { kind: "threads" }) },
    { label: "Tags", desc: "Labelled verses by topic.", go: () => (s.panel = { kind: "tags" }) },
    { label: "Weaves", desc: "Parallel passages tied together.", go: () => (s.panel = { kind: "weaves" }) },
    { label: "Suggested", desc: "Proposed weaves awaiting your review.", go: () => (s.panel = { kind: "suggested" }) },
    { label: "Constellation", desc: "Every weave drawn as a row of dots across the Bible. Tap a dot to open that verse.", go: () => (s.mapPopup = { kind: "constellation" }) },
    { label: "Weave map", desc: "A visualization of weaves across the Bible.", go: () => (s.mapPopup = { kind: "chord" }) },
  ];
</script>

<section class="screen" aria-label="Explore">
  <div class="bar">
    <button class="back" onclick={() => s.goRead()} aria-label="Back to reading">‹</button>
    <h2>Explore</h2>
  </div>
  <div class="content">
    {#each cards as c (c.label)}
      <button class="ex-card" onclick={c.go}>
        <span class="ex-name">{c.label}</span>
        <span class="ex-desc">{c.desc}</span>
      </button>
    {/each}
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
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 10px;
    background: var(--paneNavBg, #efeae1);
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .back {
    font-size: calc(22px * var(--uiScale, 1));
    line-height: 1;
    padding: 8px 14px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .back:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  h2 {
    margin: 0;
    font-size: calc(18px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    align-content: start;
  }
  .ex-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
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
  .ex-desc {
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.4;
    color: var(--faded, #8a8276);
  }
</style>
