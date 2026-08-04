<script lang="ts">
  // Passage navigator: OT/NT → book → chapter, and you're there. Two taps.
  //
  // There is no verse step (2026-07-26). Picking a verse cost a throwaway
  // layout of the whole chapter just to count its verse numbers — an async
  // round trip on every chapter tap, which on a phone is a visible wait
  // before the grid even appears. Book and chapter is the navigation people
  // actually use; verse targeting still happens through links, cross-refs
  // and search, which arrive with a verse already in hand.
  //
  // Everything this dialog needs is the TOC, prefetched at boot — so the
  // grids are synchronous, and Joel's chapter count is on screen instantly.
  import { untrack } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { readingTint, tintStyle, tintTitle, type ReadingHeat } from "./readingTint";
  import { lang, t } from "../lib/i18n.svelte";

  const s = getSession();

  const toc = $derived(s.q("toc"));
  const seg = $derived(s.q("canonSegments"));

  // The reading map (core::reading): each tile tinted by where you stand and
  // how long it has been. Read through the cache like every other engine
  // query, so the grids stay synchronous and the tint fills in a beat later
  // without moving anything on screen.
  const nowStamp = () => new Date().toISOString();
  const bookHeat = $derived.by(() => {
    if (!open) return new Map<string, ReadingHeat>();
    const r = s.q("readingBooks", nowStamp().slice(0, 10) + "T12:00:00Z");
    return new Map<string, ReadingHeat>((r?.books ?? []).map((b: any) => [b.book, b as ReadingHeat]));
  });
  const chapterHeat = $derived.by(() => {
    if (!book) return new Map<number, ReadingHeat>();
    const r = s.q("readingChapters", book, nowStamp().slice(0, 10) + "T12:00:00Z");
    return new Map<number, ReadingHeat>((r?.chapters ?? []).map((c: any) => [c.chapter, c as ReadingHeat]));
  });

  let book = $state<string | null>(null);
  /** Which testament's books the grid lists — Android's `newTestament`. */
  let newTestament = $state(false);

  const open = $derived(s.bookNavFor !== null);
  /** The book the pane being navigated is already showing, so the grid can say
   *  "you are here" (Android BookNav.kt: `currentBook`). */
  const currentBook = $derived(
    s.bookNavFor === null ? null : (s.panes[s.bookNavFor]?.book ?? null),
  );
  const divide = $derived(seg?.otNtDivide ?? 39);

  $effect(() => {
    if (!open) return;
    book = null;
    // Open on the testament the reader is standing in, so the book they came
    // from — and its "you are here" tile — is on screen without a tap. Read
    // untracked: this decides the tab at OPEN time and then leaves it alone,
    // because after that the tab is the reader's choice, not the pane's.
    untrack(() => {
      newTestament = (toc?.books ?? []).findIndex((b: any) => b.id === currentBook) >= divide;
    });
  });

  const chapterCount = $derived(book ? s.chapterCount(book) || 1 : 0);

  function close(): void {
    s.bookNavFor = null;
  }
  function go(chapter: number): void {
    if (!book || s.bookNavFor === null) return;
    s.navigate(s.bookNavFor, book, chapter);
    close();
  }

  const books = $derived.by(() => {
    const all = toc?.books ?? [];
    return newTestament ? all.slice(divide) : all.slice(0, divide);
  });

  /** A book tile's reading-map paint. Android's rule (BookNav.kt: "the gold
   *  'you are here' border always wins"): on the book the reader is in, the
   *  marker takes the fill and the border and only the bloom survives — where
   *  they ARE matters more than where they have been, and the tile still says
   *  how long it has been. */
  function bookStyle(id: string): string {
    const heat = bookHeat.get(id);
    if (id !== currentBook) return tintStyle(heat);
    const shadow = readingTint(heat)?.shadow;
    return shadow ? `box-shadow:${shadow};` : "";
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={t("booknav.title")} use:modal={{ close }}>
    <div class="bar">
      {#if book}
        <button class="crumb" onclick={() => (book = null)}>‹ {s.bookName(book)}</button>
      {:else}
        <span class="crumb-title">{t("booknav.goTo")}</span>
      {/if}
      <span class="spacer"></span>
      {#if !book}
        <!-- One testament at a time (Android BookNav.kt): 39 tiles is a grid you
             can take in, 66 is a scroll. -->
        <div class="testaments" role="group" aria-label={t("booknav.testament")}>
          <button
            class="tab"
            class:on={!newTestament}
            data-testament="ot"
            aria-pressed={!newTestament}
            onclick={() => (newTestament = false)}>{t("booknav.old")}</button
          >
          <button
            class="tab"
            class:on={newTestament}
            data-testament="nt"
            aria-pressed={newTestament}
            onclick={() => (newTestament = true)}>{t("booknav.new")}</button
          >
        </div>
      {/if}
      <button class="close" onclick={close} aria-label={t("common.close")}>✕</button>
    </div>
    <!-- NO COLOUR LEGEND. It was added so the tint would explain itself on a
         phone, where the per-tile `title` never fires — but a row of colour
         words above the grid is chrome in front of the thing the reader opened
         this to do, which is pick a book (Glendon, 2026-08-04). The tiles still
         carry their `title`, and the guide explains the tint in prose. -->
    <div class="content">
      {#if !book}
        <!-- `lang`: `hyphens: auto` above needs to know the language to break a
             word where a reader of it expects. -->
        <div class="grid books" lang={lang()}>
          {#each books as b (b.id)}
            <button
              data-book={b.id}
              class:current={b.id === currentBook}
              aria-current={b.id === currentBook ? "page" : undefined}
              onclick={() => (book = b.id)}
              style={bookStyle(b.id)}
              title={tintTitle(b.name ?? b.id, bookHeat.get(b.id))}
            >{b.name ?? b.id}</button>
          {/each}
        </div>
      {:else}
        <!-- The word came through as an English literal, on a German screen too,
             and check-i18n never saw it because the line is mostly `{…}`. -->
        <p class="sect">{s.bookName(book)} — {t("booknav.chapter")}</p>
        <div class="grid nums">
          {#each Array.from({ length: chapterCount }, (_, i) => i + 1) as c (c)}
            <button
              onclick={() => go(c)}
              style={tintStyle(chapterHeat.get(c))}
              title={tintTitle(`${s.bookName(book)} ${c}`, chapterHeat.get(c))}
            >{c}</button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 42;
  }
  .dialog {
    position: fixed;
    z-index: 43;
    top: 7vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, 96vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 14px 56px rgba(0, 0, 0, 0.3);
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    padding: 12px 14px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .crumb,
  .crumb-title {
    font-weight: 600;
    font-size: calc(17px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
  }
  .crumb {
    padding: 8px 6px;
  }
  .crumb-title {
    color: var(--ink, #211f1a);
  }
  .spacer {
    flex: 1;
  }
  .testaments {
    display: flex;
    flex: 0 0 auto;
  }
  .tab {
    padding: 8px 8px;
    font-size: calc(14px * var(--uiScale, 1));
    white-space: nowrap;
    color: var(--faded, #8a8276);
  }
  .tab.on {
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .close {
    color: var(--faded, #8a8276);
    font-size: calc(18px * var(--uiScale, 1));
    padding: 8px 12px;
  }
  .content {
    overflow-y: auto;
    padding: 12px 14px 20px;
  }
  .sect {
    font-size: calc(13.5px * var(--uiScale, 1));
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--section, #a0894a);
    margin: 10px 0 6px;
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .grid {
    display: grid;
    gap: 6px;
  }
  /* Bigger targets throughout (feedback 2026-07-29). This is the grid a reader
     uses to get anywhere in the Bible, on a phone, one-handed; Android's version
     is a full screen of big tiles and this is now sized to match rather than to
     fit the most tiles per screen. */
  .grid.books {
    grid-template-columns: repeat(auto-fill, minmax(116px, 1fr));
  }
  .grid.nums {
    grid-template-columns: repeat(auto-fill, minmax(56px, 1fr));
  }
  .grid button {
    padding: 10px 6px;
    min-height: 52px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    background: var(--paper, #fcf9f4);
    font-size: calc(16px * var(--uiScale, 1));
    /* German book names are long single words — "Apostelgeschichte",
       "Thessalonicher" — and a word with no space in it will not wrap however
       narrow the box gets, so they ran straight out of the sides (UAT,
       2026-08-03). `anywhere` is the only value that breaks a word with no
       break opportunity in it; `hyphens` puts the break somewhere a German
       reader expects when the language is declared, and `lang` on the grid
       below is what lets the browser know which rules to use. */
    line-height: 1.15;
    white-space: normal;
    overflow-wrap: anywhere;
    hyphens: auto;
  }
  .grid button:hover {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
  }
  /* You are here. After :hover so it holds when the pointer is elsewhere in the
     grid, and bold as well as gold so it is not colour alone. */
  .grid button.current {
    border-color: var(--gold, #9e7d38);
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
    font-weight: 600;
  }
</style>
