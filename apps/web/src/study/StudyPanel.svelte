<script lang="ts">
  // The study surface: fetches the current PanelView's block list and renders
  // it. Wide screens show it as the fixed 380px sidebar; narrow screens as a
  // dismissible bottom sheet (Compose-phone pattern). Re-fetches after any
  // authoring write via studyEpoch (write → reload → re-fetch, never mutate).
  import BlockList from "./BlockList.svelte";
  import EmbedMaps from "./EmbedMaps.svelte";
  import { dispatchLink } from "./links";
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  // The Strong's code behind the current study — drives the embedded map
  // cards (Android StudyMaps parity; machine-tier, so gate-checked).
  const studyCode = $derived.by(() => {
    const p = s.panel;
    if (!p || !(s.gates & 2)) return null;
    if (p.kind === "codeStudy" || p.kind === "concordance") return p.code;
    if (p.kind === "wordStudy") {
      const tok = s.q("token", p.refKey, p.tokenIndex);
      return (tok?.strongs?.[0] as string | undefined) ?? null;
    }
    return null;
  });

  // Explore: the study tools as described cards so they aren't cryptic
  // (Android ExploreScreen parity).
  const exploreCards = [
    { label: "Notes", desc: "Every note you've written, tap to revisit.", go: () => (s.panel = { kind: "notesBrowser" }) },
    { label: "Threads", desc: "Reading paths you assemble verse by verse.", go: () => (s.panel = { kind: "threads" }) },
    { label: "Tags", desc: "Your topics — accumulate now, weave later.", go: () => (s.panel = { kind: "tags" }) },
    { label: "Weaves", desc: "Passages linked as one thread through the canon.", go: () => (s.panel = { kind: "weaves" }) },
    { label: "Suggested", desc: "Proposed weaves awaiting your review.", go: () => (s.panel = { kind: "suggested" }) },
    { label: "Constellation", desc: "Every weave as lanes of stars across the canon.", go: () => (s.mapPopup = { kind: "constellation" }) },
    { label: "Weave map", desc: "Book-to-book ribbons of every link.", go: () => (s.mapPopup = { kind: "chord" }) },
  ];

  // Notes browser (Explore ▸ Notes parity): every personal note, tap → verse,
  // edit in place. Built shell-side from user_notes_json (no block producer).
  const notes = $derived.by(() => {
    void s.studyEpoch;
    return s.panel?.kind === "notesBrowser" ? ((s.q("userNotes")?.notes ?? []) as any[]) : [];
  });

  const blocks = $derived.by(() => {
    void s.studyEpoch; // any authoring write invalidates panel content
    const p = s.panel;
    if (!p) return null;
    switch (p.kind) {
      case "wordStudy":
        return s.q("wordStudyBlocks", p.refKey, p.tokenIndex, s.gates)?.blocks;
      case "codeStudy":
        return s.q("codeStudyBlocks", p.code, p.word, s.gates)?.blocks;
      case "concordance":
        return s.q("concordanceBlocks", p.code)?.blocks;
      case "renderingConcordance":
        return s.q("renderingConcordanceBlocks", p.code, p.rendering)?.blocks;
      case "threads":
        return s.q("threadsBlocks")?.blocks;
      case "thread":
        return s.q("threadBlocks", p.index)?.blocks;
      case "tags":
        return s.q("tagsBlocks")?.blocks;
      case "tag":
        return s.q("tagBlocks", p.index)?.blocks;
      case "weaves":
        return s.q("weavesBlocks")?.blocks;
      case "suggested":
        return s.q("suggestedBlocks")?.blocks;
      case "compare":
        return s.q("compareBlocks", p.index, true)?.blocks;
      case "search":
        return s.q("searchBlocks", s.searchQuery)?.blocks;
      case "guide":
        return s.qs("guideBlocks")?.blocks;
      case "about":
        return s.qs("aboutBlocks")?.blocks;
      default:
        return null;
    }
  });

  function onLink(uri: string, ev: MouseEvent): void {
    void dispatchLink(s, uri, ev);
  }

  // The "load analysis" offer (phones defer the machine-tier auto-download):
  // shown on the studies that gain machine sections, while the tier is on
  // but the pack isn't in yet. The loading state shows for desktops too —
  // their auto-download announces itself the same way.
  const rndOffer = $derived.by(() => {
    const k = s.panel?.kind;
    if (!(k === "wordStudy" || k === "codeStudy" || k === "concordance")) return false;
    if (!(s.gates & 2) || s.rndState === "ready") return false;
    return s.rndState === "loading" || s.rndDeferred;
  });

  // A cold read is SLOW and a warm one is instant: the first definition of a
  // session builds the occurrence index, the first analytical answer sweeps the
  // corpus. A bare "— loading —" flashing for seconds reads as a hang
  // (feedback 2026-07-27), so once a read outlasts a frame or two, say why and
  // promise the rest are fast. Timed rather than flagged: whatever is cold, the
  // wait itself is the honest signal.
  const SLOW_READ_MS = 600;
  let slowRead = $state(false);
  $effect(() => {
    // Re-arm per study: `blocks` null means the worker is still answering.
    void s.panel;
    if (blocks) {
      slowRead = false;
      return;
    }
    const t = setTimeout(() => (slowRead = true), SLOW_READ_MS);
    return () => clearTimeout(t);
  });

  // The reader's text-size setting scales the whole study surface too —
  // fixed 380px/13px chrome reads tiny on a 4K display (feedback 2026-07-25).
  // Everything inside multiplies by --uiScale (1 at the default 18px body).
  const uiScale = $derived(Number(s.config.bodySize ?? 18) / 18);
</script>

{#if s.panel}
  <aside class="panel" style:--uiScale={uiScale}>
    <div class="bar">
      <div class="grip" aria-hidden="true"></div>
      <button class="close" onclick={() => (s.panel = null)} aria-label="Close panel">✕</button>
    </div>
    <div class="content">
      {#if s.panel.kind === "explore"}
        <h2 class="nb-title">Explore</h2>
        {#each exploreCards as c (c.label)}
          <button class="ex-card" onclick={c.go}>
            <span class="ex-name">{c.label}</span>
            <span class="ex-desc">{c.desc}</span>
          </button>
        {/each}
      {:else if s.panel.kind === "notesBrowser"}
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
        {#if rndOffer}
          <div class="rnd-offer">
            {#if s.rndState === "loading"}
              <span class="rnd-note">
                {s.rndPreparing
                  ? "Preparing the analysis — this takes a moment on a phone…"
                  : `Downloading the analysis pack — ${Math.round(s.rndProgress * 100)}%`}
              </span>
              <div class="rnd-bar">
                <div class="rnd-fill" class:indeterminate={s.rndPreparing} style:width={`${s.rndProgress * 100}%`}></div>
              </div>
            {:else}
              <span class="rnd-note">Similar concepts, verses-like-this, and concept maps are a one-time ~4 MB download.</span>
              <button class="rnd-load" onclick={() => void s.ensureRnd()}>Load analysis</button>
            {/if}
          </div>
        {/if}
        {#if studyCode}
          <EmbedMaps code={studyCode} />
        {/if}
        {#if blocks}
          <BlockList {blocks} {onLink} />
        {:else}
          <!-- Never look frozen: the worker is answering. -->
          <p class="loading" aria-live="polite">— loading —</p>
          {#if slowRead}
            <p class="firstslow">
              The first one takes a few seconds while the analysis is built for this text. Every
              look after this is instant.
            </p>
          {/if}
        {/if}
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
    width: calc(380px * var(--uiScale, 1));
    min-width: calc(380px * var(--uiScale, 1));
    font-size: calc(16px * var(--uiScale, 1));
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
    font-size: calc(15px * var(--uiScale, 1));
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
  .loading {
    color: var(--faded, #8a8276);
    text-align: center;
    padding: 22px 0;
    font-size: calc(13.5px * var(--uiScale, 1));
    animation: loadpulse 1.1s ease-in-out infinite;
  }
  /* Deliberately NOT pulsing — an explanation should sit still and be read. */
  .firstslow {
    color: var(--faded, #8a8276);
    text-align: center;
    padding: 0 10px 18px;
    margin-top: -14px;
    line-height: 1.45;
    font-size: calc(12.5px * var(--uiScale, 1));
  }
  @keyframes loadpulse {
    0%,
    100% {
      opacity: 0.35;
    }
    50% {
      opacity: 0.9;
    }
  }
  .nb-title {
    font-size: calc(17px * var(--uiScale, 1));
    font-weight: 600;
    margin: 4px 0 8px;
  }
  .nb-empty {
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
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
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .nb-text {
    font-size: calc(14.5px * var(--uiScale, 1));
    margin-top: 2px;
    white-space: pre-wrap;
  }
  .ex-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    text-align: left;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 9px;
    background: var(--paper, #fcf9f4);
    padding: 10px 12px;
    margin: 4px 0;
  }
  .ex-card:hover {
    border-color: var(--gold, #9e7d38);
  }
  .ex-name {
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .ex-desc {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .rnd-offer {
    display: flex;
    flex-direction: column;
    gap: 6px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 9px;
    background: var(--paper, #fcf9f4);
    padding: 10px 12px;
    margin: 6px 0;
  }
  .rnd-note {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .rnd-load {
    align-self: flex-start;
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--gold, #9e7d38);
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    padding: 3px 12px;
  }
  .rnd-load:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .rnd-bar {
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--gold, #9e7d38) 18%, transparent);
    overflow: hidden;
  }
  .rnd-fill {
    height: 100%;
    background: var(--gold, #9e7d38);
    transition: width 0.2s ease;
  }
  .rnd-fill.indeterminate {
    animation: rndpulse 1.2s ease-in-out infinite;
  }
  @keyframes rndpulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
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
