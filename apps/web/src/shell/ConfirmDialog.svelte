<script lang="ts">
  // The one confirmation. Every destructive action goes through `s.askConfirm`
  // and lands here, so whether something asks before destroying it is a property
  // of the action rather than of whoever wrote its button.
  //
  // Before this, deleting a memorize card asked nothing, rejecting a suggested
  // weave asked nothing, untagging asked nothing, and deleting a thread had its
  // own inline prompt built by hand (2026-07-29). Four different answers to one
  // question.
  //
  // It sits above the destination bar like every other surface, and Escape is a
  // "no" — a confirmation the reader cannot back out of is not a confirmation.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();
  const req = $derived(s.confirmReq);

  function no(): void {
    s.cancelConfirm();
  }
  function yes(): void {
    const r = s.confirmReq;
    s.confirmReq = null;
    r?.resolve(true);
  }
</script>

{#if req}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={no}></div>
  <!-- Escape comes from `use:modal` rather than a `svelte:window` listener, so
       it answers from inside a field too, and — since a confirmation is asked
       FROM another surface — it closes this and not the surface underneath.
       No `data-modal-focus`: focus lands on the dialog, which reads the question
       out, and the first Tab reaches Cancel. Handing a keyboard the destructive
       button is not a default worth having. -->
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={req.title}
    data-surface="confirm"
    use:modal={{ close: no }}
  >
    <h2>{req.title}</h2>
    <p>{req.body}</p>
    <div class="row">
      <button onclick={no}>{t("common.cancel")}</button>
      <button class="danger" onclick={yes}>{req.verb}</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.4);
    /* Above every other surface: it is asked FROM them. */
    z-index: 50;
  }
  .dialog {
    position: fixed;
    z-index: 51;
    top: 22vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(400px, 92vw);
    max-height: calc(70vh - var(--bottomNavH, 0px));
    overflow-y: auto;
    padding: 20px 22px 14px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.32);
  }
  h2 {
    margin: 0 0 8px;
    font-size: calc(18px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  p {
    margin: 0 0 16px;
    font-size: calc(15px * var(--uiScale, 1));
    line-height: 1.5;
    color: var(--faded, #8a8276);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .row button {
    padding: 10px 16px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    font-size: calc(15px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .row .danger {
    border-color: var(--tierResearch, #b04a3a);
    color: var(--tierResearch, #b04a3a);
    font-weight: 600;
  }
  .row .danger:hover {
    background: color-mix(in srgb, var(--tierResearch, #b04a3a) 10%, transparent);
  }
</style>
