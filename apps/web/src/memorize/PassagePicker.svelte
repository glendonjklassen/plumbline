<script lang="ts">
  // Pick the end of a passage to memorize as one chunk (§Memorization).
  //
  // The convention (2026-07-27): the verse you long-pressed is the START, and
  // you tap the LAST verse from a grid of that chapter's remaining verse
  // numbers — the same tap-grid idiom as the passage navigator's chapter grid.
  // It needs no new gesture, reads the same under touch and mouse, and the grid
  // only ever offers verses that exist, which makes the same-chapter limit
  // self-evident rather than an error message.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { nowStamp } from "../engine/StudyEngine";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const start = $derived(s.memorizePassageFrom);
  /** `"Ps 23:1"` → `["Ps 23", 1]`. */
  const parts = $derived.by(() => {
    if (!start) return null;
    const m = /^(.*) (\d+):(\d+)$/.exec(start);
    return m ? { book: m[1], chapter: Number(m[2]), verse: Number(m[3]) } : null;
  });
  const lastVerse = $derived(parts ? (s.q("chapterVerseCount", parts.book, parts.chapter) ?? 0) : 0);
  /** Every verse after the start — the ends a passage could have. */
  const ends = $derived.by(() => {
    if (!parts || !lastVerse) return [];
    return Array.from({ length: Math.max(0, lastVerse - parts.verse) }, (_, i) => parts.verse + i + 1);
  });

  let end = $state<number | null>(null);
  $effect(() => {
    void start;
    end = null;
  });

  const throughRef = $derived(parts && end !== null ? `${parts.book} ${parts.chapter}:${end}` : null);
  const label = $derived(start && end !== null ? `${start}–${end}` : start);
  // The text of the chunk as it will be drilled, so the reader sees what they
  // are taking on before committing to it.
  const preview = $derived.by(() => {
    if (!parts || end === null) return "";
    const bodies: string[] = [];
    for (let v = parts.verse; v <= end; v++) {
      const got = s.q("verse", `${parts.book} ${parts.chapter}:${v}`);
      if (got?.body) bodies.push(got.body);
    }
    return bodies.join(" ");
  });

  function close(): void {
    s.memorizePassageFrom = null;
  }

  function commit(): void {
    if (!start || !throughRef) return;
    // Read the refs BEFORE close(). `start` and `throughRef` derive from
    // `s.memorizePassageFrom`, which close() nulls — and a stale $derived
    // recomputes the moment it is read again, so passing them after closing
    // handed the engine null for both and every attempt came back "null or
    // invalid argument" with no card written (feedback 2026-07-27).
    const from = start;
    const through = throughRef;
    const named = label;
    close();
    void s.author("memoryAddPassage", from, through, nowStamp()).then((err) => {
      s.showToast(err ?? t("menu.memorizing", { passage: named }));
    });
  }
</script>

{#if start}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-label={t("memorize.passageTitle")}
    data-surface="passage picker"
    use:modal={{ close }}
  >
    <div class="bar">
      <span class="title">{t("memorize.passage", { passage: label })}</span>
      <span class="spacer"></span>
      <button class="close" onclick={close} aria-label={t("common.close")}>✕</button>
    </div>
    <div class="body">
      {#if !ends.length}
        <p class="note">
          {start} is the last verse of its chapter — a passage has to end on a later verse of the
          same chapter.
        </p>
      {:else}
        <p class="note">{t("memorize.passageNote")}</p>
        <div class="grid">
          {#each ends as v (v)}
            <button class:picked={end === v} onclick={() => (end = v)}>{v}</button>
          {/each}
        </div>
        {#if preview}
          <p class="preview">{preview}</p>
        {/if}
      {/if}
    </div>
    <div class="foot">
      <button onclick={close}>{t("common.cancel")}</button>
      <button class="primary" disabled={end === null} onclick={commit}>
        Memorize {end === null ? "passage" : label}
      </button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.32);
    z-index: 46;
  }
  .sheet {
    position: fixed;
    z-index: 47;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(520px, calc(100vw - 24px));
    max-height: calc(min(80vh, 640px) - var(--bottomNavH, 0px));
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 11px;
    box-shadow: 0 14px 44px rgba(0, 0, 0, 0.26);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 11px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .title {
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .body {
    padding: 11px;
    overflow-y: auto;
  }
  .note {
    font-size: calc(13.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    margin-bottom: 9px;
  }
  .grid {
    display: grid;
    /* 44 and not 42: the tap floor (app.css) is the button's minimum width, and a
       track narrower than the thing standing in it overflows the cell. */
    grid-template-columns: repeat(auto-fill, minmax(44px, 1fr));
    gap: 6px;
  }
  .grid button {
    padding: 9px 0;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    font-variant-numeric: tabular-nums;
  }
  .grid button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .grid button.picked {
    background: color-mix(in srgb, var(--gold, #9e7d38) 30%, transparent);
    border-color: var(--gold, #9e7d38);
    font-weight: 700;
  }
  .preview {
    margin-top: 11px;
    padding-top: 9px;
    border-top: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 70%, transparent);
    font-size: calc(14px * var(--uiScale, 1));
    line-height: 1.5;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 9px 11px;
    border-top: 1px solid var(--rule, #d8cba8);
  }
  .foot button {
    padding: 7px 13px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
  }
  .foot .primary {
    background: var(--gold, #9e7d38);
    color: #fff;
    border-color: var(--gold, #9e7d38);
    font-weight: 600;
  }
  .foot button:disabled {
    opacity: 0.5;
  }
</style>
