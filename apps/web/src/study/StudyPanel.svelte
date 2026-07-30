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

  // What the overlay does to the tapped word. Read only while the overlay is
  // ON: with it off the reader is looking at the KJV and there is nothing to
  // explain. Above the Strong's, because the codes are keyed to the KJV word —
  // the original has to be read before the lexicon detail, not after it.
  const akjvWord = $derived.by(() => {
    const p = s.panel;
    if (!s.config.akjvOverlay || p?.kind !== "wordStudy") return null;
    return s.q("akjvToken", p.refKey, p.tokenIndex);
  });

  function onLink(uri: string, ev: MouseEvent): void {
    void dispatchLink(s, uri, ev);
  }

  /** refKey → the core's `go:` verb, split on the LAST space, as core `go_uri`
   *  does. Also in App.svelte and MemorizeHost — see App.svelte for why. */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;

  // The "load analysis" offer — a LAST RESORT, and no longer a progress bar.
  //
  // The bar and its percentage are gone (feedback 2026-07-28). They existed to
  // explain a wait the reader was made to sit through: the analysis load used to
  // block the one thread that answers taps, so the panel had to account for
  // itself. It does not block anything now — sections appear as their data
  // arrives — and a progress bar over a study that is already usable narrates a
  // problem the reader does not have.
  //
  // What remains is the one case that is a genuine ASK rather than a status: a
  // reader on Data Saver who has not got the pack, where downloading it is their
  // decision to make. `rndDeferred` is false whenever the load is already coming,
  // so this stays invisible for everyone else — before that, every phone launch
  // put a "one-time ~4 MB download" button in front of someone who had already
  // taken the download (feedback 2026-07-27).
  const rndOffer = $derived.by(() => {
    const k = s.panel?.kind;
    if (!(k === "wordStudy" || k === "codeStudy" || k === "concordance")) return false;
    if (!(s.gates & 2) || s.rndState === "ready") return false;
    // Only the ask. A load already under way says nothing and shows nothing.
    return s.rndDeferred && s.rndState !== "loading";
  });

  // GONE: "The first one takes a few seconds… Every look after this is instant."
  //
  // It was an apology for a bug, and it was not even accurate. "Every look after
  // this" meant every look until the tab closed — the next launch rebuilt the
  // same indexes and said it again, which is what made a reader who had used the
  // app for days keep being told it was their first time (feedback 2026-07-28).
  //
  // The wait it was apologising for is gone too: the engine no longer builds an
  // index inside a reader's request, so a study answers immediately with what is
  // ready and fills in when `warmReady` lands. There is nothing left to warn
  // about, and a message that explains a wait the reader is not having is worse
  // than silence. "— loading —" remains for the moment the worker is genuinely
  // still answering.

  // The reader's text-size setting scales the whole study surface too —
  // fixed 380px/13px chrome reads tiny on a 4K display (feedback 2026-07-25).
  // Everything inside multiplies by --uiScale (1 at the default 18px body).
  const uiScale = $derived(Number(s.config.bodySize ?? 18) / 18);
</script>

{#if s.panel}
  <aside class="panel" style:--uiScale={uiScale} data-surface="study panel">
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
              <button class="nb-ref" onclick={(e) => onLink(goUri(n.verse), e)}>
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
            <span class="rnd-note">Similar concepts, verses-like-this, and concept maps are a one-time ~4 MB download.</span>
            <button class="rnd-load" onclick={() => void s.ensureRnd()}>Load analysis</button>
          </div>
        {/if}
        {#if studyCode}
          <EmbedMaps code={studyCode} />
        {/if}
        {#if akjvWord}
          <p class="akjv">
            <span class="akjv-now">{akjvWord.akjv}</span>
            <span class="akjv-was">KJV: {akjvWord.kjv}</span>
          </p>
        {/if}
        {#if blocks}
          <BlockList {blocks} {onLink} />
          <!-- Which build is this? Neither of us could answer that from a
               screenshot (feedback 2026-07-27), and "have you relaunched yet"
               is a terrible way to debug. The release tag identifies the code,
               the pack version identifies the DATA (it moves independently),
               and the build id separates two deploys of the same tag. -->
          {#if s.panel.kind === "about"}
            <p class="version">
              Plumbline <strong>{__APP_VERSION__}</strong><br />
              <span class="vsub">engine {s.engineVersion} · data {s.packVersion.slice(0, 8)} · build {__BUILD_ID__}</span>
            </p>
          {/if}
        {:else}
          <!-- Never look frozen: the worker is answering. -->
          <p class="loading" aria-live="polite">— loading —</p>
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
.version {
    margin-top: 18px;
    padding-top: 12px;
    border-top: 1px solid var(--rule, #d8cba8);
    font-size: calc(13px * var(--uiScale));
    color: var(--faded, #8a8276);
    /* Read off a screen and typed into a bug report — let it be selected. */
    user-select: text;
  }
  .version strong {
    color: var(--ink, #211f1a);
    font-weight: 600;
  }
  .vsub {
    font-size: calc(11.5px * var(--uiScale));
    font-variant-numeric: tabular-nums;
  }
  /* The overlay's answer, directly under the headword and above the Strong's. */
  .akjv {
    margin-bottom: 10px;
    padding-bottom: 8px;
    border-bottom: 1px dotted var(--gold, #9e7d38);
    font-size: calc(15px * var(--uiScale));
  }
  .akjv-now {
    font-weight: 600;
  }
  .akjv-was {
    margin-left: 8px;
    color: var(--faded, #8a8276);
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
  /* Narrow screens: bottom sheet (Compose-phone pattern). */
  @media (max-width: 900px) {
    .panel {
      position: fixed;
      left: 0;
      right: 0;
      /* Stop ABOVE the destination bar, never under it. `--bottomNavH` is
         measured and published by Shell (0 at desktop widths, where there is no
         bar), so this never restates a height that would drift. Getting it wrong
         hides the bottom of the sheet — which for a picker is the "New …" field
         and its Add button, i.e. the whole point of opening it (2026-07-29). */
      bottom: var(--bottomNavH, 0px);
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
