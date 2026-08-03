<script lang="ts">
  // Tag picker sheet (Android TagPickerSheet parity): existing tags first, with
  // freetext "New tag…" secondary. Every tag is a topic now that highlight tones
  // are gone, so plain alphabetical is the whole ordering.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { nowStamp } from "../engine/StudyEngine";
  import { refDisplay } from "../reader/refname";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  /** The verse being tagged, named the way a reader says it ("1 Corinthians
   *  13:4"). `s.tagPickFor` stays the refKey — that is what `tagAdd` writes into
   *  the tag file, and the on-disk form is frozen. */
  const shown = $derived(s.tagPickFor ? refDisplay(s, s.tagPickFor) : "");

  const tags = $derived.by(() => {
    void s.studyEpoch;
    if (!s.tagPickFor) return [];
    const all: any[] = s.q("tags")?.tags ?? [];
    return [...all].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  });

  let newName = $state("");

  function close(): void {
    s.tagPickFor = null;
    newName = "";
  }
  function pick(name: string): void {
    const ref = s.tagPickFor!;
    // Both read before close() nulls `s.tagPickFor`: `shown` derives from it, and
    // a stale $derived recomputes on the next read (PassagePicker.commit).
    const said = shown;
    void s.author("tagAdd", name, "verse", ref, null, nowStamp()).then((err) =>
      s.showToast(err ?? t("tag.tagged", { passage: said, tag: name })),
    );
    close();
  }
</script>

{#if s.tagPickFor}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-label={t("tag.title")}
    data-surface="tag picker"
    use:modal={{ close }}
  >
    <h2>{t("tag.heading", { passage: shown })}</h2>
    <div class="list">
      <!-- `tg`, not `t` — see ThreadPicker: an each-block `t` shadows the
           catalogue lookup and `t` is `any`, so it fails only at runtime. -->
      {#each tags as tg (tg.name)}
        <button class="tag" onclick={() => pick(tg.name)}>
          {tg.name}
          <span class="count">{tg.members?.length ?? 0}</span>
        </button>
      {:else}
        <p class="empty">{t("tag.empty")}</p>
      {/each}
    </div>
    <form
      class="new"
      onsubmit={(e) => {
        e.preventDefault();
        if (newName.trim()) pick(newName.trim());
      }}
    >
      <input placeholder={t("tag.new")} bind:value={newName} />
      <button type="submit" disabled={!newName.trim()}>{t("tag.add")}</button>
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
      border-radius: 14px 14px 0 0;
    }
  }
  h2 {
    font-size: calc(15px * var(--uiScale, 1));
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
  .count {
    margin-left: auto;
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .empty {
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
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
