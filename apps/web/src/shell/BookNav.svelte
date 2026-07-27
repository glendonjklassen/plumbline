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
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  const toc = $derived(s.q("toc"));
  const seg = $derived(s.q("canonSegments"));

  let book = $state<string | null>(null);

  const open = $derived(s.bookNavFor !== null);
  $effect(() => {
    if (open) book = null;
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

  const otBooks = $derived((toc?.books ?? []).slice(0, seg?.otNtDivide ?? 39));
  const ntBooks = $derived((toc?.books ?? []).slice(seg?.otNtDivide ?? 39));
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="bar">
      {#if book}
        <button class="crumb" onclick={() => (book = null)}>‹ {s.bookName(book)}</button>
      {:else}
        <span class="crumb-title">Go to…</span>
      {/if}
      <span class="spacer"></span>
      <button class="close" onclick={close} aria-label="Close">✕</button>
    </div>
    <div class="content">
      {#if !book}
        <p class="sect">Old Testament</p>
        <div class="grid books">
          {#each otBooks as b (b.id)}
            <button onclick={() => (book = b.id)}>{b.name ?? b.id}</button>
          {/each}
        </div>
        <p class="sect">New Testament</p>
        <div class="grid books">
          {#each ntBooks as b (b.id)}
            <button onclick={() => (book = b.id)}>{b.name ?? b.id}</button>
          {/each}
        </div>
      {:else}
        <p class="sect">{s.bookName(book)} — chapter</p>
        <div class="grid nums">
          {#each Array.from({ length: chapterCount }, (_, i) => i + 1) as c (c)}
            <button onclick={() => go(c)}>{c}</button>
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
    padding: 10px 14px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .crumb,
  .crumb-title {
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .crumb-title {
    color: var(--ink, #211f1a);
  }
  .spacer {
    flex: 1;
  }
  .close {
    color: var(--faded, #8a8276);
  }
  .content {
    overflow-y: auto;
    padding: 12px 14px 20px;
  }
  .sect {
    font-size: 12px;
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
  .grid.books {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }
  .grid.nums {
    grid-template-columns: repeat(auto-fill, minmax(44px, 1fr));
  }
  .grid button {
    padding: 8px 4px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    background: var(--paper, #fcf9f4);
    font-size: 14px;
  }
  .grid button:hover {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
  }
</style>
