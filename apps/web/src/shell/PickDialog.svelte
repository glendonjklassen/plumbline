<script lang="ts">
  // Choose one of a list. Every caller so far is picking among things that
  // already exist (a tag to rename, a tag to merge into), and asking somebody to
  // retype a name they are looking at is how you get a typo that quietly creates
  // a second tag instead of touching the first.
  //
  // Escape and the backdrop are a CANCEL, resolving null — the same contract
  // `askText` and `askConfirm` keep, so a caller can always await one of these
  // and know that "no answer" is representable.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();
  const req = $derived(s.pickReq);

  function finish(v: string | null): void {
    const r = s.pickReq;
    s.pickReq = null;
    r?.resolve(v);
  }
</script>

{#if req}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <!-- Pointerdown, not click, for the reason ConfirmDialog spells out: the tap
       that OPENED this is re-delivered as a synthesized click a moment later and
       would answer a question the reader never saw. -->
  <div class="backdrop" onpointerdown={() => finish(null)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={req.title}
    data-surface="pick"
    use:modal={{ close: () => finish(null) }}
  >
    <h2>{req.title}</h2>
    <div class="list">
      {#each req.options as opt (opt)}
        <button onclick={() => finish(opt)}>{opt}</button>
      {/each}
    </div>
    <div class="actions">
      <button onclick={() => finish(null)}>{t("common.cancel")}</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 44;
  }
  .dialog {
    position: fixed;
    z-index: 45;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(420px, calc(100vw - 32px));
    max-height: min(70vh, 560px);
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 18px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    background: var(--popupPaper, #f2eee6);
  }
  h2 {
    margin: 0;
    font-size: calc(17px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  /* The list scrolls, the dialog does not: a reader with sixty tags still has
     the Cancel button where they left it. */
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .list button {
    text-align: left;
    padding: 10px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    color: var(--ink, #211f1a);
    font-size: calc(15px * var(--uiScale, 1));
  }
  .list button:hover {
    border-color: var(--gold, #9e7d38);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
