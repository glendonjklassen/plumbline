<script lang="ts">
  // The PREACH hub, as its own SCREEN — the preacher's prep room. The bar's
  // Preach tab used to raise Present directly; the maintainer's direction
  // (2026-08-11) is that the role holds more than the presentation moment:
  // the weaves being presented, and the tags and notes the sermon was built
  // from. Present stays the headline card; the materials sit under it.
  //
  // Same structure as ExploreScreen (a destination replaces the reader), and
  // the same tools appear inside Study too — the bar carries ROLES, and one
  // tool can serve two hats.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  // Catalogue KEYS, not copy (check-i18n reads `desc:`-shaped properties as
  // literals, so these are named as the ids they are).
  const cards = [
    { nameKey: "nav.present", descKey: "preach.present.desc", go: () => (s.showPresent = true) },
    { nameKey: "explore.weaves", descKey: "explore.weaves.desc", go: () => (s.panel = { kind: "weaves" }) },
    { nameKey: "explore.tags", descKey: "explore.tags.desc", go: () => (s.panel = { kind: "tags" }) },
    { nameKey: "explore.notes", descKey: "explore.notes.desc", go: () => (s.panel = { kind: "notesBrowser" }) },
  ];
</script>

<section class="screen" aria-label={t("nav.preach")}>
  <ScreenBar title={t("nav.preach")} onBack={() => s.goRead()} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    {#each cards as c (c.nameKey)}
      <button class="ex-card" onclick={c.go}>
        <span class="ex-name">{t(c.nameKey)}</span>
        <span class="ex-desc">{t(c.descKey)}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  /* ExploreScreen's card grid verbatim — the two hubs must read as siblings. */
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
    /* See ExploreScreen: `auto` restores the content-driven minimum the global
       44px tap floor replaces; the floor is still met by the padding. */
    min-height: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: start;
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
