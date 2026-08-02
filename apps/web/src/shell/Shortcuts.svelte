<script lang="ts">
  // Keyboard shortcuts overlay (?/F1) — shell-native, mirroring the web
  // bindings (manifest §Keyboard + wheel).
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";

  const s = getSession();

  const rows: [string, string][] = [
    ["↑ / ↓", "Scroll one line"],
    ["PageUp / PageDown · Space", "Scroll a page (Shift: all panes)"],
    ["Home / End", "Top / bottom of chapter"],
    ["→ or ] · ← or [", "Next / previous chapter"],
    ["Alt+← / Alt+→", "Back / forward in this pane"],
    ["Ctrl+wheel · Ctrl+± · Ctrl+0", "Zoom text / reset"],
    ["Shift+wheel", "Scroll all panes together"],
    ["Click a word", "Word study"],
    ["Right-click / long-press a verse", "Copy · note · tag · thread"],
    ["Shift+click a verse link", "Open in the other pane"],
    ["Esc", "Close panel / popup"],
    ["? / F1", "This overlay"],
  ];
</script>

{#if s.showShortcuts}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.showShortcuts = false)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Keyboard shortcuts"
    use:modal={{ close: () => (s.showShortcuts = false) }}
  >
    <h2>Keyboard shortcuts</h2>
    <table>
      <tbody>
        {#each rows as [keys, what] (keys)}
          <tr><td class="keys">{keys}</td><td>{what}</td></tr>
        {/each}
      </tbody>
    </table>
    <button class="close" onclick={() => (s.showShortcuts = false)}>Close</button>
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
    font-size: 17px;
    font-weight: 600;
    margin-bottom: 12px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }
  td {
    padding: 4px 6px;
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 55%, transparent);
  }
  .keys {
    white-space: nowrap;
    color: var(--gold, #9e7d38);
    font-weight: 600;
    padding-right: 14px;
  }
  .close {
    margin-top: 12px;
    padding: 5px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
  }
</style>
