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
  const seg = s.engine.canonSegments(); // {segments:[{label,first,last}], otNtDivide}
  const toc = s.engine.toc();
  const bookCount: number = toc.books.length;
  const orderOf = new Map<string, number>(toc.books.map((b: any, i: number) => [b.id, i]));

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

  function onClick(e: MouseEvent): void {
    const rect = canvas.getBoundingClientRect();
    const idx = Math.min(bookCount - 1, Math.max(0, Math.floor(((e.clientX - rect.left) / cssW) * bookCount)));
    const book = toc.books[idx];
    if (book) s.navigate(s.activePane, book.id, 1);
  }
</script>

<div class="strip" bind:this={host}>
  <canvas bind:this={canvas} style:height="{HEIGHT}px" onclick={onClick}></canvas>
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
</style>
