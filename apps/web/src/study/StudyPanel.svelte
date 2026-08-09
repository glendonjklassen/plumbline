<script lang="ts">
  // The study surface: fetches the current PanelView's block list and renders
  // it. Above 700px it is a sidebar beside the text (380px at the reader's
  // scale, capped at 40vw); at or below, a dismissible bottom sheet (the
  // Compose-phone pattern). Re-fetches after any
  // authoring write via studyEpoch (write → reload → re-fetch, never mutate).
  import BlockList from "./BlockList.svelte";
  import { deleteNote, dispatchLink } from "./links";
  import { getSession } from "../state/session.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  // NO EMBEDDED MAP CARD. The machine tier's sections are the symbolic concept
  // engine's, and they are ordinary blocks — they come through `blocks` below
  // like everything else.

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

  // The "load analysis" offer — a LAST RESORT, not a progress bar. The load does
  // not block the one thread that answers taps; sections appear as their data
  // arrives, so a progress bar over a study that is already usable would narrate
  // a problem the reader does not have.
  //
  // What remains is the one case that is a genuine ASK rather than a status: a
  // reader on Data Saver who has not got the pack, where downloading it is their
  // decision to make. `rndDeferred` is false whenever the load is already coming,
  // so this stays invisible for everyone else — otherwise every phone launch
  // would put a one-time-download button in front of someone who had already
  // taken the download.
  const rndOffer = $derived.by(() => {
    const k = s.panel?.kind;
    if (!(k === "wordStudy" || k === "codeStudy" || k === "concordance")) return false;
    if (!(s.gates & 2) || s.rndState === "ready") return false;
    // Only the ask. A load already under way says nothing and shows nothing.
    return s.rndDeferred && s.rndState !== "loading";
  });

  // NO "first look is slow" warning. The engine does not build an index inside a
  // reader's request, so a study answers immediately with what is ready and fills
  // in when `warmReady` lands — there is nothing to warn about. "— loading —"
  // remains for the moment the worker is genuinely still answering.

  // The reader's text-size setting scales the whole study surface too — fixed
  // 380px/13px chrome reads tiny on a 4K display. Everything inside multiplies by
  // --uiScale (1 at the default 18px body), which is published on `:root`
  // (app.css, lib/uiScale.ts) and shared by every surface — this panel does not
  // compute a private copy.
</script>

{#if s.panel}
  <aside class="panel" data-surface="study panel">
    <div class="bar">
      <div class="grip" aria-hidden="true"></div>
      <button class="close" onclick={() => (s.panel = null)} aria-label={t("panel.close")}>✕</button>
    </div>
    <div class="content">
      {#if s.panel.kind === "notesBrowser"}
        <h2 class="nb-title">{t("panel.yourNotes", { n: notes.length })}</h2>
        {#if notes.length === 0}
          <p class="nb-empty">{t("panel.noNotes")}</p>
        {/if}
        {#each notes as n (n.verse)}
          <div class="nb-note">
            <div class="nb-head">
              <button class="nb-ref" onclick={(e) => onLink(goUri(n.verse), e)}>
                {n.display ?? n.verse}
              </button>
              <button class="nb-edit" onclick={(e) => onLink(`editnote:${n.verse}`, e)}>✎ {t("panel.edit")}</button>
              <!-- Delete without opening: emptying the editor also deletes (and
                   also asks), but a row you can only remove by editing it is an
                   affordance gap — see manifest §Ask before destroying anything. -->
              <button
                class="nb-del"
                onclick={() => void deleteNote(s, n.verse, n.display ?? n.verse)}
                aria-label={t("notes.deleteVerb")}
                title={t("notes.deleteVerb")}>✕</button>
            </div>
            <p class="nb-text">{n.text}</p>
          </div>
        {/each}
      {:else}
        {#if rndOffer}
          <div class="rnd-offer">
            <span class="rnd-note">{t("panel.rndOffer")}</span>
            <button class="rnd-load" onclick={() => void s.ensureRnd()}>{t("panel.rndLoad")}</button>
          </div>
        {/if}
        {#if akjvWord}
          <p class="akjv">
            <span class="akjv-now">{akjvWord.akjv}</span>
            <span class="akjv-was">{t("panel.akjvWas", { word: akjvWord.kjv })}</span>
          </p>
        {/if}
        <!-- `blocks?.length`, not `blocks`: an empty array is truthy in JS, so a
             panel the engine answered with nothing rendered as a blank white
             sheet with no heading, no message and no sign anything had happened.
             Every producer in `panel.rs` emits at least a heading, so `[]` means
             the answer has not arrived (or a producer regressed) — either way
             "— loading —" is the honest thing to show. -->
        {#if blocks?.length}
          <BlockList {blocks} {onLink} />
          <!-- Which build is this? You cannot answer that from a screenshot, and
               "have you relaunched yet" is a terrible way to debug. The release
               tag identifies the code,
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
          <p class="loading" aria-live="polite">{t("panel.loading")}</p>
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
    /* 380px at the reader's text scale, but never more than 40% of the window.
       The cap is what makes the sidebar usable on a FOLDABLE: unfolded, a Pixel
       Fold is ~840 CSS px, where an unscaled 380 already takes 45% and a reader
       at uiScale 1.4 would get a 532px panel with 300px left for scripture. The
       Bible is the point; the panel is the annotation. */
    width: min(calc(380px * var(--uiScale, 1)), 40vw);
    min-width: min(calc(380px * var(--uiScale, 1)), 40vw);
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
  .nb-del {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    margin-left: auto;
  }
  .nb-del:hover {
    color: var(--tierResearch, #b04a3a);
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
  /* Phones: bottom sheet (Compose-phone pattern).

     THE BREAKPOINT IS 700, matching `s.narrow` and the destination bar. Above it
     the shell behaves like a desktop (top bar, up to three reading panes), so the
     study surface must sit beside the text rather than cover the reader as a
     sheet — an unfolded Pixel Fold browser is ~840 px and needs the side-by-side
     scripture and study the Android app gives it on that hardware. Android
     decides by fold posture, the web by width, and 700 is the only width the
     rest of this shell agrees on. */
  @media (max-width: 700px) {
    .panel {
      position: fixed;
      left: 0;
      right: 0;
      /* Stop ABOVE the destination bar, never under it. `--bottomNavH` is
         measured and published by Shell (0 at desktop widths, where there is no
         bar), so this never restates a height that would drift. Getting it wrong
         hides the bottom of the sheet — which for a picker is the "New …" field
         and its Add button, i.e. the whole point of opening it. */
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
