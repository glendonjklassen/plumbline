<script lang="ts">
  // Reading plans as a DESTINATION — a full screen off the Study hub, like
  // Memorize. Plans began as a study-panel kind, which crammed three different
  // jobs (running schedules, concept studies, the catalogue) into a 380px
  // sidebar column (maintainer UAT call, 2026-08-11: "crammed, not luxurious").
  // A destination replaces the reader; each job gets its own section and room.
  //
  // Bespoke, not blocks — it is interactive (start/stop/enter), which a block
  // list is not.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { dispatchLink } from "../study/links";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const plans = $derived.by(() => {
    void s.studyEpoch; // any authoring write (start/stop/sweep) re-fetches
    return s.q("plans", "");
  });

  // The two kinds of running plan are DIFFERENT THINGS and get their own
  // sections: a schedule is a calendar you keep, a concept study is a sweep
  // you are in the middle of. Interleaving them in one list is what made the
  // panel unreadable.
  const schedules = $derived(((plans?.running ?? []) as any[]).filter((p) => p.kind !== "conceptStudy"));
  const conceptStudies = $derived(((plans?.running ?? []) as any[]).filter((p) => p.kind === "conceptStudy"));

  let conceptStudyTag = $state("");
  async function launchConceptStudy(): Promise<void> {
    const tag = conceptStudyTag.trim();
    if (!tag) return;
    conceptStudyTag = "";
    await s.startConceptStudy(tag);
  }

  /** The plans still worth offering: a builtin whose CLASS is already occupied
   *  is not one of them. Running "the whole Bible in a year" and being shown
   *  the 180- and 90-day plans beside it invites a tap that can only mean
   *  "throw away the plan I am on" — the reader stops the running plan first
   *  (the maintainer's UAT call, 2026-08-11). The engine already enforces one
   *  plan per class with a replace-confirm; this stops the picker from asking. */
  function offerable(p: any): any[] {
    const runningClasses = new Set(((p?.running ?? []) as any[]).map((r) => r.class).filter(Boolean));
    return ((p?.builtins ?? []) as any[]).filter((b) => !runningClasses.has(b.class));
  }

  /** A schedule plan's display name, from the builtin catalogue (its `nameKey`
   *  is a catalogue id); falls back to the raw id for an unknown plan. */
  function planName(id: string): string {
    const b = ((plans?.builtins ?? []) as any[]).find((x) => x.id === id);
    return b ? t(b.nameKey) : id;
  }

  /** refKey → the core's `go:` verb, split on the LAST space, as core `go_uri`
   *  does. Also in App.svelte and MemorizeHost — see App.svelte for why. */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;
  function onLink(uri: string, ev: MouseEvent): void {
    void dispatchLink(s, uri, ev);
  }

  function close(): void {
    // Plans is a card inside the STUDY hub, so its ‹ returns there — up one
    // layer, not two (the Memorize pattern).
    s.screen = "explore";
  }
</script>

<section class="screen" aria-label={t("plans.title")}>
  <ScreenBar title={t("plans.title")} onBack={close} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <div class="inner">
      <h3 class="sub">{t("plans.running")}</h3>
      {#if schedules.length === 0}
        <p class="hint">{t("plans.empty")}</p>
      {/if}
      {#each schedules as p (p.id)}
        <div class="plan-card">
          <div class="plan-head">
            <span class="plan-name">{planName(p.id)}</span>
            <span class="plan-prog">{t("plans.dayProgress", { done: p.scheduleProgress[0], total: p.scheduleProgress[1] })}</span>
          </div>
          {#if p.today}
            <button class="plan-today" onclick={(e) => p.today.chapters[0] && onLink(goUri(`${p.today.chapters[0].book} ${p.today.chapters[0].chapter}:1`), e)}>
              {t("plans.today", { chapters: p.today.chapters.map((c: any) => c.display).join(", ") })}
            </button>
          {:else}
            <p class="plan-done">{t("plans.finished")}</p>
          {/if}
          <div class="plan-actions">
            <button class="danger" onclick={() => s.stopPlan(p.id, planName(p.id))}>{t("plans.stop")}</button>
          </div>
        </div>
      {/each}

      <h3 class="sub">{t("plans.conceptStudyHeading")}</h3>
      <p class="hint">{t("plans.conceptStudyHint")}</p>
      {#each conceptStudies as p (p.id)}
        <div class="plan-card concept-study">
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
      <div class="concept-study-launch">
        <input
          type="text"
          bind:value={conceptStudyTag}
          placeholder={t("plans.conceptStudyPlaceholder")}
          onkeydown={(e) => e.key === "Enter" && launchConceptStudy()}
        />
        <button disabled={!conceptStudyTag.trim()} onclick={launchConceptStudy}>{t("plans.conceptStudyStart")}</button>
      </div>

      <h3 class="sub">{t("plans.available")}</h3>
      {#each offerable(plans) as b (b.id)}
        <button class="plan-builtin" onclick={() => s.startPlan({ id: b.id, class: b.class, name: t(b.nameKey) })}>
          <span class="plan-name">{t(b.nameKey)}</span>
          <span class="plan-add">{t("plans.start")}</span>
        </button>
      {/each}
      {#if offerable(plans).length === 0}
        <p class="hint">{t("plans.classFull")}</p>
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
    font-size: calc(16px * var(--uiScale, 1));
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 8px 20px 48px;
  }
  /* One comfortable column. The panel squeezed this into 380px beside the
     text; as a destination it gets a reading-width measure, centered. */
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
  .sub:first-child {
    margin-top: 10px;
  }
  .hint {
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
    line-height: 1.5;
    margin: 0 0 12px;
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
  .plan-card.concept-study {
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
  .plan-today {
    text-align: left;
    color: var(--gold, #9e7d38);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-size: calc(14px * var(--uiScale, 1));
  }
  .plan-done {
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
    margin: 0;
  }
  .plan-actions {
    display: flex;
    gap: 10px;
  }
  .plan-actions button,
  .concept-study-launch button,
  .plan-builtin {
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
  .concept-study-launch {
    display: flex;
    gap: 10px;
    margin-top: 4px;
  }
  .concept-study-launch input {
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
  .plan-builtin {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    gap: 12px;
    margin-bottom: 8px;
    padding: 12px 16px;
    border-radius: 10px;
    text-align: left;
  }
  .plan-builtin:hover {
    border-color: var(--gold, #9e7d38);
  }
  .plan-builtin .plan-add {
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
    white-space: nowrap;
  }
</style>
