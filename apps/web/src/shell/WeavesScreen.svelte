<script lang="ts">
  // WEAVES — a page of its own under Study, the way Tags became one: the hub
  // used to spend two sibling cards (Weaves, Suggested) on two views of the
  // same library (maintainer, 2026-08-19). One door now; the Android twin is
  // WeavesScreen in ui/StudyScreen.kt, whose All/Suggested filter holds the
  // same two views.
  //
  // Browse is the library panel the Weaves card always raised; Review is the
  // suggested queue the Suggested card raised. Both stay PANELS — a weave is
  // about the passages it ties, and the panel keeps the text alongside.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  // qStale, not q: these counts only gate cards — nothing here aims a tap by
  // ordinal — and a held count keeps Review from flashing disabled while a
  // fresh answer is in flight (the hub's cards read the same way).
  const weaveCount = $derived(((s.qStale("weaves")?.weaves ?? []) as any[]).length);
  const suggestedCount = $derived(((s.qStale("suggestedWeaves")?.suggested ?? []) as any[]).length);

  const close = (): void => {
    s.screen = "explore";
  };

  const actions = $derived([
    { id: "browse", go: () => (s.panel = { kind: "weaves" }), have: weaveCount, need: 0 },
    { id: "review", go: () => (s.panel = { kind: "suggested" }), have: suggestedCount, need: 1 },
  ]);
</script>

<section class="screen" aria-label={t("explore.weaves")}>
  <ScreenBar title={t("explore.weaves")} onBack={close} backLabel={t("nav.study")} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <p class="lede">{t("explore.weaves.desc")}</p>
    <div class="grid">
      {#each actions as a (a.id)}
        <!-- An action with nothing to act on is DISABLED rather than hidden,
             same as the Tags page: a menu whose items appear as you acquire
             data is a menu you cannot learn. The reason it is off is in the
             description. -->
        <button class="ex-card" disabled={a.have < a.need} onclick={a.go}>
          <span class="ex-name">{t(`weaves.${a.id}`)}</span>
          <span class="ex-desc">
            {a.have < a.need ? t(`weaves.${a.id}.needs`) : t(`weaves.${a.id}.desc`)}
          </span>
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
  .ex-card:hover:not(:disabled) {
    border-color: var(--gold, #9e7d38);
  }
  .ex-card:disabled {
    opacity: 0.55;
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
