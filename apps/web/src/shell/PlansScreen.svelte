<script lang="ts">
  // Reading plans as a DESTINATION — a full screen off the Study hub, like
  // Memorize. Plans began as a study-panel kind, which crammed three different
  // jobs (running schedules, concept studies, the catalogue) into a 380px
  // sidebar column (maintainer UAT call, 2026-08-11: "crammed, not luxurious").
  // A destination replaces the reader; each job gets its own section and room.
  // The concept study has since moved out altogether, to its own screen
  // (ConceptStudyScreen.svelte): it is filed as a plan kind in the engine, but a
  // sweep for a theme is not a schedule, and a reader should not have to know
  // the filing to find it.
  //
  // Bespoke, not blocks — it is interactive (start/stop/enter), which a block
  // list is not.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { dispatchLink } from "../study/links";
  import { dateLocale, lang, plural, t } from "../lib/i18n.svelte";
  import { localDay } from "../engine/StudyEngine";

  const s = getSession();

  const plans = $derived.by(() => {
    void s.studyEpoch; // any authoring write (start/stop/sweep) re-fetches
    return s.q("plans", "");
  });

  // Schedules only: the engine lists concept studies among the running plans,
  // but they are a different thing (a sweep, not a calendar) and have their own
  // screen. Interleaving them in one list is what made the old panel unreadable.
  const schedules = $derived(((plans?.running ?? []) as any[]).filter((p) => p.kind !== "conceptStudy"));

  /** Devotionals — running and offered. A THIRD kind of thing on this screen
   *  and so a third section, for the reason the two above are separate: a
   *  booklet is a month of authored readings, not a calendar of chapters and
   *  not a sweep. They start and stop exactly like a plan (maintainer,
   *  2026-08-26), which is why they are here rather than behind their own door.
   *
   *  There is no class exclusivity: two booklets at once is a reader's business,
   *  so every catalogue entry not already running is offered. */
  const devotionals = $derived.by(() => {
    void s.studyEpoch; // start/stop/done re-fetches, like the plans read above
    return s.q("devotionals", lang(), localDay());
  });
  const runningDevotionals = $derived(((devotionals?.running ?? []) as any[]) ?? []);
  const offerableDevotionals = $derived.by(() => {
    const running = new Set(runningDevotionals.map((r) => r.id));
    return (((devotionals?.catalogue ?? []) as any[]) ?? []).filter((b) => !running.has(b.id));
  });

  /** Today's memorization under the daily cap (maintainer, 2026-10-24: "a
   *  memory tile there for scheduled memorization … a maximum per day").
   *  HELD through a refetch for the reason PlanChip holds its plans: a grade
   *  or a cap change re-asks, and the card must not blink empty meanwhile. */
  let heldToday: any = null;
  const today = $derived.by(() => {
    const q = s.memoryToday();
    if (q == null) return heldToday;
    return (heldToday = q);
  });
  const todayRefs = $derived((today?.refs ?? []) as string[]);
  const reviewedToday = $derived(Number(today?.reviewedToday ?? 0));
  const dueTotal = $derived(Number(today?.dueTotal ?? 0));

  function reviewToday(): void {
    if (todayRefs.length === 0) return;
    s.memorize = { view: "review", queue: todayRefs };
    s.screen = "memorize";
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

  /** A schedule plan's display name: a custom plan's own (the engine spells it
   *  in the reader's language), else the builtin catalogue's `nameKey`; falls
   *  back to the raw id for an unknown plan. */
  function planName(p: any): string {
    if (typeof p.name === "string" && p.name) return p.name;
    const b = ((plans?.builtins ?? []) as any[]).find((x) => x.id === p.id);
    return b ? t(b.nameKey) : p.id;
  }

  // ── the custom plan builder ─────────────────────────────────────────────────
  //
  // Parts of the Bible (the reading map's sections, off the plans wire) and
  // chapter ranges, added up into spans, and a day count. The engine normalizes
  // the spans — canon order, overlaps joined, clamped to the corpus — so the
  // builder only has to collect what the reader points at.
  let customOpen = $state(false);
  let chosenParts = $state(new Set<string>());
  let ranges = $state<[string, number, number][]>([]);
  let fromBook = $state("Gen");
  let fromCh = $state(1);
  let toBook = $state("Gen");
  let toCh = $state(1);
  let days = $state(30);

  type Book = { id: string; name: string; chapters: number };
  const books = $derived(((s.q("toc")?.books ?? []) as Book[]) ?? []);
  const parts = $derived(((plans?.parts ?? []) as { id: string; name: string; books: string[] }[]) ?? []);
  const bookIndex = (id: string): number => books.findIndex((b) => b.id === id);
  const bookName = (id: string): string => books.find((b) => b.id === id)?.name ?? id;
  const chaptersOf = (id: string): number => books.find((b) => b.id === id)?.chapters ?? 0;

  function togglePart(id: string): void {
    const next = new Set(chosenParts);
    if (!next.delete(id)) next.add(id);
    chosenParts = next;
  }

  /** The range row as spans: one book, or the first book from its chapter,
   *  every book between whole, the last book to its chapter. Ends given
   *  backwards are swapped rather than refused. */
  function addRange(): void {
    let [a, ac, z, zc] = [fromBook, fromCh, toBook, toCh];
    if (bookIndex(a) > bookIndex(z) || (a === z && ac > zc)) [a, ac, z, zc] = [z, zc, a, ac];
    const clamp = (b: string, c: number) => Math.min(Math.max(1, Math.floor(c || 1)), chaptersOf(b) || 1);
    ac = clamp(a, ac);
    zc = clamp(z, zc);
    const out: [string, number, number][] = [];
    if (a === z) out.push([a, ac, zc]);
    else {
      out.push([a, ac, chaptersOf(a)]);
      for (let i = bookIndex(a) + 1; i < bookIndex(z); i++) out.push([books[i].id, 1, books[i].chapters]);
      out.push([z, 1, zc]);
    }
    ranges = [...ranges, ...out];
  }
  function removeRange(i: number): void {
    ranges = ranges.filter((_, k) => k !== i);
  }
  /** "Isaiah" for a whole book, "Psalms 1–75" for part of one, "Psalms 23" for one chapter. */
  function rangeLabel([b, f, l]: [string, number, number]): string {
    if (f === 1 && l === chaptersOf(b)) return bookName(b);
    if (f === l) return t("ref.chapter", { book: bookName(b), chapter: f });
    return `${bookName(b)} ${f}–${l}`;
  }

  const spans = $derived.by((): [string, number, number][] => {
    const out: [string, number, number][] = [];
    for (const p of parts) {
      if (!chosenParts.has(p.id)) continue;
      for (const b of p.books) {
        const n = chaptersOf(b);
        if (n > 0) out.push([b, 1, n]);
      }
    }
    return [...out, ...ranges];
  });
  /** Chapters the plan would hold, each counted once — a part and a range may overlap. */
  const chapterTotal = $derived.by(() => {
    const seen = new Set<string>();
    for (const [b, f, l] of spans) for (let c = f; c <= l; c++) seen.add(`${b} ${c}`);
    return seen.size;
  });
  const perDay = $derived(days >= 1 && chapterTotal > 0 ? chapterTotal / days : 0);
  const nf = $derived(new Intl.NumberFormat(lang()));
  const nf1 = $derived(new Intl.NumberFormat(lang(), { maximumFractionDigits: 1 }));
  const canStart = $derived(chapterTotal > 0 && days >= 1);

  function resetBuilder(): void {
    customOpen = false;
    chosenParts = new Set();
    ranges = [];
    days = 30;
  }
  async function startCustom(): Promise<void> {
    if (!canStart) return;
    const label = [...parts.filter((p) => chosenParts.has(p.id)).map((p) => p.name), ...ranges.map(rangeLabel)].join(" · ");
    // Plain arrays, not the $state proxies `ranges` holds: the call crosses to
    // the engine worker by structured clone, which cannot copy a Proxy.
    const plain = spans.map(([b, f, l]): [string, number, number] => [b, Number(f), Number(l)]);
    const ok = await s.startCustomPlan(plain, Math.floor(Number(days)), label);
    if (ok) resetBuilder();
  }

  /** "3 Aug 2026" in the reader's language — how a run is told apart from any
   *  other run of the same plan: its name plus the day it was started. */
  function startedOn(stamp: string): string {
    const d = new Date(stamp);
    if (isNaN(d.getTime())) return stamp.slice(0, 10);
    return d.toLocaleDateString(dateLocale(), { day: "numeric", month: "short", year: "numeric" });
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
      <h3 class="sub">{t("devotional.heading")}</h3>
      <!-- "Start one below" is only true when there IS one below. An
           untranslated booklet is no longer offered at all (crates/ffi's
           devotionals.rs), so a language nobody has written one in reaches this
           section with an empty shelf, and the old line sent the reader looking
           for a button that was not there. -->
      {#if runningDevotionals.length === 0}
        <p class="hint">{t(offerableDevotionals.length === 0 ? "devotional.noneYet" : "devotional.empty")}</p>
      {/if}
      {#each runningDevotionals as d (d.id)}
        <div class="plan-card" class:paused={d.paused}>
          <div class="plan-head">
            <span class="plan-name">{d.name}</span>
            <span class="plan-prog">{t("devotional.progress", { done: d.daysDone, total: d.daysTotal })}</span>
          </div>
          <p class="plan-started">
            {t("plans.startedOn", { date: startedOn(d.started) })}{#if d.paused}
              · {t("plans.pausedBadge")}{/if}
          </p>
          {#if d.paused}
            <!-- No day line: a paused booklet asks nothing, the plan stance. -->
          {:else if d.today}
            <button class="plan-today" onclick={() => s.openDevotional(d.id, d.today.day)}>
              {t("devotional.dayOf", { day: d.today.day, total: d.daysTotal })} · {d.today.title}
            </button>
          {:else}
            <p class="plan-done">{t("devotional.finished")}</p>
          {/if}
          <div class="plan-actions">
            {#if d.today}
              {#if d.paused}
                <button onclick={() => s.setDevotionalPaused(d.id, false, d.name)}>{t("plans.resume")}</button>
              {:else}
                <button onclick={() => s.setDevotionalPaused(d.id, true, d.name)}>{t("plans.pause")}</button>
              {/if}
            {/if}
            <button class="danger" onclick={() => s.stopDevotional(d.id, d.name)}>{t("plans.stop")}</button>
          </div>
        </div>
      {/each}
      {#each offerableDevotionals as b (b.id)}
        <button class="plan-builtin" onclick={() => s.startDevotional({ id: b.id, name: b.name })}>
          <span class="plan-name">
            {b.name}
            {#if !b.translated}<span class="plan-note">{t("devotional.untranslated")}</span>{/if}
          </span>
          <span class="plan-add">{t("devotional.start")}</span>
        </button>
      {/each}

      <h3 class="sub">{t("plans.running")}</h3>
      {#if schedules.length === 0}
        <p class="hint">{t("plans.empty")}</p>
      {/if}
      {#each schedules as p (p.id)}
        <div class="plan-card" class:paused={p.paused}>
          <div class="plan-head">
            <span class="plan-name">{planName(p)}</span>
            <span class="plan-prog">{t("plans.dayProgress", { done: p.scheduleProgress[0], total: p.scheduleProgress[1] })}</span>
          </div>
          <!-- A run is its name plus the day it began — what tells "the NT in 90
               I paused in March" from "the one I started fresh". -->
          <p class="plan-started">
            {t("plans.startedOn", { date: startedOn(p.started) })}{#if p.paused}
              · {t("plans.pausedBadge")}{/if}
          </p>
          {#if p.paused}
            <!-- No today line: a paused plan asks nothing. Where it will pick
                 up is one Resume away, and promising chapters it is not asking
                 for reads as asking for them. -->
          {:else if p.today}
            <button class="plan-today" onclick={(e) => p.today.chapters[0] && onLink(goUri(`${p.today.chapters[0].book} ${p.today.chapters[0].chapter}:1`), e)}>
              {t("plans.today", { chapters: p.today.chapters.map((c: any) => c.display).join(", ") })}
            </button>
          {:else}
            <p class="plan-done">{t("plans.finished")}</p>
          {/if}
          <div class="plan-actions">
            {#if p.today}
              {#if p.paused}
                <button onclick={() => s.setPlanPaused(p.id, false, planName(p))}>{t("plans.resume")}</button>
              {:else}
                <button onclick={() => s.setPlanPaused(p.id, true, planName(p))}>{t("plans.pause")}</button>
              {/if}
            {/if}
            <button class="danger" onclick={() => s.stopPlan(p.id, planName(p))}>{t("plans.stop")}</button>
          </div>
        </div>
      {/each}

      <h3 class="sub">{t("memorize.title")}</h3>
      <!-- One card, always, even with nothing memorized yet: it is where today's
           memorization is accounted for. The per-day cap itself is a setting
           (Settings ▸ Verses to memorize per day), not a control on this card. -->
      <div class="plan-card" data-surface="memory-today">
        <div class="plan-head">
          <span class="plan-name">{t("memorize.title")}</span>
          <span class="plan-prog">
            {s.memorizePerDay > 0
              ? t("memorize.todayProgress", { done: reviewedToday, cap: s.memorizePerDay })
              : t("memorize.todayReviewed", { done: reviewedToday })}
          </span>
        </div>
        {#if todayRefs.length > 0}
          <button class="plan-today" onclick={reviewToday}>
            {plural("memorize.todayDue.one", "memorize.todayDue.other", todayRefs.length)}
          </button>
        {:else if dueTotal > 0 || reviewedToday > 0}
          <!-- Cards still due but the cap is spent, or the day's work is done:
               either way the tile asks nothing more today. -->
          <p class="plan-done">{t("memorize.todayDone")}</p>
        {:else}
          <p class="plan-done">{t("memorize.nothingDue")}</p>
        {/if}
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
      {#if !customOpen}
        <button class="plan-builtin" onclick={() => (customOpen = true)} data-surface="custom-plan-open">
          <span class="plan-name">{t("plans.custom")}</span>
          <span class="plan-add">{t("plans.start")}</span>
        </button>
      {:else}
        <div class="plan-card builder" data-surface="custom-plan">
          <div class="plan-head"><span class="plan-name">{t("plans.custom")}</span></div>
          <p class="hint">{t("plans.customHint")}</p>

          <h4 class="bsub">{t("plans.customParts")}</h4>
          <div class="chips" role="group" aria-label={t("plans.customParts")}>
            {#each parts as p (p.id)}
              <button class="chip" class:on={chosenParts.has(p.id)} aria-pressed={chosenParts.has(p.id)} onclick={() => togglePart(p.id)}>
                {p.name}
              </button>
            {/each}
          </div>

          <h4 class="bsub">{t("plans.customRange")}</h4>
          <div class="range" role="group" aria-label={t("plans.customRange")}>
            <label class="rlabel">{t("plans.customFrom")}
              <select bind:value={fromBook} onchange={() => (fromCh = Math.min(fromCh, chaptersOf(fromBook) || 1))} aria-label={t("plans.customFrom")}>
                {#each books as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
              </select>
              <input type="number" min="1" max={chaptersOf(fromBook) || 1} bind:value={fromCh} aria-label={t("plans.customFrom")} />
            </label>
            <label class="rlabel">{t("plans.customTo")}
              <select bind:value={toBook} onchange={() => (toCh = Math.min(toCh, chaptersOf(toBook) || 1))} aria-label={t("plans.customTo")}>
                {#each books as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
              </select>
              <input type="number" min="1" max={chaptersOf(toBook) || 1} bind:value={toCh} aria-label={t("plans.customTo")} />
            </label>
            <button class="badd" onclick={addRange}>{t("plans.customAdd")}</button>
          </div>

          <h4 class="bsub">{t("plans.customChosen")}</h4>
          {#if spans.length === 0}
            <p class="hint">{t("plans.customNone")}</p>
          {:else}
            <ul class="chosen">
              {#each parts.filter((p) => chosenParts.has(p.id)) as p (p.id)}
                <li><span>{p.name}</span><button class="bx" aria-label={t("plans.customRemove", { name: p.name })} onclick={() => togglePart(p.id)}>✕</button></li>
              {/each}
              {#each ranges as r, i (i)}
                <li><span>{rangeLabel(r)}</span><button class="bx" aria-label={t("plans.customRemove", { name: rangeLabel(r) })} onclick={() => removeRange(i)}>✕</button></li>
              {/each}
            </ul>
          {/if}

          <label class="rlabel days">{t("plans.customDays")}
            <input type="number" min="1" max="3650" bind:value={days} aria-label={t("plans.customDays")} />
          </label>
          {#if chapterTotal > 0}
            <p class="summary">{t("plans.customSummary", { chapters: nf.format(chapterTotal), perDay: nf1.format(perDay) })}</p>
          {/if}
          <div class="plan-actions">
            <button onclick={resetBuilder}>{t("plans.customCancel")}</button>
            <button class="primary" disabled={!canStart} onclick={startCustom}>{t("plans.customStart")}</button>
          </div>
        </div>
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
  /* Set aside: visibly at rest, not disabled — Resume is right there. */
  .plan-card.paused {
    background: transparent;
    border-style: dashed;
  }
  .plan-started {
    color: var(--faded, #8a8276);
    font-size: calc(12.5px * var(--uiScale, 1));
    margin: -6px 0 0;
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
    text-align: start;
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
  .plan-builtin {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    gap: 12px;
    margin-bottom: 8px;
    padding: 12px 16px;
    border-radius: 10px;
    text-align: start;
  }
  .plan-builtin:hover {
    border-color: var(--gold, #9e7d38);
  }
  /* "Available in English only" beside a booklet's name — a statement about
     the content, not a warning, so it sits quiet under the name. */
  .plan-note {
    display: block;
    color: var(--faded, #8a8276);
    font-size: calc(12.5px * var(--uiScale, 1));
  }
  .plan-builtin .plan-add {
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
    white-space: nowrap;
  }
  /* ── the custom plan builder ─────────────────────────────────────────── */
  .builder .hint {
    margin: 0;
  }
  .bsub {
    margin: 6px 0 0;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    font-weight: 600;
    letter-spacing: 0.03em;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    font-size: calc(13px * var(--uiScale, 1));
    padding: 6px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--faded, #8a8276);
  }
  .chip.on {
    color: var(--gold, #9e7d38);
    border-color: var(--gold, #9e7d38);
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .range {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px 12px;
  }
  .rlabel {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .rlabel select,
  .rlabel input {
    font-size: calc(14px * var(--uiScale, 1));
    padding: 6px 8px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
  }
  .rlabel input {
    width: 5.5em;
  }
  .rlabel select {
    max-width: 11em;
  }
  .badd,
  .plan-actions button.primary {
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    padding: 7px 14px;
    background: var(--paper, #fcf9f4);
    color: var(--gold, #9e7d38);
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .plan-actions button.primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .chosen {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .chosen li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 4px 0 4px 8px;
    border-inline-start: 2px solid var(--rule, #d8cba8);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .bx {
    color: var(--faded, #8a8276);
    padding: 4px 10px;
    font-size: calc(13px * var(--uiScale, 1));
  }
  .summary {
    margin: 0;
    font-size: calc(13.5px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
  }
</style>
