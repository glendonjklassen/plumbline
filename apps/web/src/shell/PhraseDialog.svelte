<script lang="ts">
  // The backup phrase: shown once when this device makes it, and again from
  // the Backup surface on request. Five words the reader is asked to write
  // down — the one thing that opens their backups, and the one thing nobody can
  // recover for them. OK resolves true (the backup goes on); Escape, the
  // backdrop or Back resolve false (nothing is made, nothing is remembered).
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();
  const req = $derived(s.phraseReq);

  let copied = $state(false);

  function close(ok: boolean): void {
    const r = s.phraseReq;
    s.phraseReq = null;
    copied = false;
    r?.resolve(ok);
  }

  async function copy(): Promise<void> {
    if (!req) return;
    try {
      await navigator.clipboard.writeText(req.phrase);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // No clipboard here (an insecure context, a browser that refuses): the
      // words are on screen to be written down, which is the point anyway.
      s.showToast(t("settings.copyBlocked"));
    }
  }
</script>

{#if req}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onpointerdown={() => close(false)}></div>
  <!-- No `data-modal-focus`: focus lands on the dialog, which reads the words
       out, and Tab reaches Copy then OK. Stacked with the confirmation (50/51):
       it is asked FROM the Backup surface and must sit above it. -->
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t("settings.phraseTitle")}
    data-surface="phrase"
    use:modal={{ close: () => close(false) }}
  >
    <h2>{t("settings.phraseTitle")}</h2>
    <!-- The words are English whatever the UI language: they are a key, not copy. -->
    <p class="words" lang="en" dir="ltr">{req.phrase}</p>
    <p class="body">{t("settings.phraseBody")}</p>
    <div class="row">
      <button onclick={copy}>{copied ? t("settings.phraseCopied") : t("settings.phraseCopy")}</button>
      <button class="primary" onclick={() => close(true)}>{t("common.ok")}</button>
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
    width: min(440px, 92vw);
    max-height: calc(70vh - var(--bottomNavH, 0px));
    overflow-y: auto;
    padding: 20px 22px 14px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.32);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h2 {
    font-size: calc(15px * var(--uiScale, 1));
    font-weight: 600;
  }
  /* Large, spaced and selectable: these are read aloud to a spouse and copied
     by hand, and a word that looks like another costs the backup. */
  .words {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: calc(20px * var(--uiScale, 1));
    line-height: 1.5;
    letter-spacing: 0.02em;
    padding: 10px 12px;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    user-select: all;
    -webkit-user-select: all;
    overflow-wrap: anywhere;
  }
  .body {
    font-size: calc(13px * var(--uiScale, 1));
    line-height: 1.45;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .row button {
    padding: 5px 14px;
    border-radius: 6px;
    border: 1px solid var(--rule, #d8cba8);
  }
  .row .primary {
    background: var(--gold, #9e7d38);
    color: #fff;
    border-color: var(--gold, #9e7d38);
  }
</style>
