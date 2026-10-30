<script lang="ts">
  // The Concept Study as a DESTINATION of its own off the Study hub, like Plans
  // and Memorize. It lived at the bottom of the Plans screen because the engine
  // files it as a plan kind, and a reader had to know that to find it
  // (maintainer, 2026-10-29: "needs to be a first class citizen on the study
  // page"). A sweep for a theme is not a reading schedule: it has no days, it
  // suspends the reading tracker, and it is the one plan kind with its own
  // reader mode — so it gets its own door, its own screen and its own band row.
  //
  // Bespoke, not blocks — start/resume/exit/stop are interactive, which a block
  // list is not.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const plans = $derived.by(() => {
    void s.studyEpoch; // any authoring write (start/stop/sweep) re-fetches
    return s.q("plans", "");
  });
  const studies = $derived(((plans?.running ?? []) as any[]).filter((p) => p.kind === "conceptStudy"));

  let tag = $state("");
  async function launch(): Promise<void> {
    const name = tag.trim();
    if (!name) return;
    tag = "";
    await s.startConceptStudy(name);
  }

  function close(): void {
    // A card inside the STUDY hub, so ‹ returns there — up one layer, not two
    // (the Plans and Memorize pattern).
    s.screen = "explore";
  }
</script>

<section class="screen" aria-label={t("conceptStudy.title")}>
  <ScreenBar title={t("conceptStudy.title")} onBack={close} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <div class="inner">
      <p class="hint">{t("plans.conceptStudyHint")}</p>

      {#if studies.length > 0}
        <h3 class="sub">{t("plans.running")}</h3>
      {/if}
      {#each studies as p (p.id)}
        <div class="plan-card concept-study" class:active={s.conceptStudyId === p.id}>
          <div class="plan-head">
            <span class="plan-name">{t("plans.conceptStudyTag", { tag: p.tag })}</span>
            <span class="plan-prog">{t("plans.sweepProgress", { done: p.sweepProgress[0], total: p.sweepProgress[1] })}</span>
          </div>
          <div class="plan-actions">
            {#if s.conceptStudyId !== p.id}
              <button onclick={() => s.enterConceptStudy(p.id)}>{t("plans.enter")}</button>
            {:else}
              <button onclick={() => s.exitConceptStudy()}>{t("conceptStudy.exit")}</button>
            {/if}
            <button class="danger" onclick={() => s.stopPlan(p.id, p.tag)}>{t("plans.stop")}</button>
          </div>
        </div>
      {/each}

      <h3 class="sub">{t("plans.conceptStudyHeading")}</h3>
      <div class="launch">
        <input
          type="text"
          bind:value={tag}
          placeholder={t("plans.conceptStudyPlaceholder")}
          onkeydown={(e) => e.key === "Enter" && launch()}
        />
        <button disabled={!tag.trim()} onclick={launch}>{t("plans.conceptStudyStart")}</button>
      </div>
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
    font-size: calc(16px * var(--uiScale, 1));
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 8px 20px 48px;
  }
  /* One comfortable column, the Plans screen's measure. */
  .inner {
    max-width: 560px;
    margin: 0 auto;
  }
  .sub {
    margin: 30px 0 10px;
    font-size: calc(13.5px * var(--uiScale, 1));
    color: var(--section-header, #a0894a);
    font-variant: small-caps;
    letter-spacing: 0.04em;
  }
  .hint {
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
    line-height: 1.5;
    margin: 10px 0 12px;
  }
  .plan-card {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    padding: 16px 18px;
    margin-bottom: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--popupPaper, #f2eee6);
  }
  /* The one the reader is IN is the tinted card; a run at rest is plain. */
  .plan-card.active {
    border-color: var(--tier-research, #b04a3a);
    background: color-mix(in srgb, var(--tier-research, #b04a3a) 8%, transparent);
  }
  .plan-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
  }
  .plan-name {
    font-weight: 600;
    font-size: calc(15.5px * var(--uiScale, 1));
  }
  .plan-prog {
    color: var(--faded, #8a8276);
    font-size: calc(13px * var(--uiScale, 1));
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .plan-actions {
    display: flex;
    gap: 10px;
  }
  .plan-actions button,
  .launch button {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 7px 14px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    cursor: pointer;
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .plan-actions button.danger {
    color: var(--tier-research, #b04a3a);
    border-color: color-mix(in srgb, var(--tier-research, #b04a3a) 60%, var(--rule, #d8cba8));
  }
  .launch button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .launch {
    display: flex;
    gap: 10px;
    margin-top: 4px;
  }
  .launch input {
    flex: 1;
    min-width: 0;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 8px 12px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font: inherit;
    font-size: calc(13.5px * var(--uiScale, 1));
  }
</style>
