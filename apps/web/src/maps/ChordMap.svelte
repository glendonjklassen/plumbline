<script lang="ts">
  // Chord/arc "Weave map" (manifest §Chord/arc): the fold comes from the core
  // (plumbline_engine_chord_map_json); the shell paints the canon axis, ribbons
  // heaviest-first, and routes clicks x→book → active pane.
  import MapFrame from "./MapFrame.svelte";
  import { getSession } from "../state/session.svelte";
  import type { ZoomState } from "./zoomable";

  const s = getSession();
  const W = 1000;
  const H = 360;

  const model = $derived.by(() => {
    void s.studyEpoch;
    return s.q("chordMap"); // {pairs:[{a,b,count}], max, otNtDivide, bookCount}
  });
  const seg = $derived(s.q("canonSegments"));
  const toc = $derived(s.q("toc"));

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement | undefined = $state();
  let zoom: ZoomState = $state({ scale: 1, x: 0, y: 0 });

  $effect(() => {
    void model;
    void zoom;
    if (!canvas || !host) return;
    const cssW = host.clientWidth;
    const cssH = host.clientHeight;
    const dpr = devicePixelRatio || 1;
    canvas.width = Math.round(cssW * dpr);
    canvas.height = Math.round(cssH * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#f2eee6";
    ctx.fillRect(0, 0, cssW, cssH);
    ctx.translate(zoom.x, zoom.y);
    ctx.scale(zoom.scale * (cssW / W), zoom.scale * (cssH / H));
    paint(ctx);
  });

  function paint(ctx: CanvasRenderingContext2D): void {
    const m = model;
    const baseY = H - 46;
    const n = m.bookCount;
    const xOf = (book: number) => ((book + 0.5) / n) * W;

    // Canon axis: section bands + labels, gold baseline, OT/NT seam.
    ctx.textBaseline = "middle";
    ctx.font = '11px "EB Garamond", Georgia, serif';
    seg.segments.forEach((sec: any, i: number) => {
      const x0 = (sec.first / n) * W;
      const x1 = ((sec.last + 1) / n) * W;
      if (i % 2 === 1) {
        ctx.fillStyle = "rgba(0,0,0,0.04)";
        ctx.fillRect(x0, baseY, x1 - x0, 34);
      }
      const w = ctx.measureText(sec.label).width;
      if (w < x1 - x0 - 4) {
        ctx.fillStyle = "#8a8276";
        ctx.fillText(sec.label, (x0 + x1) / 2 - w / 2, baseY + 18);
      }
    });
    ctx.strokeStyle = "#9e7d38";
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.moveTo(0, baseY);
    ctx.lineTo(W, baseY);
    ctx.stroke();
    const seamX = (m.otNtDivide / n) * W;
    ctx.strokeStyle = "rgba(138,130,118,0.8)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(seamX, 12);
    ctx.lineTo(seamX, baseY + 30);
    ctx.stroke();

    // Ribbons heaviest-first.
    const pairs = [...(m.pairs ?? [])].sort((p: any, q: any) => q.count - p.count);
    for (const pr of pairs) {
      const frac = m.max > 0 ? pr.count / m.max : 0;
      const alpha = Math.min(0.12 + 0.3 * frac + 0.08, 0.5);
      const ot = pr.a < m.otNtDivide;
      const nt = pr.b >= m.otNtDivide;
      const [r, g, b] =
        ot && nt ? [0.78, 0.59, 0.86] : ot ? [0.82, 0.7, 0.43] : [0.5, 0.7, 0.9];
      ctx.fillStyle = `rgba(${Math.round(r * 255)},${Math.round(g * 255)},${Math.round(b * 255)},${alpha})`;
      const fw = 2 + 8 * frac;
      const x1 = xOf(pr.a);
      const x2 = xOf(pr.b);
      if (pr.a === pr.b) {
        ctx.beginPath();
        ctx.arc(x1, baseY - 8, 8, 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      const dx = Math.abs(x2 - x1);
      const apex = baseY - Math.min(0.42 * H, 22 + 0.26 * H * (dx / W) * 4);
      ctx.beginPath();
      ctx.moveTo(x1 - fw / 2, baseY);
      ctx.quadraticCurveTo((x1 + x2) / 2, apex, x2 - fw / 2, baseY);
      ctx.lineTo(x2 + fw / 2, baseY);
      ctx.quadraticCurveTo((x1 + x2) / 2, apex, x1 + fw / 2, baseY);
      ctx.closePath();
      ctx.fill();
    }
  }

  function onClick(e: MouseEvent): void {
    if (!host) return;
    const rect = canvas.getBoundingClientRect();
    const px = ((e.clientX - rect.left - zoom.x) / (zoom.scale * (rect.width / W)) / W) * model.bookCount;
    const idx = Math.min(model.bookCount - 1, Math.max(0, Math.floor(px)));
    const book = toc?.books?.[idx];
    if (book) {
      s.navigate(s.activePane, book.id, 1);
      s.mapPopup = null;
    }
  }
</script>

<MapFrame title="Weave map" width={W} height={H} onZoom={(z) => (zoom = z)}>
  <div class="fill" bind:this={host}>
    <canvas bind:this={canvas} onclick={onClick}></canvas>
  </div>
</MapFrame>

<style>
  .fill {
    position: absolute;
    inset: 0;
  }
</style>
