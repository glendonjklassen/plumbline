<script lang="ts">
  // The card behind the weekly feature chip (shell/spotlight.ts): the feature's
  // name, a few plain sentences on what it does, where it lives, and two
  // buttons — Open goes there, Done clears the chip until next week. Closing
  // any other way (Escape, the backdrop) leaves the chip where it was: the
  // reader may want it later in the week.
  import { getSession } from "../state/session.svelte";
  import { FEATURE_BY_ID } from "./spotlight";
  import { modal } from "../lib/modal";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();
  const id = $derived(s.spotlight);

  function close(): void {
    s.spotlight = null;
  }
  function done(): void {
    s.spotlightDone();
    close();
  }
  function open(): void {
    const f = id ? FEATURE_BY_ID[id] : null;
    s.spotlightDone();
    close();
    f?.go(s);
  }
</script>

{#if id}
  <!-- pointerdown, not click, for the reason ConfirmDialog gives: a touch tap's
       synthesized click must not land on this backdrop and close what it opened. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" onpointerdown={close}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t(`spotlight.${id}.name`)}
    data-surface="spotlight"
    use:modal={{ close }}
  >
    <p class="eyebrow">{t("spotlight.title")}</p>
    <h2>{t(`spotlight.${id}.name`)}</h2>
    <p class="body">{t(`spotlight.${id}.body`)}</p>
    <p class="where">{t(`spotlight.${id}.where`)}</p>
    <div class="row">
      <button onclick={done}>{t("spotlight.done")}</button>
      <button class="go" onclick={open}>{t("spotlight.open")}</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.4);
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
    padding: 18px 22px 14px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.32);
  }
  .eyebrow {
    margin: 0 0 4px;
    font-size: calc(12px * var(--uiScale, 1));
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--section-header, #a0894a);
  }
  h2 {
    margin: 0 0 8px;
    font-size: calc(18px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .body {
    margin: 0 0 10px;
    font-size: calc(15px * var(--uiScale, 1));
    line-height: 1.5;
    color: var(--ink, #211f1a);
  }
  .where {
    margin: 0 0 16px;
    font-size: calc(13.5px * var(--uiScale, 1));
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
  .row .go {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
    font-weight: 600;
  }
  .row .go:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 10%, transparent);
  }
</style>
