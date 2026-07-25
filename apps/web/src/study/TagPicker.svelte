<script lang="ts">
  // Tag picker sheet (Android TagPickerSheet parity): existing tags first —
  // plain before coloured tone tags — with freetext "New tag…" secondary.
  // Tags stay colourless unless explicitly coloured elsewhere.
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";

  const s = getSession();

  const tags = $derived.by(() => {
    void s.studyEpoch;
    if (!s.tagPickFor) return [];
    const all: any[] = s.engine.tags()?.tags ?? [];
    return [...all.filter((t) => !t.color), ...all.filter((t) => t.color)];
  });

  let newName = $state("");

  function close(): void {
    s.tagPickFor = null;
    newName = "";
  }
  function pick(name: string): void {
    const ref = s.tagPickFor!;
    const err = s.engine.tagAdd(name, "verse", ref, null, nowStamp());
    s.showToast(err ?? `Tagged ${ref} — ${name}`);
    close();
  }
</script>

{#if s.tagPickFor}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div class="sheet" role="dialog" aria-modal="true">
    <h2>Tag {s.tagPickFor}</h2>
    <div class="list">
      {#each tags as t (t.name)}
        <button class="tag" onclick={() => pick(t.name)}>
          {#if t.color}<span class="dot" style:background={t.color}></span>{/if}
          {t.name}
          <span class="count">{t.members?.length ?? 0}</span>
        </button>
      {:else}
        <p class="empty">No tags yet.</p>
      {/each}
    </div>
    <form
      class="new"
      onsubmit={(e) => {
        e.preventDefault();
        if (newName.trim()) pick(newName.trim());
      }}
    >
      <input placeholder="New tag…" bind:value={newName} />
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
      bottom: 0;
      left: 0;
      transform: none;
      width: 100%;
      border-radius: 14px 14px 0 0;
    }
  }
  h2 {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 10px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tag {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 6px 8px;
    border-radius: 6px;
  }
  .tag:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.12);
  }
  .count {
    margin-left: auto;
    font-size: 12px;
    color: var(--faded, #8a8276);
  }
  .empty {
    color: var(--faded, #8a8276);
    font-size: 13.5px;
    padding: 6px 8px;
  }
  .new {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .new input {
    flex: 1;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 5px 8px;
  }
  .new button {
    padding: 5px 12px;
    border: 1px solid var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
    border-radius: 6px;
  }
  .new button:disabled {
    opacity: 0.4;
  }
</style>
