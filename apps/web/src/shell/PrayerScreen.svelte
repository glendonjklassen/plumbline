<script lang="ts">
  // PRAY — a role of its own beside Read, Study, Share and Sing.
  //
  // Two views of one collection (`core::prayer`): the prayer list, and the
  // prayers marked answered. A prayer has a title and a type; there is no
  // expiry — a prayer that no longer applies is deleted.
  //
  // The type decides whether a prayer can be answered: a request, an
  // intercession and an imprecation can; praise, thanks and lament stay on the list.
  // The types, what each can do and its example verse come off the wire
  // (`kinds`), so the shell holds no second copy of that table.
  //
  // Both views are newest first, filtered by type and by a range of days, and
  // shown a page at a time. The day a row is filed under is the day it was
  // added on the list and the day it was answered on Answered.
  //
  // The screen is the list. Everything that is not a prayer sits behind one of
  // three buttons at the end of the view row (maintainer, 2026-11-27): Filter
  // opens the filters, Edit turns on the controls that change a prayer, and New
  // opens the form for one. A row at rest carries only what is done to a prayer
  // every day: Prayed, and Answered where the type can be.
  //
  // "Prayed" is recorded once a day per prayer, in the reader's own calendar
  // day (`localDay`), and cannot be taken back: it is not a delete path, so the
  // restore merge can union the days from two devices without a record.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { localDay, nowStamp } from "../engine/StudyEngine";
  import { dateLocale, lang, plural, t } from "../lib/i18n.svelte";
  import { modal } from "../lib/modal";
  import { refDisplay } from "../reader/refname";
  import { dispatchLink } from "../study/links";

  const s = getSession();

  type Kind = { id: string; answerable: boolean; example: string; exampleLabel: string };
  /** One passage shown for Confession: its label and each verse's refKey. */
  type Passage = { label: string; verses: string[] };
  type Prayer = {
    id: string;
    title: string;
    kind: string;
    created: string;
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
  const kinds = $derived((listQ?.kinds ?? []) as Kind[]);
  // Confession is in the picker and is not a type: picking it shows verses to
  // read and takes the field away, so nothing confessed is ever put on a list.
  // Verses only — no copy of ours beside them (maintainer, 2026-11-25).
  const CONFESSION = "confession";
  const confession = $derived((listQ?.confession ?? []) as Passage[]);
  const answerable = $derived(new Set(kinds.filter((k) => k.answerable).map((k) => k.id)));

  let view = $state<"list" | "answered">("list");
  /** The prayer form: a new prayer (`id` null) or the one being edited, with
   *  its title and type as typed so far. Null while the form is closed. */
  let form = $state<{ id: string | null; title: string; kind: string } | null>(null);
  /** The type picked in the form; none while Confession is picked. */
  const formInfo = $derived(kinds.find((k) => k.id === form?.kind) ?? null);
  let filtersOpen = $state(false);

  // ── filters and pages ──
  const PAGE = 10;
  /** "" is every type. */
  let filterKind = $state("");
  /** `YYYY-MM-DD` from a date field, or "" for no bound. */
  let from = $state("");
  let to = $state("");
  let page = $state(0);
  const filtering = $derived(filterKind !== "" || from !== "" || to !== "");

  /** A UTC stamp's day on the reader's own calendar, to compare with the date
   *  fields, which are the reader's days too. */
  function stampDay(stamp: string): string {
    const d = new Date(stamp);
    if (Number.isNaN(d.getTime())) return stamp.slice(0, 10);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  }
  /** The day a row is filed under in the view it is shown in. */
  function rowDay(p: Prayer): string {
    return p.answered ?? stampDay(p.created);
  }
  const rows = $derived(view === "list" ? praying : answered);
  const shown = $derived(
    rows.filter((p) => {
      if (filterKind && p.kind !== filterKind) return false;
      const day = rowDay(p);
      return (!from || day >= from) && (!to || day <= to);
    }),
  );
  const pages = $derived(Math.max(1, Math.ceil(shown.length / PAGE)));
  // A page past the end — the last row of the last page was answered or
  // deleted — falls back to the last page there is.
  const pageAt = $derived(Math.min(page, pages - 1));
  const pageRows = $derived(shown.slice(pageAt * PAGE, pageAt * PAGE + PAGE));
  /** A change of view or filter starts again at the first page. */
  function firstPage(): void {
    page = 0;
  }
  function show(v: "list" | "answered"): void {
    view = v;
    // Never found already armed in the view it was not turned on in.
    editing = false;
    // Answered has no praise or thanks to filter to.
    if (v === "answered" && filterKind && !answerable.has(filterKind)) filterKind = "";
    firstPage();
  }
  function clearFilters(): void {
    filterKind = "";
    from = "";
    to = "";
    firstPage();
  }
  /** Edit is on: each row shows its ✕ and the control that changes it — Edit
   *  on the list, the way back to the list on Answered. Off again when the
   *  view is left or its last row goes. */
  let editing = $state(false);
  $effect(() => {
    if (rows.length === 0) editing = false;
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

  function openNew(): void {
    form = { id: null, title: "", kind: "request" };
  }
  function openEdit(p: Prayer): void {
    form = { id: p.id, title: p.title, kind: p.kind };
  }

  async function saveForm(ev: SubmitEvent): Promise<void> {
    ev.preventDefault();
    const f = form;
    const title = f?.title.trim();
    if (!f || !title || f.kind === CONFESSION) return;
    form = null;
    if (f.id === null) {
      // The new prayer leads the first page of the list.
      view = "list";
      page = 0;
      await run(s.author("prayerAdd", title, f.kind, nowStamp()));
      return;
    }
    const was = praying.find((p) => p.id === f.id);
    if (was && title === was.title && f.kind === was.kind) return;
    await run(s.author("prayerEdit", f.id, title, f.kind, nowStamp()));
  }

  /** Open a verse in the reader. refKey → the core's `go:` verb, split on the
   *  LAST space, as App.svelte's `goUri` does. */
  function goRef(refKey: string): void {
    form = null;
    void dispatchLink(s, `go:${refKey.replace(/ (?=\S*$)/, ":")}`);
  }
  /** A verse's text in the reader's Bible; empty until the engine answers. */
  function verseText(refKey: string): string {
    return s.q("verse", refKey)?.body ?? "";
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
      <button class="chip" class:on={view === "list"} aria-pressed={view === "list"} onclick={() => show("list")}>
        {t("prayer.list")}
        {#if praying.length > 0}<span class="n">{nf.format(praying.length)}</span>{/if}
      </button>
      <button
        class="chip"
        class:on={view === "answered"}
        aria-pressed={view === "answered"}
        onclick={() => show("answered")}
      >
        {t("prayer.answered")}
        {#if answered.length > 0}<span class="n">{nf.format(answered.length)}</span>{/if}
      </button>
      <!-- Filter · Edit · New, at the far end. Filter and Edit act on rows, so
           they are off with none; New is always there. -->
      <div class="tools" data-surface="prayer-tools">
        <button
          class="tool"
          class:on={filtering}
          disabled={rows.length === 0}
          aria-label={t("prayer.filter")}
          title={t("prayer.filter")}
          aria-haspopup="dialog"
          data-filtering={filtering}
          onclick={() => (filtersOpen = true)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5h16l-6 7.5V19l-4-2v-4.5z" /></svg>
        </button>
        <button
          class="tool"
          class:on={editing}
          disabled={rows.length === 0}
          aria-label={t("prayer.edit")}
          title={t("prayer.edit")}
          aria-pressed={editing}
          onclick={() => (editing = !editing)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20l1-4.5L16.5 4l3.5 3.5L8.5 19z" /><path d="M14 6.5l3.5 3.5" /></svg>
        </button>
        <button
          class="tool"
          aria-label={t("prayer.chipAdd")}
          title={t("prayer.chipAdd")}
          aria-haspopup="dialog"
          onclick={openNew}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
        </button>
      </div>
    </div>

    {#if listQ != null && rows.length === 0}
      <p class="empty">{t(view === "list" ? "prayer.empty" : "prayer.answeredEmpty")}</p>
    {:else if rows.length > 0 && shown.length === 0}
      <p class="empty">{t("prayer.noMatch")}</p>
    {/if}

    {#if view === "list"}
      <!-- Each item: the title, its type and the day it was added, then one
           line holding the days prayed and, at the far end, what is done to a
           prayer day to day. Edit adds the ✕ and the Edit button. -->
      <ul class="list" data-surface="prayer-list">
        {#each pageRows as p (p.id)}
          <li class="item" data-kind={p.kind}>
            <div class="head">
              <span class="title">{p.title}</span>
              {#if editing}
                <button class="x" aria-label={t("prayer.delete")} title={t("prayer.delete")} onclick={() => remove(p)}>✕</button>
              {/if}
            </div>
            <span class="note">{t(`prayer.kind.${p.kind}`)} · {dayLabel(stampDay(p.created))}</span>
            <div class="foot">
              {#if p.prayedDays > 0}
                <span class="note">
                  {plural("prayer.prayedDays.one", "prayer.prayedDays.other", p.prayedDays, { n: nf.format(p.prayedDays) })}
                </span>
              {/if}
              <div class="actions">
                {#if editing}
                  <button onclick={() => openEdit(p)}>{t("prayer.edit")}</button>
                {:else}
                  <button
                    class="primary"
                    disabled={p.prayedToday}
                    onclick={() => run(s.author("prayerPrayed", p.id, localDay()))}
                  >
                    {t(p.prayedToday ? "prayer.prayedToday" : "prayer.prayed")}
                  </button>
                  {#if answerable.has(p.kind)}
                    <button onclick={() => answer(p)}>{t("prayer.markAnswered")}</button>
                  {/if}
                {/if}
              </div>
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <ul class="list" data-surface="prayer-answered">
        {#each pageRows as p (p.id)}
          <li class="item" data-kind={p.kind}>
            <div class="head">
              <span class="title">{p.title}</span>
              {#if editing}
                <button class="x" aria-label={t("prayer.delete")} title={t("prayer.delete")} onclick={() => remove(p)}>✕</button>
              {/if}
            </div>
            <span class="note">
              {t(`prayer.kind.${p.kind}`)} · {t("prayer.answeredOn", { date: dayLabel(p.answered ?? "") })}
            </span>
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

    {#if pages > 1}
      <div class="pager" data-surface="prayer-pager">
        <button disabled={pageAt === 0} onclick={() => (page = pageAt - 1)}>{t("prayer.pagePrev")}</button>
        <span class="note">{t("prayer.pageOf", { n: nf.format(pageAt + 1), total: nf.format(pages) })}</span>
        <button disabled={pageAt >= pages - 1} onclick={() => (page = pageAt + 1)}>{t("prayer.pageNext")}</button>
      </div>
    {/if}
  </div>
</section>

{#if filtersOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (filtersOpen = false)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t("prayer.filter")}
    tabindex="-1"
    data-surface="prayer-filters"
    use:modal={{ close: () => (filtersOpen = false) }}
  >
    <h2>{t("prayer.filter")}</h2>
    <label class="field">
      <span>{t("prayer.kindLabel")}</span>
      <select bind:value={filterKind} onchange={firstPage}>
        <option value="">{t("prayer.filterAll")}</option>
        {#each kinds as k (k.id)}
          <!-- Answered holds only the types that can be. -->
          {#if view === "list" || k.answerable}
            <option value={k.id}>{t(`prayer.kind.${k.id}`)}</option>
          {/if}
        {/each}
      </select>
    </label>
    <label class="field">
      <span>{t("prayer.filterFrom")}</span>
      <input type="date" bind:value={from} max={to || undefined} onchange={firstPage} />
    </label>
    <label class="field">
      <span>{t("prayer.filterTo")}</span>
      <input type="date" bind:value={to} min={from || undefined} onchange={firstPage} />
    </label>
    <div class="dialog-actions">
      <button disabled={!filtering} onclick={clearFilters}>{t("common.clear")}</button>
      <button class="primary" onclick={() => (filtersOpen = false)}>{t("prayer.editDone")}</button>
    </div>
  </div>
{/if}

{#if form}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (form = null)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t(form.id === null ? "prayer.new" : "prayer.editTitle")}
    tabindex="-1"
    data-surface="prayer-form"
    use:modal={{ close: () => (form = null) }}
  >
   <form onsubmit={saveForm}>
    <h2>{t(form.id === null ? "prayer.new" : "prayer.editTitle")}</h2>
    <label class="field">
      <span>{t("prayer.kindLabel")}</span>
      <select bind:value={form.kind} data-surface="prayer-kind">
        {#each kinds as k (k.id)}
          <option value={k.id}>{t(`prayer.kind.${k.id}`)}</option>
        {/each}
        <!-- Confession is read, not kept, so it is offered for a new prayer
             and never as a type to change one to. -->
        {#if form.id === null && confession.length > 0}
          <option value={CONFESSION}>{t("prayer.kind.confession")}</option>
        {/if}
      </select>
    </label>
    {#if formInfo}
      <!-- What the picked type is, with a prayer of that type to read. -->
      <p class="kind-info" data-surface="prayer-kind-info">
        {t(`prayer.kindDef.${formInfo.id}`)}
        <button type="button" class="ref" onclick={() => goRef(formInfo.example)}>
          {refDisplay(s, formInfo.exampleLabel)}
        </button>
      </p>
    {/if}
    {#if form.kind === CONFESSION}
      <!-- The devotional's passage block: a reference that opens the reader,
           then the verses. No field: nothing confessed is put on a list. -->
      <div class="confession" data-surface="prayer-confession">
        {#each confession as c (c.label)}
          <div class="passage">
            <button type="button" class="ref" onclick={() => goRef(c.verses[0])}>{refDisplay(s, c.label)}</button>
            <p class="verses">
              {#each c.verses as v (v)}<span class="v"
                  >{#if c.verses.length > 1}<span class="n">{v.slice(v.lastIndexOf(":") + 1)}</span>{/if}{verseText(v)}
                </span>{/each}
            </p>
          </div>
        {/each}
      </div>
    {:else}
      <input
        data-modal-focus
        bind:value={form.title}
        placeholder={t("prayer.addPlaceholder")}
        aria-label={t("prayer.addPlaceholder")}
      />
    {/if}
    <div class="dialog-actions">
      <button type="button" onclick={() => (form = null)}>{t("common.cancel")}</button>
      {#if form.kind !== CONFESSION}
        <button type="submit" class="primary" disabled={!form.title.trim()}>{t("common.save")}</button>
      {/if}
    </div>
   </form>
  </div>
{/if}

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
  /* Filter · Edit · New: three icon buttons at the far end of the view row. */
  .tools {
    margin-inline-start: auto;
    display: flex;
    gap: 6px;
  }
  .tool {
    width: 40px;
    height: 40px;
    min-height: auto;
    padding: 0;
    display: grid;
    place-items: center;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--faded, #8a8276);
  }
  .tool svg {
    width: 20px;
    height: 20px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .tool:hover:not(:disabled) {
    color: var(--ink, #211f1a);
  }
  .tool.on {
    color: var(--gold, #9e7d38);
    border-color: var(--gold, #9e7d38);
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .tool:disabled {
    opacity: 0.4;
  }
  .chip .n {
    margin-inline-start: 4px;
    font-variant-numeric: tabular-nums;
  }
  /* The two dialogs: PromptDialog's box, at its level. */
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 44;
  }
  .dialog {
    position: fixed;
    z-index: 45;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, 92vw);
    max-height: 76vh;
    overflow-y: auto;
    box-sizing: border-box;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    padding: 16px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .dialog form {
    display: contents;
  }
  .dialog h2 {
    margin: 0;
    font-size: calc(15px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .dialog input,
  .dialog select {
    width: 100%;
    box-sizing: border-box;
    padding: 7px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(16px * var(--uiScale, 1));
    min-width: 0;
  }
  .dialog input:focus,
  .dialog select:focus {
    outline: none;
    border-color: var(--gold, #9e7d38);
  }
  /* The picker's own chevron, in place of the browser's: that one sits hard
     against the border and takes its colour from the system, not the theme.
     Two half-filled squares meet as a ⌄ in the text colour. */
  .dialog select {
    appearance: none;
    padding-inline-end: 30px;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--faded, #8a8276) 50%),
      linear-gradient(135deg, var(--faded, #8a8276) 50%, transparent 50%);
    background-position:
      calc(100% - 16px) 50%,
      calc(100% - 11px) 50%;
    background-size: 5px 5px;
    background-repeat: no-repeat;
  }
  .dialog select:dir(rtl) {
    background-position:
      11px 50%,
      16px 50%;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .dialog-actions button,
  .pager button {
    padding: 6px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .dialog-actions button:disabled {
    opacity: 0.4;
  }
  .dialog-actions .primary:not(:disabled) {
    background: var(--gold, #9e7d38);
    color: #fff;
    border-color: var(--gold, #9e7d38);
  }
  .pager {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .pager button:disabled {
    opacity: 0.4;
  }
  .kind-info {
    margin: -6px 0 0;
    font-size: calc(13.5px * var(--uiScale, 1));
    line-height: 1.45;
    color: var(--faded, #8a8276);
  }
  .passage {
    margin: 0 0 10px;
    padding: 10px 12px;
    border-inline-start: 3px solid var(--gold, #9e7d38);
    background: var(--paneNavBg, #efeae1);
    border-radius: 0 6px 6px 0;
  }
  .verses {
    margin: 4px 0 0;
    font-size: calc(16px * var(--uiScale, 1));
    line-height: 1.5;
    color: var(--ink, #211f1a);
  }
  .n {
    margin-inline-end: 3px;
    color: var(--faded, #8a8276);
    font-size: 0.72em;
    vertical-align: super;
    font-variant-numeric: tabular-nums;
  }
  /* A reference that opens the reader, as the memorize list's does. */
  .passage .ref,
  .kind-info .ref {
    padding: 4px 0;
    min-height: auto;
    border: none;
    background: none;
    color: var(--gold, #9e7d38);
    font-size: inherit;
    text-decoration: underline;
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
