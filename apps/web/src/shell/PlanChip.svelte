<script lang="ts">
  // The nav-strip plan chip (docs/READING-PLANS.md decision #5): "Day 12 ·
  // Gen 30–31", riding the reader whenever a schedule plan is running, so the
  // plan is present where the reading happens instead of only behind
  // Explore ▸ Plans. Tap goes to today's first unread chapter; when several
  // plans run, the first one's day rides the chip and "+1 more" opens the
  // Plans panel for the rest.
  //
  // NOT shown in concept-study mode: the tracker is suspended there, so time in
  // the mode cannot advance a schedule — a chip inviting schedule reading would
  // promise credit the mode deliberately withholds.
  import { getSession } from "../state/session.svelte";
  import { chapterSpan, firstUnread, remaining, todayPlans } from "./planToday";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const plans = $derived.by(() => {
    void s.studyEpoch;
    return todayPlans(s.q("plans", ""));
  });
  const lead = $derived(plans[0] ?? null);

  function go(): void {
    const target = lead && firstUnread(lead);
    if (target) s.navigate(s.activePane, target.book, target.chapter);
  }
</script>

{#if lead && !s.inConceptStudy}
  <div class="plan-chip-row">
    <button class="plan-chip" onclick={go} title={t("plans.chipGo")}>
      {t("plans.chip", { day: lead.day, chapters: chapterSpan(remaining(lead)) })}
    </button>
    {#if plans.length > 1}
      <button class="plan-chip more" onclick={() => (s.panel = { kind: "plans" })}>
        {t("plans.chipMore", { n: plans.length - 1 })}
      </button>
    {/if}
  </div>
{/if}

<style>
  /* A quiet pill just above the canon strip: present, not campaigning — the
     reading-map philosophy (an invitation, not a debt). */
  .plan-chip-row {
    display: flex;
    justify-content: center;
    gap: 6px;
    padding: 4px 8px;
    background: var(--paneNavBg, #efeae1);
    border-top: 1px solid var(--rule, #d8cba8);
  }
  .plan-chip {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 4px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 999px;
    background: var(--paper, #fcf9f4);
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
  }
  .plan-chip:hover {
    border-color: var(--gold, #9e7d38);
  }
  .plan-chip.more {
    color: var(--faded, #8a8276);
    font-weight: 400;
    flex: 0 0 auto;
  }
</style>
