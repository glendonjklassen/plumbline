<script lang="ts">
  // SEARCH — a destination, not a panel.
  //
  // The magnifying glass used to open a field in the app header whose answers
  // came back in the study panel: a 380px sidebar shared with word study, on a
  // phone a bottom sheet over the text. Searching is not a footnote about the
  // verse you are looking at — it is a place you go, with room for the query,
  // the scope, and results you can read without a sheet covering the Bible.
  //
  // The engine call is the same one the panel made; what is new is the SCOPE,
  // which is why this screen exists rather than a taller sheet.
  import BlockList from "../study/BlockList.svelte";
  import { dispatchLink } from "../study/links";
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  let field = $state<HTMLInputElement | null>(null);
  // The field takes focus on arrival: the reader pressed a magnifying glass,
  // so typing is the only thing they came here to do.
  $effect(() => {
    field?.focus();
  });

  const pane = $derived(s.panes[s.activePane]);
  const books = $derived((s.q("toc")?.books ?? []) as { id: string; name: string }[]);

  // ── the range picker ───────────────────────────────────────────────────────
  //
  // "This book" and "this chapter" answer where a reader already is; a range
  // answers a question they came with — the Sermon on the Mount, the exile,
  // Paul on the law. It is a SPAN rather than a set of ticks because almost
  // every such question is contiguous in canon order, and a span stays one
  // range test in the engine (core::search::SearchScope::Span).
  let rangeOpen = $state(false);
  let fromBook = $state("");
  let fromChapter = $state(1);
  let toBook = $state("");
  let toChapter = $state(1);

  // Opening the picker starts where the READER is, so the common edit is one
  // field rather than four.
  function openRange(): void {
    const here = pane?.book ?? books[0]?.id ?? "";
    if (!fromBook) {
      fromBook = here;
      fromChapter = pane?.chapter ?? 1;
      toBook = here;
      toChapter = s.chapterCount(here) || 1;
    }
    rangeOpen = !rangeOpen;
  }

  const clampChapter = (book: string, ch: number): number =>
    Math.min(Math.max(1, ch), Math.max(1, s.chapterCount(book) || 1));

  function applyRange(): void {
    if (!fromBook || !toBook) return;
    const a = clampChapter(fromBook, fromChapter);
    const z = clampChapter(toBook, toChapter);
    s.searchScope = `span:${fromBook}:${a}:${toBook}:${z}`;
    rangeOpen = false;
  }

  /** The canon's own sections (`reference::CANON_SEGMENTS`, the same rows the
   *  canon strip paints) as ready-made spans — Law, Gospels, Letters. One
   *  source, so a preset can never name a stretch the strip draws differently. */
  const presets = $derived.by(() => {
    const segs = (s.q("canonSegments")?.segments ?? []) as { label: string; first: number; last: number }[];
    return segs
      .map((seg) => {
        const a = books[seg.first];
        const z = books[seg.last];
        if (!a || !z) return null;
        return {
          label: seg.label,
          token: `span:${a.id}:1:${z.id}:${s.chapterCount(z.id) || 1}`,
          range: a.id === z.id ? a.name : `${a.name}–${z.name}`,
        };
      })
      .filter((x): x is { label: string; token: string; range: string } => x !== null);
  });

  /** What a span chip says: "John 3–8", "Matthew–John". Built from the token so
   *  it describes the search that actually ran, not the picker's live fields. */
  function spanLabel(token: string): string {
    const [, fb, fc, tb, tc] = token.split(":");
    const name = (id: string) => books.find((b) => b.id === id)?.name ?? id;
    if (fb === tb) return fc === tc ? `${name(fb)} ${fc}` : `${name(fb)} ${fc}–${tc}`;
    return `${name(fb)} ${fc} – ${name(tb)} ${tc}`;
  }

  /**
   * The chips, resolved against the active pane AT BUILD TIME — the two narrow
   * ones carry a concrete book/chapter, so a result list keeps meaning what it
   * meant when it was drawn even if the pane moves underneath it.
   *
   * `book`/`chapter` are absent when no pane has a chapter open yet (boot), and
   * the chip is then simply not offered rather than being offered broken.
   */
  const chips = $derived.by(() => {
    const out: { token: string; label: string }[] = [{ token: "all", label: t("search.scopeAll") }];
    if (pane?.book) {
      out.push({ token: `book:${pane.book}`, label: s.bookName(pane.book) });
      if (pane.chapter) {
        out.push({
          token: `chapter:${pane.book}:${pane.chapter}`,
          label: `${s.bookName(pane.book)} ${pane.chapter}`,
        });
      }
    }
    out.push({ token: "ot", label: t("search.scopeOT") }, { token: "nt", label: t("search.scopeNT") });
    // The span in force keeps its own chip, so the reader can see what they
    // chose and switch away and back without re-picking it.
    if (s.searchScope.startsWith("span:")) {
      out.push({ token: s.searchScope, label: spanLabel(s.searchScope) });
    }
    return out;
  });

  // A scope the chips no longer offer (the pane moved to another book) still
  // shows as selected — it is what the results below were actually searched
  // with, and dropping the highlight would misreport them.
  const blocks = $derived.by(() => {
    void s.studyEpoch;
    if (!s.searchQuery.trim()) return null;
    return s.q("searchBlocksScoped", s.searchQuery, s.searchScope)?.blocks ?? null;
  });

  function onInput(e: Event): void {
    s.setSearch((e.currentTarget as HTMLInputElement).value);
  }

  function onKey(e: KeyboardEvent): void {
    // Escape empties a field with something in it, and leaves the screen when
    // it is already empty — the two-stage Escape a search field is expected to
    // have. Back/‹ always leaves.
    if (e.key !== "Escape") return;
    e.stopPropagation();
    if (s.searchDraft) s.setSearch("");
    else close();
  }

  function pick(token: string): void {
    s.searchScope = token;
    // Choosing anything — a chip or a preset — settles the question the picker
    // was open to ask, so it closes behind the choice rather than sitting over
    // the results it just changed.
    rangeOpen = false;
  }

  function onLink(uri: string, ev: MouseEvent): void {
    // A result navigates the reader to the verse, which means leaving this
    // screen — but the query and the scope are KEPT, so Back returns to the
    // results rather than to an empty search box.
    void dispatchLink(s, uri, ev);
    s.screen = "read";
  }

  function close(): void {
    s.clearSearch();
    s.screen = "read";
  }
</script>

<div class="screen">
  <ScreenBar title={t("search.title")} onBack={close} onMenu={() => (s.menuOpen = true)} />

  <div class="body">
    <input
      class="field"
      type="search"
      bind:this={field}
      value={s.searchDraft}
      oninput={onInput}
      onkeydown={onKey}
      placeholder={t("search.placeholder")}
      aria-label={t("common.search")}
    />

    <div class="chips" role="group" aria-label={t("search.scopeLabel")}>
      {#each chips as c (c.token)}
        <button class="chip" class:on={s.searchScope === c.token} onclick={() => pick(c.token)}>{c.label}</button>
      {/each}
      <button class="chip range" class:on={rangeOpen} onclick={openRange} aria-expanded={rangeOpen}>
        {t("search.scopeRange")}
      </button>
    </div>

    {#if rangeOpen}
      <!-- The picker is a PANEL, not a dialog: it sits under the chips it
           belongs to, and the query and results stay on screen behind it, so
           narrowing a search you can see is one gesture rather than a trip
           through a modal. -->
      <div class="range-panel">
        <div class="presets">
          {#each presets as p (p.label)}
            <button class="preset" onclick={() => pick(p.token)} title={p.range}>
              <span class="preset-name">{p.label}</span>
              <span class="preset-range">{p.range}</span>
            </button>
          {/each}
        </div>

        <div class="ends">
          <label class="end">
            <span class="end-label">{t("search.rangeFrom")}</span>
            <select bind:value={fromBook} onchange={() => (fromChapter = 1)}>
              {#each books as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
            </select>
            <select bind:value={fromChapter}>
              {#each Array.from({ length: s.chapterCount(fromBook) || 1 }, (_, i) => i + 1) as n (n)}
                <option value={n}>{n}</option>
              {/each}
            </select>
          </label>
          <label class="end">
            <span class="end-label">{t("search.rangeTo")}</span>
            <select bind:value={toBook} onchange={() => (toChapter = s.chapterCount(toBook) || 1)}>
              {#each books as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
            </select>
            <select bind:value={toChapter}>
              {#each Array.from({ length: s.chapterCount(toBook) || 1 }, (_, i) => i + 1) as n (n)}
                <option value={n}>{n}</option>
              {/each}
            </select>
          </label>
          <button class="apply" onclick={applyRange}>{t("search.rangeApply")}</button>
        </div>
      </div>
    {/if}

    <div class="results" data-surface="search results">
      {#if blocks}
        <BlockList {blocks} {onLink} />
      {:else if s.searchQuery.trim()}
        <!-- The engine is answering. The last answer is NOT held on screen
             here (unlike the panel): a full screen of stale hits under a new
             query reads as an answer to it. -->
        <p class="hint">{t("search.searching")}</p>
      {:else}
        <p class="hint">{t("search.hint")}</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--paper, #fcf9f4);
  }
  .body {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    padding: 12px 14px 0;
    /* The reader's column, centred — the same measure the text keeps, so a
       result list on a wide desktop is not one word per line. */
    width: 100%;
    max-width: 720px;
    margin: 0 auto;
  }
  .field {
    width: 100%;
    box-sizing: border-box;
    font-size: calc(17px * var(--uiScale, 1));
    font-family: inherit;
    padding: 10px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    background: var(--popupPaper, #f2eee6);
    color: var(--ink, #211f1a);
  }
  .field:focus {
    outline: 2px solid var(--gold, #9e7d38);
    outline-offset: -1px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 0 8px;
  }
  .chip {
    font-size: calc(13px * var(--uiScale, 1));
    padding: 6px 12px;
    border-radius: 999px;
    border: 1px solid var(--rule, #d8cba8);
    color: var(--faded, #6c665d);
    background: transparent;
  }
  .chip:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 10%, transparent);
  }
  .chip.on {
    color: var(--paper, #fcf9f4);
    background: var(--gold, #9e7d38);
    border-color: var(--gold, #9e7d38);
  }
  .range-panel {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--popupPaper, #f2eee6);
    padding: 10px;
    margin-bottom: 10px;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-bottom: 10px;
    margin-bottom: 10px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .preset {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 6px 10px;
    border-radius: 8px;
    border: 1px solid var(--rule, #d8cba8);
    text-align: start;
  }
  .preset:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .preset-name {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
  }
  .preset-range {
    font-size: calc(11px * var(--uiScale, 1));
    color: var(--faded, #6c665d);
  }
  .ends {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px;
  }
  .end {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .end-label {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #6c665d);
    min-width: 3.2em;
  }
  .end select {
    font-family: inherit;
    font-size: calc(13px * var(--uiScale, 1));
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--rule, #d8cba8);
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
  }
  .apply {
    margin-inline-start: auto;
    padding: 8px 16px;
    border-radius: 8px;
    background: var(--gold, #9e7d38);
    color: var(--paper, #fcf9f4);
    font-size: calc(13px * var(--uiScale, 1));
  }
  .results {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: 24px;
  }
  .hint {
    color: var(--faded, #6c665d);
    font-size: calc(14px * var(--uiScale, 1));
    padding: 8px 2px;
  }
</style>
