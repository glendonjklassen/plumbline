<script lang="ts">
  // Reading history (the shared config's recents, most-recent-first).
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { historySpans, spanLabel } from "./historySpans";
  import { SEATING_ICONS, SEATING_LABEL_KEY } from "./seatingIcons";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();
  const history = $derived((s.config.history ?? []) as { book: string; chapter: number }[]);
  /** One line per RUN, not per chapter: an evening in Genesis 1–3 is one thing
   *  the reader did, and three lines of it pushed everything else off the sheet. */
  const spans = $derived(historySpans(history));

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
      {#each spans as sp, i (i)}
        <button onclick={() => open(sp.book, sp.open)}>
          <span>{spanLabel(sp, (b) => s.bookName(b))}</span>
          {#if sp.slot && SEATING_ICONS[sp.slot]}
            <!-- WHERE it was read: a named seating's icon, trailing so the
                 passages stay in one column. This is the seating's only
                 surface now that the bookmarks row no longer carries it
                 (maintainer, 2026-09-21: an icon on the history entries is
                 fine). role="img" with the seating's name, so a screen reader
                 hears "Sunday morning" where a sighted reader sees the sun. -->
            <svg
              class="seat"
              viewBox="0 0 24 24"
              role="img"
              aria-label={t(SEATING_LABEL_KEY[sp.slot])}
              data-slot={sp.slot}
            >
              <path d={SEATING_ICONS[sp.slot]} />
            </svg>
          {/if}
        </button>
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
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: start;
    padding: 5px 8px;
    border-radius: 5px;
  }
  .seat {
    flex: none;
    margin-inline-start: auto;
    width: calc(16px * var(--uiScale, 1));
    height: calc(16px * var(--uiScale, 1));
    fill: var(--gold, #9e7d38);
  }
  .list button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
</style>
