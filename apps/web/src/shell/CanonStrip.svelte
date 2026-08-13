<script lang="ts">
  // The 30px canon strip (manifest §Canon strip): 8 sections from the core's
  // canonSegments view-model, odd sections shaded, OT/NT divide line, one pin
  // per pane (active gold), click → that book ch 1 in the active pane.
  import { getSession } from "../state/session.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let cssW = $state(0);
  // The full strip (sections, labels, pins) has painted at least once. Until
  // then the canvas stays transparent over the strip's own CSS background —
  // the same colour the paint lays down — so boot shows a quiet band that the
  // detail FADES into, instead of a differently-coloured bar that the minimap
  // pops onto a beat after the page (maintainer UAT, 2026-08-12).
  let painted = $state(false);

  const HEIGHT = 30;
  const seg = $derived(s.q("canonSegments")); // {segments:[{label,first,last}], otNtDivide}
  const toc = $derived(s.q("toc"));
  const bookCount: number = $derived(toc?.books?.length ?? 0);
  const orderOf = $derived(new Map<string, number>((toc?.books ?? []).map((b: any, i: number) => [b.id, i])));

  // Where the strip is pointing right now — the active pane's book. It is what
  // the gold pin marks, and it is what the strip reports as its value.
  const activeOrder = $derived(orderOf.get(s.panes[s.activePane]?.book) ?? 0);
  const activeBook = $derived(toc?.books?.[activeOrder]?.name ?? "");

  $effect(() => {
    const ro = new ResizeObserver(() => (cssW = host.clientWidth));
    ro.observe(host);
    return () => ro.disconnect();
  });

  $effect(() => {
    void s.palette;
    void s.panes.map((p) => p.book).join();
    void s.activePane;
    if (!canvas || cssW <= 0) return;
    const dpr = devicePixelRatio || 1;
    canvas.width = Math.round(cssW * dpr);
    canvas.height = HEIGHT * dpr;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const p = s.palette;
    ctx.fillStyle = p.stripBg ?? "#ebe6db";
    ctx.fillRect(0, 0, cssW, HEIGHT);

    const xOf = (bookIdx: number) => (bookIdx / bookCount) * cssW;
    // Odd sections shaded (ink α0.04); labels centred when they fit.
    ctx.textBaseline = "middle";
    ctx.font = '11px "EB Garamond", Georgia, serif';
    if (!seg?.segments) return;
    painted = true;
    seg.segments.forEach((sec: any, i: number) => {
      const x0 = xOf(sec.first);
      const x1 = xOf(sec.last + 1);
      if (i % 2 === 1) {
        ctx.fillStyle = "rgba(0,0,0,0.04)";
        ctx.fillRect(x0, 0, x1 - x0, HEIGHT);
      }
      const label = sec.label as string;
      const w = ctx.measureText(label).width;
      if (w < x1 - x0 - 6) {
        ctx.fillStyle = p.faded ?? "#8a8276";
        ctx.fillText(label, (x0 + x1) / 2 - w / 2, HEIGHT / 2);
      }
    });
    // OT/NT divide.
    ctx.strokeStyle = p.faded ?? "#8a8276";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(xOf(seg.otNtDivide), 0);
    ctx.lineTo(xOf(seg.otNtDivide), HEIGHT);
    ctx.stroke();
    // One pin per pane at (order+0.5)/66·w — active gold, others gray.
    s.panes.forEach((pane, i) => {
      const order = orderOf.get(pane.book);
      if (order === undefined) return;
      const x = ((order + 0.5) / bookCount) * cssW;
      ctx.fillStyle = i === s.activePane ? (p.gold ?? "#9e7d38") : (p.faded ?? "#8a8276");
      ctx.beginPath();
      ctx.arc(x, HEIGHT / 2, 3.2, 0, Math.PI * 2);
      ctx.fill();
    });
  });

  // WHAT THE STRIP SAYS OUT LOUD.
  //
  // Chromium does not compute `aria-valuetext` for a canvas with
  // `role="slider"`: the node comes back with `valuetext: ""` and `value` set
  // to `aria-valuenow`, while the DOM carries `aria-valuetext="Revelation"` at
  // that same instant. So a screen reader driving this strip was told the
  // position was "42".
  //
  // The attributes below stay — they are correct, and they are what other AT
  // reads. This is a second channel beside them: a polite live region carrying
  // the BOOK'S NAME.
  //
  // Set here, in the one function that moves the strip, and not derived from the
  // active pane: a live region fed by the pane would also speak when the reader
  // navigated from BookNav, a link or a search result, announcing the book on top
  // of whatever took them there. It speaks when the strip is what moved.
  // Assigning the same name twice is silence, which is right — nothing moved.
  let spoken = $state("");

  function goTo(idx: number): void {
    const book = toc?.books?.[Math.min(bookCount - 1, Math.max(0, idx))];
    if (!book) return;
    spoken = book.name ?? book.id;
    s.navigate(s.activePane, book.id, 1);
  }

  function onClick(e: MouseEvent): void {
    const rect = canvas.getBoundingClientRect();
    goTo(Math.floor(((e.clientX - rect.left) / cssW) * bookCount));
  }

  // The strip is a position along the canon, so it is driven like one: arrows
  // step a book, Home/End go to the ends. Without this it was a mouse-only
  // control with no keyboard story whatever — 66 books reachable only by aiming
  // at a 30px band.
  function onKeydown(e: KeyboardEvent): void {
    let idx: number;
    switch (e.key) {
      case "ArrowLeft":
        idx = activeOrder - 1;
        break;
      case "ArrowRight":
        idx = activeOrder + 1;
        break;
      case "Home":
        idx = 0;
        break;
      case "End":
        idx = bookCount - 1;
        break;
      default:
        return; // everything else stays the shell's (scroll, chapter, Escape…)
    }
    // The strip has the focus, so the shell's global arrows must not also fire.
    e.preventDefault();
    e.stopPropagation();
    goTo(idx);
  }
</script>

<div class="strip" bind:this={host}>
  <!-- A slider over the canon: one value, 66 stops, and the book it is on read
       out by name rather than as "book 43 of 66". -->
  <canvas
    bind:this={canvas}
    class:painted
    style:height="{HEIGHT}px"
    role="slider"
    tabindex="0"
    aria-label={t("canon.jumpToBook")}
    aria-valuemin="0"
    aria-valuemax={Math.max(0, bookCount - 1)}
    aria-valuenow={activeOrder}
    aria-valuetext={activeBook}
    onclick={onClick}
    onkeydown={onKeydown}
  ></canvas>
  <!-- The book, spoken. See `spoken` above for why this exists beside attributes
       that are already correct. Hidden the same way ReaderPane's text mirror is
       — a clipped 1px box, never `display: none` or `aria-hidden`, either of
       which takes it out of the accessibility tree and silences it. -->
  <span class="announce" role="status" aria-live="polite">{spoken}</span>
</div>

<style>
  .strip {
    height: 30px;
    border-top: 1px solid var(--rule, #d8cba8);
    /* The exact colour the canvas paints (applyTheme publishes the palette as
       CSS variables before the engine exists). The band is RIGHT from the
       first frame; the canvas fades its detail in over it. */
    background: var(--stripBg, #ebe6db);
  }
  canvas {
    display: block;
    width: 100%;
    cursor: pointer;
    /* See `painted` in the script: transparent until the first full paint, so
       boot never shows a half-drawn strip — just the band, then the detail. */
    opacity: 0;
    transition: opacity 200ms ease;
  }
  canvas.painted {
    opacity: 1;
  }
  /* Inset so the ring sits inside the 30px band instead of over the text above. */
  canvas:focus-visible {
    outline: 2px solid var(--gold, #9e7d38);
    outline-offset: -2px;
  }
  /* Hidden to the eye, present to everything else — ReaderPane's `.mirror`
     technique, and for the same reason: `display: none` and `visibility: hidden`
     both drop a live region out of the tree, and a live region that is not in the
     tree announces nothing. `position: fixed` keeps the box out of the 30px band
     rather than adding a row to it. */
  .announce {
    position: fixed;
    top: 0;
    left: 0;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
