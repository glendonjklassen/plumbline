<script lang="ts">
  // Thread picker sheet (Android ThreadPickerSheet parity): the threads you have
  // are a list you tap; freetext is only for a genuinely new one.
  //
  // It used to be a bare `askText` prompt (2026-07-28 feedback: "a nightmare").
  // A freetext-only prompt makes the common case — adding a fifth passage to the
  // thread you have been building all week — require retyping its name exactly,
  // and a typo silently forks a second thread instead of failing.
  //
  // Deleting lives here for the same reason: a thread started by typo had no way
  // out at all before.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { nowStamp } from "../engine/StudyEngine";
  import { refDisplay } from "../reader/refname";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  /** The verse being filed, named the way a reader says it ("1 Peter 5:7").
   *  `s.threadPickFor` stays the refKey — that is what `threadAdd` writes into
   *  the thread file, and the on-disk form is frozen. */
  const shown = $derived(s.threadPickFor ? refDisplay(s, s.threadPickFor) : "");

  const threads = $derived.by(() => {
    void s.studyEpoch;
    if (!s.threadPickFor) return [];
    const all: any[] = s.q("threads")?.threads ?? [];
    return [...all].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  });

  let newName = $state("");

  function close(): void {
    s.threadPickFor = null;
    newName = "";
  }

  function pick(name: string): void {
    const ref = s.threadPickFor!;
    // Both read before close() nulls `s.threadPickFor`: `shown` derives from it,
    // and a stale $derived recomputes on the next read (PassagePicker.commit).
    const said = shown;
    void s.author("threadAdd", name, ref, null, nowStamp()).then((err) =>
      s.showToast(err ?? t("thread.added", { passage: said, thread: name })),
    );
    close();
  }

  async function remove(name: string): Promise<void> {
    // The shared confirmation (s.askConfirm), not an inline one of its own: every
    // destructive action in the app asks the same way now.
    const ok = await s.askConfirm(
      t("thread.deleteAsk", { thread: name }),
      t("thread.deleteBody"),
      t("thread.deleteVerb"),
    );
    if (!ok) return;
    const err = await s.author("threadRemove", name);
    s.showToast(err ?? t("thread.deleted", { thread: name }));
  }
</script>

{#if s.threadPickFor}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-label={t("thread.title")}
    data-surface="thread picker"
    use:modal={{ close }}
  >
    <h2>{t("thread.heading", { passage: shown })}</h2>
    <div class="list">
      {#each threads as t (t.name)}
        <div class="row">
          <button class="thread" onclick={() => pick(t.name)}>
            {t.name}
            <span class="count">{t.entries?.length ?? 0}</span>
          </button>
          <button class="del" title={t("thread.delete")} onclick={() => void remove(t.name)}>✕</button>
        </div>
      {:else}
        <p class="empty">{t("thread.empty")}</p>
      {/each}
    </div>
    <form
      class="new"
      onsubmit={(e) => {
        e.preventDefault();
        if (newName.trim()) pick(newName.trim());
      }}
    >
      <input placeholder={t("thread.new")} bind:value={newName} />
      <button type="submit" disabled={!newName.trim()}>{t("thread.add")}</button>
    </form>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 44;
  }
  .sheet {
    position: fixed;
    z-index: 45;
    left: 50%;
    top: 24vh;
    transform: translateX(-50%);
    width: min(380px, 92vw);
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    padding: 14px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
  }
  @media (max-width: 900px) {
    .sheet {
      top: auto;
      /* Stop ABOVE the destination bar, never under it. `--bottomNavH` is
         measured and published by Shell (0 at desktop widths, where there is no
         bar), so this never restates a height that would drift. Getting it wrong
         hides the bottom of the sheet — which for a picker is the "New …" field
         and its Add button, i.e. the whole point of opening it (2026-07-29). */
      bottom: var(--bottomNavH, 0px);
      left: 0;
      transform: none;
      width: 100%;
      max-height: 72vh;
      border-radius: 12px 12px 0 0;
    }
  }
  h2 {
    font-size: calc(16px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
    margin: 0 0 8px;
  }
  .list {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-bottom: 1px solid var(--rule, #d8cba8);
    padding-bottom: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .thread {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 8px;
    text-align: left;
    color: var(--ink, #211f1a);
    font-size: calc(15px * var(--uiScale, 1));
    border-radius: 6px;
  }
  .thread:hover {
    background: var(--paper, #fcf9f4);
  }
  .count {
    margin-left: auto;
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .del {
    padding: 8px 10px;
    color: var(--faded, #8a8276);
    border-radius: 6px;
    font-size: calc(13px * var(--uiScale, 1));
  }
  .del:hover {
    color: var(--tierResearch, #b04a3a);
  }
  .empty {
    font-size: calc(14px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    margin: 8px 4px;
  }
  .new {
    display: flex;
    gap: 6px;
    padding-top: 10px;
  }
  .new input {
    flex: 1;
    padding: 8px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .new button {
    padding: 8px 14px;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .new button:disabled {
    opacity: 0.4;
    border-color: var(--rule, #d8cba8);
    color: var(--faded, #8a8276);
  }
</style>
