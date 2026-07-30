<script lang="ts">
  // The 30px canon strip (manifest §Canon strip): 8 sections from the core's
  // canonSegments view-model, odd sections shaded, OT/NT divide line, one pin
  // per pane (active gold), click → that book ch 1 in the active pane.
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let cssW = $state(0);

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

  function goTo(idx: number): void {
    const book = toc?.books?.[Math.min(bookCount - 1, Math.max(0, idx))];
    if (book) s.navigate(s.activePane, book.id, 1);
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
    style:height="{HEIGHT}px"
    role="slider"
    tabindex="0"
    aria-label="Jump to a book"
    aria-valuemin="0"
    aria-valuemax={Math.max(0, bookCount - 1)}
    aria-valuenow={activeOrder}
    aria-valuetext={activeBook}
    onclick={onClick}
    onkeydown={onKeydown}
  ></canvas>
</div>

<style>
  .strip {
    height: 30px;
    border-top: 1px solid var(--rule, #d8cba8);
  }
  canvas {
    display: block;
    width: 100%;
    cursor: pointer;
  }
  /* Inset so the ring sits inside the 30px band instead of over the text above. */
  canvas:focus-visible {
    outline: 2px solid var(--gold, #9e7d38);
    outline-offset: -2px;
  }
</style>
