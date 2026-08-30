<script lang="ts">
  // The per-block painter (manifest P0.1): walks the typed block list from the
  // core producer and renders Section / Para / Rule. It derives nothing — the
  // producer owns tier order, caps, colors (as palette roles), and link URIs.
  //
  // The one interaction it owns is DRAG-REORDER: a para the producer marked
  // with `drag` ("{thread}:{entry}" on a thread entry's header row) gets a grip,
  // and dropping one row on another reports the pair to `onDrag`. What the drop
  // MEANS still belongs to the dispatcher — this file never parses the ids.
  interface Run {
    text: string;
    size: number;
    color: string;
    bold: boolean;
    italic: boolean;
    uri?: string;
    /** Pinned to the row's end — action icons across from a header or stat. */
    end?: boolean;
  }
  interface Block {
    kind: "section" | "para" | "rule";
    title?: string;
    markGlyph?: string;
    markColor?: string;
    runs?: Run[];
    indent?: boolean;
    topGap?: boolean;
    drag?: string;
  }
  interface Props {
    blocks: Block[];
    onLink: (uri: string, ev: MouseEvent) => void;
    /** A drag row was dropped on another: (dragged id, target id). */
    onDrag?: (from: string, to: string) => void;
  }
  let { blocks, onLink, onDrag }: Props = $props();

  const color = (role: string | undefined) => `var(--${role ?? "ink"}, var(--ink, #211f1a))`;

  // ── drag state ──────────────────────────────────────────────────────────────
  // Pointer-based, not HTML5 DnD: a grip with `touch-action: none` drags on a
  // phone as well as under a mouse, and the ↑/↓ links stay as the assistive
  // path. The move/up listeners live on the DOCUMENT for the drag's lifetime —
  // pointer capture on the grip was tried and lost the pointer mid-gesture (the
  // row re-renders under it) — and the row under the pointer is found by hit
  // test (elementFromPoint), so nothing here keeps a rect table that a
  // re-render would stale.
  let dragging = $state<string | null>(null);
  let dragOver = $state<string | null>(null);

  function rowUnder(ev: PointerEvent): string | null {
    const el = document.elementFromPoint(ev.clientX, ev.clientY);
    const id = el?.closest("[data-drag]")?.getAttribute("data-drag") ?? null;
    return id !== dragging ? id : null;
  }
  function gripDown(id: string, ev: PointerEvent): void {
    if (!onDrag) return;
    ev.preventDefault();
    dragging = id;
    dragOver = null;
    const move = (e: PointerEvent): void => {
      dragOver = rowUnder(e);
    };
    const finish = (e: PointerEvent, drop: boolean): void => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      document.removeEventListener("pointercancel", cancel);
      const from = dragging;
      const to = drop ? rowUnder(e) : null;
      dragging = null;
      dragOver = null;
      if (drop && from !== null && to !== null && to !== from) onDrag?.(from, to);
    };
    const up = (e: PointerEvent): void => finish(e, true);
    const cancel = (e: PointerEvent): void => finish(e, false);
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
    document.addEventListener("pointercancel", cancel);
  }
</script>

<div class="blocks">
  {#each blocks as b, i (i)}
    {#if b.kind === "rule"}
      <hr />
    {:else if b.kind === "section"}
      <h3>
        <span class="title">{b.title}</span>
        {#if b.markGlyph}
          <span class="mark" style:color={color(b.markColor)}>{b.markGlyph}</span>
        {/if}
      </h3>
    {:else if b.kind === "para"}
      <p
        class:indent={b.indent}
        class:gap={b.topGap}
        class:has-trail={(b.runs ?? []).some((r) => r.end)}
        class:drag-row={b.drag !== undefined && !!onDrag}
        class:dragging={b.drag !== undefined && dragging === b.drag}
        class:drag-over={b.drag !== undefined && dragOver === b.drag}
        data-drag={b.drag}
      >
        {#if b.drag !== undefined && onDrag}
          <span
            class="drag-grip"
            role="presentation"
            aria-hidden="true"
            onpointerdown={(e) => gripDown(b.drag!, e)}>⠿</span
          >
        {/if}
        {#each b.runs ?? [] as r, j (j)}
          {#if r.uri}
            <button
              class="link"
              class:trail-start={r.end && !(b.runs ?? [])[j - 1]?.end}
              style:font-size="calc({r.size}px * var(--uiScale, 1))"
              style:color={color(r.color)}
              style:font-weight={r.bold ? 600 : 400}
              style:font-style={r.italic ? "italic" : "normal"}
              onclick={(e) => onLink(r.uri!, e)}>{r.text}</button
            >
          {:else}
            <span
              class:trail-start={r.end && !(b.runs ?? [])[j - 1]?.end}
              style:font-size="calc({r.size}px * var(--uiScale, 1))"
              style:color={color(r.color)}
              style:font-weight={r.bold ? 600 : 400}
              style:font-style={r.italic ? "italic" : "normal"}>{r.text}</span
            >
          {/if}
        {/each}
      </p>
    {/if}
  {/each}
</div>

<style>
  .blocks {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-wrap: break-word;
  }
  hr {
    border: none;
    border-top: 1px solid var(--rule, #d8cba8);
    margin: 10px 0 6px;
  }
  h3 {
    font-size: calc(12px * var(--uiScale, 1));
    font-weight: 600;
    letter-spacing: 0.09em;
    color: var(--section, #a0894a);
    margin: 12px 0 2px;
  }
  h3 .title {
    font-variant: small-caps;
    text-transform: lowercase;
  }
  h3 .mark {
    margin-inline-start: 6px;
    font-size: calc(12px * var(--uiScale, 1));
  }
  p {
    margin: 2px 0;
    line-height: 1.45;
  }
  /* A row with end-pinned runs lays out as a flex line: the leading runs keep
     their order, the first `end` run takes the auto margin, and the trailing
     group spaces itself with the row gap. Only such rows become flex — an
     ordinary paragraph must keep INLINE flow, or multi-run verse text would
     break at every style boundary. `margin-inline-start` keeps it right in
     RTL. */
  p.has-trail {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 0.6em;
  }
  p.has-trail .trail-start {
    margin-inline-start: auto;
  }
  p.indent {
    padding-inline-start: 14px;
  }
  p.gap {
    margin-top: 10px;
  }
  .link {
    display: inline;
    text-align: start;
    padding: 0;
    text-decoration: none;
    cursor: pointer;
    /* The one control the 44px tap floor in app.css is wrong for, and
       `display: inline` does not save it: a `<button>` is blockified to
       inline-block whatever display says, so the floor reaches it and a 44px
       word in the middle of a sentence sets the whole paragraph's line height
       to 44px. A cross-reference inside running study prose is aimed at as a
       WORD — the sentence around it is the target's context, and there is no
       neighbouring control to mis-hit. */
    min-height: 0;
    min-width: 0;
  }
  .link:hover {
    text-decoration: underline;
  }
  .drag-grip {
    display: inline-block;
    margin-inline-end: 6px;
    color: var(--faded, #8a8276);
    font-size: calc(12px * var(--uiScale, 1));
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .drag-row.dragging {
    opacity: 0.45;
  }
  .drag-row.drag-over {
    box-shadow: 0 -2px 0 0 var(--gold, #9e7d38);
  }
</style>
