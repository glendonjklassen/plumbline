<script lang="ts">
  // Reading history (the shared config's recents, most-recent-first).
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();
  const history = $derived((s.config.history ?? []) as { book: string; chapter: number }[]);

  function open(book: string, chapter: number): void {
    s.showHistory = false;
    s.navigate(s.activePane, book, chapter);
  }
</script>

{#if s.showHistory}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.showHistory = false)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t("history.title")}
    data-surface="history"
    use:modal={{ close: () => (s.showHistory = false) }}
  >
    <h2>{t("history.title")}</h2>
    {#if history.length === 0}
      <p class="empty">{t("history.empty")}</p>
    {/if}
    <div class="list">
      {#each history as h, i (i)}
        <button onclick={() => open(h.book, h.chapter)}>{s.bookName(h.book)} {h.chapter}</button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 38;
  }
  .dialog {
    position: fixed;
    z-index: 39;
    top: 20vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(320px, 90vw);
    max-height: calc(60vh - var(--bottomNavH, 0px));
    overflow-y: auto;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    padding: 14px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
  }
  h2 {
    font-size: calc(15px * var(--uiScale, 1));
    font-weight: 600;
    margin-bottom: 8px;
  }
  .empty {
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .list {
    display: flex;
    flex-direction: column;
  }
  .list button {
    text-align: left;
    padding: 5px 8px;
    border-radius: 5px;
  }
  .list button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
</style>
