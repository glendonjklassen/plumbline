<script lang="ts">
  // The study surface: fetches the current PanelView's block list and renders
  // it. Wide screens show it as the fixed 380px sidebar; narrow screens as a
  // dismissible bottom sheet (Compose-phone pattern). Re-fetches after any
  // authoring write via studyEpoch (write → reload → re-fetch, never mutate).
  import BlockList from "./BlockList.svelte";
  import { dispatchLink } from "./links";
  import { getSession } from "../state/session.svelte";
  import { aboutBlocks, guideBlocks } from "../engine/StudyEngine";

  const s = getSession();

  // Notes browser (Explore ▸ Notes parity): every personal note, tap → verse,
  // edit in place. Built shell-side from user_notes_json (no block producer).
  const notes = $derived.by(() => {
    void s.studyEpoch;
    return s.panel?.kind === "notesBrowser" ? ((s.engine.userNotes()?.notes ?? []) as any[]) : [];
  });

  const blocks = $derived.by(() => {
    void s.studyEpoch; // any authoring write invalidates panel content
    const p = s.panel;
    if (!p) return null;
    const e = s.engine;
    switch (p.kind) {
      case "wordStudy":
        return e.wordStudyBlocks(p.refKey, p.tokenIndex, s.gates)?.blocks;
      case "codeStudy":
        return e.codeStudyBlocks(p.code, p.word, s.gates)?.blocks;
      case "concordance":
        return e.concordanceBlocks(p.code)?.blocks;
      case "renderingConcordance":
        return e.renderingConcordanceBlocks(p.code, p.rendering)?.blocks;
      case "threads":
        return e.threadsBlocks()?.blocks;
      case "thread":
        return e.threadBlocks(p.index)?.blocks;
      case "tags":
        return e.tagsBlocks()?.blocks;
      case "tag":
        return e.tagBlocks(p.index)?.blocks;
      case "weaves":
        return e.weavesBlocks()?.blocks;
      case "suggested":
        return e.suggestedBlocks()?.blocks;
      case "compare":
        return e.compareBlocks(p.index, true)?.blocks;
      case "search":
        return e.searchBlocks(s.searchQuery)?.blocks;
      case "guide":
        return guideBlocks(s.wasm)?.blocks;
      case "about":
        return aboutBlocks(s.wasm)?.blocks;
      default:
        return null;
    }
  });

  function onLink(uri: string, ev: MouseEvent): void {
    void dispatchLink(s, uri, ev);
  }
</script>

{#if s.panel && (blocks || s.panel.kind === "notesBrowser")}
  <aside class="panel">
    <div class="bar">
      <div class="grip" aria-hidden="true"></div>
      <button class="close" onclick={() => (s.panel = null)} aria-label="Close panel">✕</button>
    </div>
    <div class="content">
      {#if s.panel.kind === "notesBrowser"}
        <h2 class="nb-title">Your notes ({notes.length})</h2>
        {#if notes.length === 0}
          <p class="nb-empty">No notes yet — right-click or long-press a verse and choose Note…</p>
        {/if}
        {#each notes as n (n.verse)}
          <div class="nb-note">
            <div class="nb-head">
              <button class="nb-ref" onclick={(e) => onLink(`go:${n.verse.replace(" ", ":")}`, e)}>
                {n.display ?? n.verse}
              </button>
              <button class="nb-edit" onclick={(e) => onLink(`editnote:${n.verse}`, e)}>✎ edit</button>
            </div>
            <p class="nb-text">{n.text}</p>
          </div>
        {/each}
      {:else}
        <BlockList {blocks} {onLink} />
      {/if}
    </div>
  </aside>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border-left: 1px solid var(--rule, #d8cba8);
    width: 380px;
    min-width: 380px;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 4px 8px 0;
  }
  .grip {
    display: none;
  }
  .close {
    color: var(--faded, #8a8276);
    font-size: 15px;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .close:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 16px 24px;
  }
  .nb-title {
    font-size: 17px;
    font-weight: 600;
    margin: 4px 0 8px;
  }
  .nb-empty {
    color: var(--faded, #8a8276);
    font-size: 13.5px;
  }
  .nb-note {
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 55%, transparent);
    padding: 6px 0;
  }
  .nb-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .nb-ref {
    color: var(--gold, #9e7d38);
    font-weight: 600;
  }
  .nb-ref:hover {
    text-decoration: underline;
  }
  .nb-edit {
    font-size: 12px;
    color: var(--faded, #8a8276);
  }
  .nb-text {
    font-size: 14.5px;
    margin-top: 2px;
    white-space: pre-wrap;
  }

  /* Narrow screens: bottom sheet (Compose-phone pattern). */
  @media (max-width: 900px) {
    .panel {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      top: auto;
      width: auto;
      min-width: 0;
      max-height: 88dvh;
      height: 62dvh;
      z-index: 20;
      border-left: none;
      border-top: 1px solid var(--rule, #d8cba8);
      border-radius: 14px 14px 0 0;
      box-shadow: 0 -8px 32px rgba(0, 0, 0, 0.18);
    }
    .bar {
      justify-content: space-between;
    }
    .grip {
      display: block;
      width: 42px;
      height: 4px;
      border-radius: 2px;
      background: var(--rule, #d8cba8);
      margin: 4px auto 0;
    }
  }
</style>
