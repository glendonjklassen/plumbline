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
  import { nowStamp } from "../engine/StudyEngine";

  const s = getSession();

  const threads = $derived.by(() => {
    void s.studyEpoch;
    if (!s.threadPickFor) return [];
    const all: any[] = s.q("threads")?.threads ?? [];
    return [...all].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  });

  let newName = $state("");
  let confirming = $state<string | null>(null);

  function close(): void {
    s.threadPickFor = null;
    newName = "";
    confirming = null;
  }

  function pick(name: string): void {
    const ref = s.threadPickFor!;
    void s.author("threadAdd", name, ref, null, nowStamp()).then((err) =>
      s.showToast(err ?? `Added ${ref} to ${name}`),
    );
    close();
  }

  function remove(name: string): void {
    confirming = null;
    void s.author("threadRemove", name).then((err) => s.showToast(err ?? `Deleted ${name}`));
  }
</script>

{#if s.threadPickFor}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div class="sheet" role="dialog" aria-modal="true" data-surface="thread picker">
    <h2>Add {s.threadPickFor} to a thread</h2>
    <div class="list">
      {#each threads as t (t.name)}
        <div class="row">
          <button class="thread" onclick={() => pick(t.name)}>
            {t.name}
            <span class="count">{t.entries?.length ?? 0}</span>
          </button>
          <button class="del" title="Delete this thread" onclick={() => (confirming = t.name)}>✕</button>
        </div>
      {:else}
        <p class="empty">No threads yet — name your first below.</p>
      {/each}
    </div>
    {#if confirming}
      <p class="confirm">
        Delete <b>{confirming}</b>? The thread and every passage on it go — the verses themselves
        are untouched.
        <button class="danger" onclick={() => remove(confirming!)}>Delete</button>
        <button onclick={() => (confirming = null)}>Cancel</button>
      </p>
    {/if}
    <form
      class="new"
      onsubmit={(e) => {
        e.preventDefault();
        if (newName.trim()) pick(newName.trim());
      }}
    >
      <input placeholder="New thread…" bind:value={newName} />
      <button type="submit" disabled={!newName.trim()}>Add</button>
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
    font-size: 16px;
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
    font-size: 15px;
    border-radius: 6px;
  }
  .thread:hover {
    background: var(--paper, #fcf9f4);
  }
  .count {
    margin-left: auto;
    font-size: 12px;
    color: var(--faded, #8a8276);
  }
  .del {
    padding: 8px 10px;
    color: var(--faded, #8a8276);
    border-radius: 6px;
    font-size: 13px;
  }
  .del:hover {
    color: var(--tierResearch, #b04a3a);
  }
  .empty {
    font-size: 14px;
    color: var(--faded, #8a8276);
    margin: 8px 4px;
  }
  .confirm {
    font-size: 13px;
    line-height: 1.5;
    color: var(--ink, #211f1a);
    margin: 10px 0 0;
  }
  .confirm button {
    margin-left: 6px;
    padding: 4px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    font-size: 13px;
    color: var(--faded, #8a8276);
  }
  .confirm .danger {
    color: var(--tierResearch, #b04a3a);
    border-color: var(--tierResearch, #b04a3a);
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
    font-size: 14px;
  }
  .new button {
    padding: 8px 14px;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
    font-size: 14px;
  }
  .new button:disabled {
    opacity: 0.4;
    border-color: var(--rule, #d8cba8);
    color: var(--faded, #8a8276);
  }
</style>
