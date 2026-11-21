<script lang="ts">
  // PRAY — a role of its own beside Read, Study, Share and Sing.
  //
  // Two views of one collection (`core::prayer`): the prayer list, and the
  // prayers marked answered. A prayer has a title and nothing else to fill in;
  // there is no expiry — a prayer that no longer applies is deleted.
  //
  // "Prayed" is recorded once a day per prayer, in the reader's own calendar
  // day (`localDay`), and cannot be taken back: it is not a delete path, so the
  // restore merge can union the days from two devices without a record.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { localDay, nowStamp } from "../engine/StudyEngine";
  import { dateLocale, lang, plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  type Prayer = {
    id: string;
    title: string;
    prayedDays: number;
    prayedToday: boolean;
    answered?: string;
    answer?: string;
  };

  // `localDay()` is part of the cache key, so the list re-reads on a new day.
  // `qStale`: every write invalidates the cache, and a list that blanked and
  // refilled on each tap of "Prayed" would move the row under the thumb. Rows
  // are keyed and acted on by id, never by position, so a stale beat is safe.
  const listQ = $derived(s.qStale("prayers", localDay()));
  const praying = $derived((listQ?.praying ?? []) as Prayer[]);
  const answered = $derived((listQ?.answered ?? []) as Prayer[]);

  let view = $state<"list" | "answered">("list");
  let draft = $state("");
  /** Answered prayers are a record to read, so they carry no controls until
   *  Edit is on: then each shows its ✕ and the way back to the list. Off again
   *  whenever the view is left, so it is never found already armed. */
  let editing = $state(false);
  $effect(() => {
    if (view !== "answered" || answered.length === 0) editing = false;
  });
  const nf = $derived(new Intl.NumberFormat(lang()));

  /** "30 Jul 2026" from `YYYY-MM-DD`, in the reader's language. Parsed by parts:
   *  `new Date("2026-07-30")` is UTC midnight, which is the 29th in the Americas. */
  function dayLabel(ymd: string): string {
    const [y, m, d] = ymd.split("-").map(Number);
    if (!y || !m || !d) return ymd;
    return new Date(y, m - 1, d).toLocaleDateString(dateLocale(), { day: "numeric", month: "short", year: "numeric" });
  }

  async function run(call: Promise<string | null>, done?: string): Promise<void> {
    const err = await call;
    if (err) s.showToast(err);
    else if (done) s.showToast(done);
  }

  async function add(ev: SubmitEvent): Promise<void> {
    ev.preventDefault();
    const title = draft.trim();
    if (!title) return;
    draft = "";
    await run(s.author("prayerAdd", title, nowStamp()));
  }

  async function edit(p: Prayer): Promise<void> {
    const title = await s.askText(t("prayer.editTitle"), p.title);
    if (title === null || !title.trim() || title.trim() === p.title) return;
    await run(s.author("prayerEdit", p.id, title.trim(), nowStamp()));
  }

  /** The comment is optional: OK on an empty field answers with none, and only
   *  Cancel leaves the prayer on the list. */
  async function answer(p: Prayer): Promise<void> {
    const comment = await s.askText(t("prayer.answerComment"), "");
    if (comment === null) return;
    await run(s.author("prayerAnswer", p.id, comment.trim(), localDay(), nowStamp()), t("prayer.movedToAnswered"));
  }

  async function remove(p: Prayer): Promise<void> {
    const ok = await s.askConfirm(t("prayer.deleteAsk", { title: p.title }), t("prayer.deleteBody"));
    if (!ok) return;
    await run(s.author("prayerDelete", p.id));
  }
</script>

<section class="screen" aria-label={t("prayer.title")}>
  <ScreenBar top title={t("prayer.title")} onBack={() => s.goRead()} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <div class="views" role="group" aria-label={t("prayer.title")}>
      <button class="chip" class:on={view === "list"} aria-pressed={view === "list"} onclick={() => (view = "list")}>
        {t("prayer.list")}
        {#if praying.length > 0}<span class="n">{nf.format(praying.length)}</span>{/if}
      </button>
      <button
        class="chip"
        class:on={view === "answered"}
        aria-pressed={view === "answered"}
        onclick={() => (view = "answered")}
      >
        {t("prayer.answered")}
        {#if answered.length > 0}<span class="n">{nf.format(answered.length)}</span>{/if}
      </button>
      {#if view === "answered" && answered.length > 0}
        <button class="chip edit" class:on={editing} aria-pressed={editing} onclick={() => (editing = !editing)}>
          {t(editing ? "prayer.editDone" : "prayer.edit")}
        </button>
      {/if}
    </div>

    {#if view === "list"}
      <form class="add" onsubmit={add}>
        <input bind:value={draft} placeholder={t("prayer.addPlaceholder")} aria-label={t("prayer.addPlaceholder")} />
        <!-- ＋ beside a "New …" field, as the tag and thread pickers add. -->
        <button type="submit" disabled={!draft.trim()} aria-label={t("prayer.chipAdd")} title={t("prayer.chipAdd")}>
          {t("prayer.add")}
        </button>
      </form>
      {#if listQ != null && praying.length === 0}
        <p class="empty">{t("prayer.empty")}</p>
      {/if}
      <!-- Each item: the title with its ✕ in the top corner, then one line
           holding the days prayed and, at the far end, what can be done. -->
      <ul class="list" data-surface="prayer-list">
        {#each praying as p (p.id)}
          <li class="item">
            <div class="head">
              <span class="title">{p.title}</span>
              <button class="x" aria-label={t("prayer.delete")} title={t("prayer.delete")} onclick={() => remove(p)}>✕</button>
            </div>
            <div class="foot">
              {#if p.prayedDays > 0}
                <span class="note">
                  {plural("prayer.prayedDays.one", "prayer.prayedDays.other", p.prayedDays, { n: nf.format(p.prayedDays) })}
                </span>
              {/if}
              <div class="actions">
                <button
                  class="primary"
                  disabled={p.prayedToday}
                  onclick={() => run(s.author("prayerPrayed", p.id, localDay()))}
                >
                  {t(p.prayedToday ? "prayer.prayedToday" : "prayer.prayed")}
                </button>
                <button onclick={() => answer(p)}>{t("prayer.markAnswered")}</button>
                <button onclick={() => edit(p)}>{t("prayer.edit")}</button>
              </div>
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      {#if listQ != null && answered.length === 0}
        <p class="empty">{t("prayer.answeredEmpty")}</p>
      {/if}
      <ul class="list" data-surface="prayer-answered">
        {#each answered as p (p.id)}
          <li class="item">
            <div class="head">
              <span class="title">{p.title}</span>
              {#if editing}
                <button class="x" aria-label={t("prayer.delete")} title={t("prayer.delete")} onclick={() => remove(p)}>✕</button>
              {/if}
            </div>
            <span class="note">{t("prayer.answeredOn", { date: dayLabel(p.answered ?? "") })}</span>
            {#if p.answer}<p class="answer">{p.answer}</p>{/if}
            <div class="foot">
              {#if p.prayedDays > 0}
                <span class="note">
                  {plural("prayer.prayedDays.one", "prayer.prayedDays.other", p.prayedDays, { n: nf.format(p.prayedDays) })}
                </span>
              {/if}
              {#if editing}
                <div class="actions">
                  <button onclick={() => run(s.author("prayerReopen", p.id, nowStamp()))}>{t("prayer.reopen")}</button>
                </div>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
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
    width: 100%;
    max-width: 720px;
    margin: 0 auto;
    box-sizing: border-box;
  }
  .views {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  /* The hymnal's filter chips, to the pixel. */
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
  /* The Answered view's Edit toggle, at the far end of the row. */
  .chip.edit {
    margin-inline-start: auto;
  }
  .chip .n {
    margin-inline-start: 4px;
    font-variant-numeric: tabular-nums;
  }
  .add {
    display: flex;
    gap: 8px;
  }
  /* The pickers' add row: a field on paper and a gold-outlined ＋. */
  .add input {
    flex: 1;
    min-width: 0;
    padding: 5px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(16px * var(--uiScale, 1));
  }
  .add input:focus {
    outline: none;
    border-color: var(--gold, #9e7d38);
  }
  .add button {
    padding: 5px 14px;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .add button:disabled {
    opacity: 0.4;
  }
  .actions button {
    padding: 8px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    background: var(--popupPaper, #f2eee6);
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .actions button:hover:not(:disabled) {
    border-color: var(--gold, #9e7d38);
  }
  .actions button:disabled {
    opacity: 0.55;
  }
  .actions .primary:not(:disabled) {
    font-weight: 600;
    color: var(--gold, #9e7d38);
    border-color: var(--gold, #9e7d38);
  }
  /* Delete: a ✕ in the item's top corner, as the sermon editor's rows have,
     not a labelled button among the things a prayer is for. */
  .x {
    flex: none;
    /* Pulled into the corner: the glyph, not its tap area, lines up with the
       actions' right edge below. */
    margin: -6px 0 0;
    margin-inline-end: -8px;
    width: 36px;
    height: 36px;
    min-height: auto;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--faded, #8a8276);
    font-size: calc(15px * var(--uiScale, 1));
  }
  .x:hover {
    background: var(--popupPaper, #f2eee6);
    color: var(--ink, #211f1a);
  }
  .empty {
    margin: 0;
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 2px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  /* The days prayed at the start, the actions pushed to the end; the actions
     drop to their own line, still end-aligned, when the two do not fit. */
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-size: calc(16.5px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
    overflow-wrap: anywhere;
  }
  .note {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .answer {
    margin: 2px 0;
    font-size: calc(15px * var(--uiScale, 1));
    line-height: 1.45;
    color: var(--ink, #211f1a);
    overflow-wrap: anywhere;
  }
  .actions {
    margin-inline-start: auto;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
