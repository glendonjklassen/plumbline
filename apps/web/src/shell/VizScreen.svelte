<script lang="ts">
  // VISUALIZATIONS — a page of its own under Study, the Android twin being the
  // second MapOverlay in ui/StudyScreen.kt's ExploreScreen.
  //
  // This was an expanding card on the hub, with the two maps as indented
  // sub-cards beneath it (UAT 2026-08-12 asked for the maps to be "one of N
  // subitems of a visualization menu item", and in-place expansion was the
  // reading of that). It is a page now, because the tree was the odd one out
  // in a shell where a destination REPLACES what came before rather than
  // unfolding inside it: Plans and Memorize are both pages under Study, and
  // one card that grows a branch when tapped looked like a mechanism the rest
  // of the app does not have.
  //
  // Being a page also buys the thing an expanding card could not: room. Each
  // map gets its full sentence at the same size as every other tool, instead
  // of an indented line competing with the card that spawned it.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const maps = [
    { id: "constellation", go: () => (s.mapPopup = { kind: "constellation" }) },
    { id: "weaveMap", go: () => (s.mapPopup = { kind: "chord" }) },
  ];

  // ‹ returns to the STUDY hub, not to the reader: this is one layer down from
  // it, and the Plans/Memorize screens make the same call.
  const close = (): void => {
    s.screen = "explore";
  };
</script>

<section class="screen" aria-label={t("explore.viz")}>
  <ScreenBar title={t("explore.viz")} onBack={close} backLabel={t("nav.study")} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <p class="lede">{t("explore.viz.desc")}</p>
    <div class="grid">
      {#each maps as m (m.id)}
        <button class="ex-card" onclick={m.go}>
          <span class="ex-name">{t(`explore.${m.id}`)}</span>
          <span class="ex-desc">{t(`explore.${m.id}.desc`)}</span>
        </button>
      {/each}
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
    gap: 12px;
  }
  .lede {
    margin: 0;
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.45;
    color: var(--faded, #8a8276);
  }
  .grid {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    align-content: start;
  }
  /* The hub's card, to the pixel — this is the same kind of thing one layer
     down, and a different card here would read as a different kind of choice. */
  .ex-card {
    min-height: auto;
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
