<script lang="ts">
  // Text prompt modal — the web stand-in for the desktops' native prompts
  // (tag/thread names, note editing). Empty submission is meaningful (clears
  // a note), so OK always resolves with the current value; Esc/Cancel → null.
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  let value = $state("");
  let inputEl: HTMLInputElement | HTMLTextAreaElement | undefined = $state();

  $effect(() => {
    if (s.promptReq) {
      value = s.promptReq.initial;
      setTimeout(() => inputEl?.focus(), 30);
    }
  });

  function finish(v: string | null): void {
    s.promptReq?.resolve(v);
    s.promptReq = null;
  }
  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") finish(null);
    else if (e.key === "Enter" && (!s.promptReq?.multiline || e.ctrlKey)) finish(value);
  }
</script>

{#if s.promptReq}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => finish(null)}></div>
  <div class="dialog" role="dialog" aria-modal="true" tabindex="-1" onkeydown={onKeydown}>
    <h2>{s.promptReq.title}</h2>
    {#if s.promptReq.multiline}
      <textarea bind:this={inputEl} bind:value rows="5"></textarea>
      <p class="hint">Ctrl+Enter to save · empty text clears</p>
    {:else}
      <input bind:this={inputEl} bind:value />
    {/if}
    <div class="actions">
      <button onclick={() => finish(null)}>Cancel</button>
      <button class="primary" onclick={() => finish(value)}>OK</button>
    </div>
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
    top: 24vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, 92vw);
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    padding: 16px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 {
    font-size: 15px;
    font-weight: 600;
  }
  input,
  textarea {
    width: 100%;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 8px;
    resize: vertical;
  }
  .hint {
    font-size: 11.5px;
    color: var(--faded, #8a8276);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .actions button {
    padding: 5px 14px;
    border-radius: 6px;
    border: 1px solid var(--rule, #d8cba8);
  }
  .actions .primary {
    background: var(--gold, #9e7d38);
    color: #fff;
    border-color: var(--gold, #9e7d38);
  }
</style>
