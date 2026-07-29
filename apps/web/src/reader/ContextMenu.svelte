<script lang="ts">
  // Verse context menu (Tier-0 #1): copy shapes · Note… · highlight tones ·
  // Remove highlight · (Full) Tag… / Add to thread… · Memorize. Opened by
  // right-click or long-press; the target verse rides in session state.
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";
  import { dispatchLink } from "../study/links";

  const s = getSession();
  const tones = $derived(s.tones);

  const menu = $derived(s.contextMenu);

  function close(): void {
    s.contextMenu = null;
  }

  async function copy(kind: string): Promise<void> {
    const ref = menu!.refKey;
    close();
    const text = await s.rpc.call("copyText", ref, kind);
    if (text) {
      await navigator.clipboard.writeText(text);
      s.showToast("Copied");
    }
  }

  function note(): void {
    const ref = menu!.refKey;
    close();
    void dispatchLink(s, `editnote:${ref}`);
  }

  function highlight(tone: { name: string; hex: string }): void {
    const ref = menu!.refKey;
    close();
    // Whole-verse wash: membership in the tone's colour tag (created coloured).
    const capName = tone.name[0].toUpperCase() + tone.name.slice(1);
    void s.author("tagAdd", capName, "verse", ref, null, nowStamp()).then((err) => {
      if (err) {
        s.showToast(err);
        return;
      }
      void s.author("tagSetColor", capName, tone.hex);
      s.lastTone = { name: capName, hex: tone.hex };
    });
  }

  async function removeHighlight(): Promise<void> {
    const ref = menu!.refKey;
    close();
    // Clear any word-precise range covering the verse, then drop membership
    // in every colour tag holding it (errors from non-membership are noise).
    void s.author("highlightClearVerse", ref);
    for (const t of (await s.fetchQ("tags"))?.tags ?? [])
      if (t.color) void s.author("tagRemove", t.name, "verse", ref);
  }

  /** The chapter of the verse under the menu, when it is that chapter's FIRST
   *  verse — the only place "Mark chapter read…" is offered. Kept to verse 1 on
   *  purpose: findable when wanted, too fiddly to do across a whole Bible, which
   *  is exactly the balance the feature asks for. */
  const markable = $derived.by(() => {
    const ref = menu?.refKey;
    if (!ref) return null;
    const m = /^(.+) (\d+):(\d+)$/.exec(ref);
    if (!m || m[3] !== "1") return null;
    return { book: m[1], chapter: Number(m[2]) };
  });

  function markRead(): void {
    const t = markable;
    close();
    if (t) s.markReadFor = t;
  }

  function tagPick(): void {
    const ref = menu!.refKey;
    close();
    s.tagPickFor = ref;
  }

  function addThread(): void {
    const ref = menu!.refKey;
    close();
    void dispatchLink(s, `addthread:${ref}`);
  }

  function memorize(): void {
    const ref = menu!.refKey;
    close();
    void s.author("memoryAdd", ref, nowStamp()).then((err) => s.showToast(err ?? `Memorizing ${ref}`));
  }

  /** A whole section as one card — this verse starts it, the picker ends it. */
  function memorizePassage(): void {
    const ref = menu!.refKey;
    close();
    s.memorizePassageFrom = ref;
  }

  // Clamp the menu into the viewport.
  let el: HTMLDivElement | undefined = $state();
  const pos = $derived.by(() => {
    if (!menu) return { x: 0, y: 0 };
    const w = el?.offsetWidth ?? 230;
    const h = el?.offsetHeight ?? 320;
    return {
      x: Math.min(menu.x, innerWidth - w - 8),
      y: Math.min(menu.y, innerHeight - h - 8),
    };
  });
</script>

{#if menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close} oncontextmenu={(e) => (e.preventDefault(), close())}></div>
  <div class="menu" bind:this={el} style:left="{pos.x}px" style:top="{pos.y}px">
    <div class="ref">{menu.refKey}</div>
    <button onclick={() => copy("verse")}>Copy verse</button>
    <button onclick={() => copy("verseRef")}>Copy with reference</button>
    <button onclick={() => copy("verseMarkdown")}>Copy as markdown</button>
    <button onclick={() => copy("chapter")}>Copy chapter</button>
    <hr />
    <button onclick={note}>Note…</button>
    <div class="tones">
      {#each tones as t (t.name)}
        <button
          class="swatch"
          style:background={t.hex}
          title="Highlight — {t.name}"
          aria-label="Highlight {t.name}"
          onclick={() => highlight(t)}
        ></button>
      {/each}
    </div>
    <button onclick={removeHighlight}>Remove highlight</button>
    <hr />
    <button onclick={tagPick}>Tag…</button>
    <button onclick={addThread}>Add to thread…</button>
    <hr />
    <button onclick={memorize}>Memorize this verse</button>
    <button onclick={memorizePassage}>Memorize passage…</button>
    {#if markable}
      <hr />
      <button onclick={markRead}>Mark chapter read…</button>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 44;
  }
  .menu {
    position: fixed;
    z-index: 45;
    min-width: 210px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 9px;
    box-shadow: 0 10px 36px rgba(0, 0, 0, 0.22);
    padding: 6px;
    display: flex;
    flex-direction: column;
  }
  .ref {
    font-size: 12px;
    color: var(--faded, #8a8276);
    padding: 4px 8px 6px;
    font-weight: 600;
  }
  .menu > button {
    text-align: left;
    padding: 5px 9px;
    border-radius: 5px;
    font-size: 14.5px;
  }
  .menu > button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  hr {
    border: none;
    border-top: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 70%, transparent);
    margin: 4px 6px;
  }
  .tones {
    display: flex;
    gap: 6px;
    padding: 5px 9px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.15);
  }
  .swatch:hover {
    transform: scale(1.12);
  }
</style>
