<script lang="ts">
  // Explore, as its own SCREEN — the Android twin is ui/ExploreScreen.kt.
  //
  // A destination should replace the reader, not hover over it.
  //
  // The cards themselves are unchanged: each study tool with a sentence saying
  // what it is, because "Suggested" and "Constellation" mean nothing cold.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const cards = [
    { id: "plans", go: () => (s.panel = { kind: "plans" }) },
    { id: "notes", go: () => (s.panel = { kind: "notesBrowser" }) },
    { id: "threads", go: () => (s.panel = { kind: "threads" }) },
    { id: "tags", go: () => (s.panel = { kind: "tags" }) },
    { id: "weaves", go: () => (s.panel = { kind: "weaves" }) },
    { id: "suggested", go: () => (s.panel = { kind: "suggested" }) },
    { id: "constellation", go: () => (s.mapPopup = { kind: "constellation" }) },
    { id: "weaveMap", go: () => (s.mapPopup = { kind: "chord" }) },
  ];
</script>

<section class="screen" aria-label={t("nav.explore")}>
  <ScreenBar title={t("nav.explore")} onBack={() => s.goRead()} />
  <div class="content">
    {#each cards as c (c.id)}
      <button class="ex-card" onclick={c.go}>
        <span class="ex-name">{t(`explore.${c.id}`)}</span>
        <span class="ex-desc">{t(`explore.${c.id}.desc`)}</span>
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
