<script lang="ts">
  // Keyboard shortcuts overlay (?/F1) — shell-native, mirroring the web
  // bindings (manifest §Keyboard + wheel).
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  /** BOTH COLUMNS come from the catalogue, keys included. It is tempting to
   *  treat a key combination as language-neutral and leave it a literal, but
   *  "Space", "wheel", "Click a word" and "long-press" are English words that
   *  happen to sit next to a symbol — a German reader needs "Leertaste" and
   *  "Mausrad" as much as they need the row's description. */
  const rows = [
    "scrollLine",
    "scrollPage",
    "ends",
    "chapter",
    "history",
    "zoom",
    "together",
    "word",
    "verse",
    "otherPane",
    "escape",
    "help",
  ];
</script>

{#if s.showShortcuts}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.showShortcuts = false)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t("shell.shortcuts")}
    use:modal={{ close: () => (s.showShortcuts = false) }}
  >
    <h2>{t("shell.shortcuts")}</h2>
    <table>
      <tbody>
        {#each rows as id (id)}
          <tr><td class="keys">{t(`shortcut.${id}.keys`)}</td><td>{t(`shortcut.${id}`)}</td></tr>
        {/each}
      </tbody>
    </table>
    <button class="close" onclick={() => (s.showShortcuts = false)}>{t("common.close")}</button>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 40;
  }
  .dialog {
    position: fixed;
    z-index: 41;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(520px, 94vw);
    max-height: 76vh;
    overflow-y: auto;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    padding: 20px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
  }
  h2 {
    font-size: calc(17px * var(--uiScale, 1));
    font-weight: 600;
    margin-bottom: 12px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: calc(14px * var(--uiScale, 1));
  }
  td {
    padding: 4px 6px;
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 55%, transparent);
  }
  .keys {
    white-space: nowrap;
    color: var(--gold, #9e7d38);
    font-weight: 600;
    padding-inline-end: 14px;
  }
  .close {
    margin-top: 12px;
    padding: 5px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
  }
</style>
