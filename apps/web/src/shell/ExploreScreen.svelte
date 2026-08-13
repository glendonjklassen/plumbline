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
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const cards = [
    { id: "plans", go: () => (s.screen = "plans") },
    {
      id: "memorize",
      go: () => {
        s.screen = "memorize";
        s.memorize = { view: "hub" };
      },
    },
    { id: "notes", go: () => (s.panel = { kind: "notesBrowser" }) },
    { id: "threads", go: () => (s.panel = { kind: "threads" }) },
    { id: "tags", go: () => (s.panel = { kind: "tags" }) },
    { id: "weaves", go: () => (s.panel = { kind: "weaves" }) },
    { id: "suggested", go: () => (s.panel = { kind: "suggested" }) },
  ];

  // The maps live under ONE card (maintainer UAT, 2026-08-12: the weave map
  // "should be one of N subitems of a visualization menu item") — two sibling
  // cards read as two more tools, when they are two views of the same thing.
  // The card expands in place: a whole destination for a two-item choice would
  // be a hallway with two doors.
  const VIZ = [
    { id: "constellation", go: () => (s.mapPopup = { kind: "constellation" }) },
    { id: "weaveMap", go: () => (s.mapPopup = { kind: "chord" }) },
  ];
  let vizOpen = $state(false);
</script>

<section class="screen" aria-label={t("nav.study")}>
  <ScreenBar title={t("nav.study")} onBack={() => s.goRead()} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    {#each cards as c (c.id)}
      <button class="ex-card" onclick={c.go}>
        <span class="ex-name">{t(`explore.${c.id}`)}</span>
        <span class="ex-desc">{t(`explore.${c.id}.desc`)}</span>
      </button>
    {/each}
    <div class="ex-group" class:open={vizOpen}>
      <button class="ex-card ex-toggle" aria-expanded={vizOpen} onclick={() => (vizOpen = !vizOpen)}>
        <span class="ex-name">{t("explore.viz")} <span class="ex-chevron">{vizOpen ? "▾" : "▸"}</span></span>
        <span class="ex-desc">{t("explore.viz.desc")}</span>
      </button>
      {#if vizOpen}
        {#each VIZ as v (v.id)}
          <button class="ex-card ex-sub" onclick={v.go}>
            <span class="ex-name">{t(`explore.${v.id}`)}</span>
            <span class="ex-desc">{t(`explore.${v.id}.desc`)}</span>
          </button>
        {/each}
      {/if}
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
  .ex-desc {
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.4;
    color: var(--faded, #8a8276);
  }
  /* The Visualizations group: one grid cell, the toggle card on top and the
     sub-cards stacked under it when open — the choice unfolds where the
     reader's finger already is instead of navigating anywhere. */
  .ex-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ex-sub {
    margin-left: 18px;
  }
  .ex-chevron {
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
  }
</style>
