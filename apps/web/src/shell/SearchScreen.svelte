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
    </div>

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
