<script lang="ts">
  // Tag→weave conversion (the makeweave: verb): a topic tag accumulates
  // passages over time; this chains the checked members through the canon.
  // Re-running after the tag grows just adds the new edges.
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";

  const s = getSession();

  const tag = $derived.by(() => {
    void s.studyEpoch;
    return s.tagWeaveFor === null ? null : (s.q("tags")?.tags?.[s.tagWeaveFor] ?? null);
  });
  const members = $derived(
    (tag?.members ?? []).filter((m: any) => m.kind === "verse" && m.verse) as any[],
  );

  let checked = $state<Set<string>>(new Set());
  let name = $state("");
  $effect(() => {
    if (tag) {
      checked = new Set(members.map((m) => m.verse));
      name = tag.name;
    }
  });

  function close(): void {
    s.tagWeaveFor = null;
  }
  function toggle(ref: string): void {
    const next = new Set(checked);
    if (next.has(ref)) next.delete(ref);
    else next.add(ref);
    checked = next;
  }
  function create(): void {
    if (!tag) return;
    const refsJson = checked.size === members.length ? null : JSON.stringify([...checked]);
    const weaveName = name.trim() !== tag.name ? name.trim() : null;
    void s.author("weaveFromTag", tag.name, refsJson, weaveName, nowStamp()).then((err) =>
      s.showToast(err ?? `Weave “${name.trim()}” — ${checked.size} passages chained`),
    );
    close();
  }
</script>

{#if tag}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div class="sheet" role="dialog" aria-modal="true">
    <h2>Make a weave — {tag.name}</h2>
    <p class="hint">
      Chains the checked passages through the canon. Run it again after the tag grows to add the
      new links.
    </p>
    <div class="list">
      {#each members as m (m.verse)}
        <label class="member">
          <input type="checkbox" checked={checked.has(m.verse)} onchange={() => toggle(m.verse)} />
          <span>{m.display ?? m.verse}</span>
        </label>
      {/each}
    </div>
    <input class="name" bind:value={name} placeholder="Weave name" aria-label="Weave name" />
    <div class="actions">
      <span class="count">{checked.size} of {members.length} passages</span>
      <button onclick={close}>Cancel</button>
      <button class="primary" disabled={checked.size < 2 || !name.trim()} onclick={create}>
        Create weave
      </button>
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
  .sheet {
    position: fixed;
    z-index: 45;
    left: 50%;
    top: 18vh;
    transform: translateX(-50%);
    width: min(420px, 92vw);
    max-height: 66vh;
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    padding: 16px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    gap: 8px;
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
  }
  .hint {
    font-size: 12.5px;
    color: var(--faded, #8a8276);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .member {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 2px;
    cursor: pointer;
  }
  .member:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 10%, transparent);
    border-radius: 5px;
  }
  .name {
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 8px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .count {
    flex: 1;
    font-size: 12.5px;
    color: var(--faded, #8a8276);
  }
  .actions button {
    padding: 5px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
  }
  .actions .primary {
    background: var(--gold, #9e7d38);
    color: #fff;
    border-color: var(--gold, #9e7d38);
  }
  .actions .primary:disabled {
    opacity: 0.45;
  }
</style>
