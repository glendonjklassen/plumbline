<script lang="ts">
  // One day of a devotional, as its own DESTINATION — a full page like the
  // hymnal, not a study-panel block (maintainer, 2026-08-26). The distinction
  // is what the surface is about: the study panel annotates the verse you were
  // already looking at, and a devotional is somewhere you go to read.
  //
  // The order down the page is the booklet's own, with one addition the paper
  // cannot make: THE PASSAGE ITSELF, set beneath the title, so the reading and
  // the reflection on it are not two places. The engine hands over the
  // reference structured (`{book, chapter, verse, end}`), never as the string
  // "John 3:16–21", so the text comes from whichever corpus this reader is in
  // and the label is written their way ("Johannes 3,16–21").
  //
  // Verses are fetched one `q("verse", …)` at a time, the idiom PassagePicker
  // already uses for a range: they are cached per session, and a day is a few
  // of them. Nothing here waits on all of them — a verse that has not landed
  // yet simply has not landed, and the paragraph above it is already readable.
  import { getSession } from "../state/session.svelte";
  import { t } from "../lib/i18n.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";

  const s = getSession();

  const at = $derived(s.devotionalAt);
  const wire = $derived(s.devotionals());
  /** The run this page belongs to, for its name and its banked days. */
  const run = $derived(((wire?.running ?? []) as any[]).find((r) => r.id === at?.id) ?? null);
  const booklet = $derived(((wire?.catalogue ?? []) as any[]).find((b) => b.id === at?.id) ?? null);
  const name = $derived(run?.name ?? booklet?.name ?? "");
  const total = $derived(run?.daysTotal ?? booklet?.days ?? 0);

  /** The open day's content. The running answer already carries the day it is
   *  ON, so reading THAT day costs no second call; any other day (browsed back
   *  to) is its own cheap read. */
  const entry = $derived.by(() => {
    if (!at) return null;
    const open = run?.today;
    if (open && open.day === at.day) return open;
    return s.q("devotionalDay", at.id, at.day, "");
  });

  /** Whether this day has already been banked — the ONLY thing that says a day
   *  was read, since nothing observable says a reflection was reflected on. A
   *  day below the open one has been banked by definition (the open day is the
   *  lowest unbanked one); at or above it, it has not. */
  const done = $derived.by(() => {
    const open = run?.today?.day;
    if (!at) return false;
    if (open === undefined) return true; // the booklet is finished — every day is
    return at.day < open;
  });

  /** How this reference reads, in the reader's language. The label is DERIVED
   *  rather than stored, so German writes "Johannes 3,16–21" from the same
   *  data — the welcome screen's quotes are built the same way. */
  function label(r: { book: string; chapter: number; verse: number; end?: number }): string {
    const book = s.bookName(r.book);
    return r.end === undefined
      ? t("ref.verse", { book, chapter: r.chapter, verse: r.verse })
      : t("ref.range", { book, chapter: r.chapter, verse: r.verse, end: r.end });
  }

  /** The verse numbers one range covers. */
  function span(r: { verse: number; end?: number }): number[] {
    const out: number[] = [];
    for (let v = r.verse; v <= (r.end ?? r.verse); v++) out.push(v);
    return out;
  }

  function verseText(book: string, chapter: number, verse: number): string {
    return s.q("verse", `${book} ${chapter}:${verse}`)?.body ?? "";
  }

  /** Open the passage in the reader. The devotional stays where it is — a
   *  reader who wants the surrounding chapter is going there, and the back
   *  arrow is how they come back. */
  function openInReader(r: { book: string; chapter: number; verse: number }): void {
    s.goRead();
    s.navigate(s.activePane, r.book, r.chapter, r.verse);
  }

  async function markDone(): Promise<void> {
    if (!at || done) return;
    await s.markDevotionalDone(at.id, at.day);
  }

  function goDay(day: number): void {
    if (!at || day < 1 || day > total) return;
    s.devotionalAt = { id: at.id, day };
  }
</script>

<section class="screen" aria-label={t("devotional.heading")}>
  <ScreenBar
    title={name}
    onBack={() => (s.screen = "explore")}
    backLabel={t("nav.study")}
    onMenu={() => (s.menuOpen = true)}
  />
  <div class="content">
    {#if !at || !entry}
      <!-- A placeholder of the same shape rather than a spinner: the day line
           and the title are what the page resolves to, so the passage below
           starts where it will stay. -->
      <div class="skeleton" aria-hidden="true">
        <div class="ghost line"></div>
        <div class="ghost title"></div>
        <div class="ghost block"></div>
      </div>
    {:else}
      <p class="dayline">
        {t("devotional.dayOf", { day: at.day, total })}
        {#if entry.section}
          <span class="section">{entry.section.title}</span>
        {/if}
      </p>

      <h1>{entry.title}</h1>

      <!-- THE PASSAGE, beneath the title. Each range is its own block with its
           own reference; tapping one opens it in the reader. -->
      {#each entry.scripture ?? [] as ref, i (i)}
        <div class="passage">
          <button class="ref" onclick={() => openInReader(ref)}>{label(ref)}</button>
          <p class="verses">
            {#each span(ref) as v (v)}<span class="v"
                ><span class="n">{v}</span>{verseText(ref.book, ref.chapter, v)}
              </span>{/each}
          </p>
        </div>
      {/each}

      {#each entry.reflection ?? [] as para, i (i)}
        <p class="reflection">{para}</p>
      {/each}

      {#if entry.activity}
        <div class="activity">
          <h2>{t("devotional.activity")}</h2>
          <p>{entry.activity}</p>
        </div>
      {/if}

      <!-- The booklet's send-off, at the foot of its LAST day rather than on a
           page of its own (maintainer, 2026-08-26): finishing day 30 and being
           sent off are one act. -->
      {#each entry.closing ?? [] as para, i (i)}
        <p class="closing">{para}</p>
      {/each}

      <div class="foot">
        {#if done}
          <span class="already">{t("devotional.doneAlready")}</span>
        {:else}
          <button class="done" onclick={markDone}>{t("devotional.done")}</button>
        {/if}
        <!-- Out to the TEXT, not up to Study. The ‹ in the bar goes back where
             the reader came from; this is the other thing someone wants when
             they have finished an entry, and it is the reader (maintainer,
             2026-08-26). Named Close rather than Done because "Mark as read"
             is already the Done on this page. -->
        <button class="close" onclick={() => s.goRead()}>{t("common.close")}</button>
        <!-- Browsing the booklet. Forward stops at the day the reader has
             reached: a devotional that let you read all thirty tonight would
             not be a daily one. -->
        <div class="steps">
          <button disabled={at.day <= 1} onclick={() => goDay(at.day - 1)} aria-label={t("devotional.day", { day: at.day - 1 })}>‹</button>
          <button
            disabled={at.day >= Math.min(total, run?.today?.day ?? total)}
            onclick={() => goDay(at.day + 1)}
            aria-label={t("devotional.day", { day: at.day + 1 })}>›</button
          >
        </div>
      </div>

      {#if !run?.today}
        <p class="next">{t("devotional.finished")}</p>
      {/if}
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
    padding: 16px;
    /* A reading measure, centred — this is prose, and a full-width line on a
       desktop is the one thing that makes it unreadable. */
    max-width: 42rem;
    width: 100%;
    margin: 0 auto;
  }
  .dayline {
    margin: 0 0 4px;
    color: var(--faded, #8a8276);
    font-size: calc(13px * var(--uiScale, 1));
    font-variant-numeric: tabular-nums;
  }
  .section::before {
    content: " · ";
  }
  h1 {
    margin: 0 0 14px;
    font-size: calc(23px * var(--uiScale, 1));
    line-height: 1.25;
    color: var(--ink, #211f1a);
  }
  /* The passage reads as SCRIPTURE and not as more of the booklet's prose:
     ruled off, in the reader's own gold for its reference. */
  .passage {
    margin: 0 0 16px;
    padding: 10px 12px;
    border-inline-start: 3px solid var(--gold, #9e7d38);
    background: var(--paneNavBg, #efeae1);
    border-radius: 0 6px 6px 0;
  }
  .ref {
    display: block;
    margin-bottom: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
    cursor: pointer;
  }
  .ref:hover {
    text-decoration: underline;
  }
  .verses {
    margin: 0;
    color: var(--ink, #211f1a);
    font-size: calc(15.5px * var(--uiScale, 1));
    line-height: 1.55;
  }
  .n {
    margin-inline-end: 3px;
    color: var(--faded, #8a8276);
    font-size: 0.72em;
    vertical-align: super;
    font-variant-numeric: tabular-nums;
  }
  .reflection {
    margin: 0 0 12px;
    color: var(--ink, #211f1a);
    font-size: calc(16px * var(--uiScale, 1));
    line-height: 1.6;
  }
  .activity {
    margin: 18px 0 0;
    padding: 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
  }
  .activity h2 {
    margin: 0 0 6px;
    font-size: calc(13px * var(--uiScale, 1));
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--gold, #9e7d38);
  }
  .activity p {
    margin: 0;
    color: var(--ink, #211f1a);
    font-size: calc(15.5px * var(--uiScale, 1));
    line-height: 1.55;
  }
  .closing {
    margin: 18px 0 0;
    color: var(--ink, #211f1a);
    font-size: calc(16px * var(--uiScale, 1));
    line-height: 1.6;
    font-style: italic;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 22px 0 0;
    padding-top: 14px;
    border-top: 1px solid var(--rule, #d8cba8);
  }
  .done {
    height: calc(44px * var(--uiScale, 1));
    padding: 0 calc(18px * var(--uiScale, 1));
    border-radius: 999px;
    border: 1px solid var(--gold, #9e7d38);
    background: var(--gold, #9e7d38);
    color: var(--paper, #fcf9f4);
    font-size: calc(15px * var(--uiScale, 1));
    cursor: pointer;
  }
  .already {
    color: var(--faded, #8a8276);
    font-size: calc(14px * var(--uiScale, 1));
  }
  /* Quiet beside the gold Mark-as-read: leaving is not the thing the page is
     asking for. */
  .close {
    height: calc(44px * var(--uiScale, 1));
    padding: 0 calc(16px * var(--uiScale, 1));
    border-radius: 999px;
    border: 1px solid var(--rule, #d8cba8);
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(15px * var(--uiScale, 1));
    cursor: pointer;
  }
  .close:hover {
    border-color: var(--gold, #9e7d38);
  }
  .steps {
    margin-inline-start: auto;
    display: flex;
    gap: 8px;
  }
  .steps button {
    width: calc(44px * var(--uiScale, 1));
    height: calc(44px * var(--uiScale, 1));
    border-radius: 999px;
    border: 1px solid var(--rule, #d8cba8);
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(18px * var(--uiScale, 1));
    cursor: pointer;
  }
  .steps button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .next {
    margin: 10px 0 0;
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .ghost {
    background: var(--paneNavBg, #efeae1);
    border-radius: 6px;
  }
  .ghost.line {
    height: 14px;
    width: 40%;
    margin-bottom: 10px;
  }
  .ghost.title {
    height: 26px;
    width: 70%;
    margin-bottom: 14px;
  }
  .ghost.block {
    height: 120px;
  }
</style>
