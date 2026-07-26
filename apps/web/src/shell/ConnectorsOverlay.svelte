<script lang="ts">
  // Ambient weave connectors (manifest §Ambient weave connectors): an
  // input-transparent canvas over the pane row. Pairs come deduped from the
  // core (plumbline_engine_link_pairs_json); the shell only maps endpoints to
  // showing panes and draws the Béziers riding the gutters.
  import { getSession } from "../state/session.svelte";
  import { MARGIN } from "../reader/paint";

  const LINK_INSET = 14;
  const YINSET = 5;
  const NAV_H = 33; // pane nav strip height (px) — canvas sits below it

  const s = getSession();

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let cssW = $state(0);
  let cssH = $state(0);

  $effect(() => {
    const ro = new ResizeObserver(() => {
      cssW = host.clientWidth;
      cssH = host.clientHeight;
    });
    ro.observe(host);
    return () => ro.disconnect();
  });

  const pairs = $derived.by(() => {
    void s.studyEpoch;
    return (s.q("linkPairs")?.pairs ?? []).filter((p: any) => p.resolved);
  });

  let rafId = 0;
  $effect(() => {
    void pairs;
    void s.palette;
    void s.paneVerseGeom;
    void s.panes.map((p) => `${p.book}:${p.chapter}:${p.scrollY}`).join();
    void cssW;
    void cssH;
    cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(draw);
  });

  function draw(): void {
    if (!canvas || cssW <= 0) return;
    const dpr = devicePixelRatio || 1;
    canvas.width = Math.round(cssW * dpr);
    canvas.height = Math.round(cssH * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);
    const n = s.panes.length;
    if (n < 2) return;

    const paneW = cssW / n;
    // Later pane wins duplicates (manifest): walk panes in order into a map.
    const paneFor = new Map<string, number>();
    s.panes.forEach((p, i) => paneFor.set(`${p.book}|${p.chapter}`, i));

    // Endpoint y in overlay coords: verse line centre − pane scroll, clamped
    // into the pane's visible band ±YINSET so an off-screen end lingers as an
    // edge dot.
    const endpointY = (paneIdx: number, verse: number): number | null => {
      const geom = s.paneVerseGeom[paneIdx]?.get(verse);
      if (!geom) return null;
      const y = NAV_H + MARGIN + geom.y + geom.h / 2 - s.panes[paneIdx].scrollY;
      return Math.min(Math.max(y, NAV_H + YINSET), cssH - YINSET);
    };

    const gold = s.palette.gold ?? "#9e7d38";
    for (const pr of pairs) {
      const ia = paneFor.get(`${pr.aBook}|${pr.aChapter}`);
      const ib = paneFor.get(`${pr.bBook}|${pr.bChapter}`);
      if (ia === undefined || ib === undefined || ia === ib) continue;
      const [li, lv, ri, rv] = ia < ib ? [ia, pr.aVerse, ib, pr.bVerse] : [ib, pr.bVerse, ia, pr.aVerse];
      const y1 = endpointY(li, lv);
      const y2 = endpointY(ri, rv);
      if (y1 === null || y2 === null) continue;
      const x1 = (li + 1) * paneW - LINK_INSET;
      const x2 = ri * paneW + LINK_INSET;
      const dx = x2 - x1;
      ctx.strokeStyle = withAlpha(gold, 0.35);
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.bezierCurveTo(x1 + dx * 0.4, y1, x2 - dx * 0.4, y2, x2, y2);
      ctx.stroke();
      ctx.fillStyle = withAlpha(gold, 0.7);
      for (const [x, y] of [
        [x1, y1],
        [x2, y2],
      ] as const) {
        ctx.beginPath();
        ctx.arc(x, y, 2, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  function withAlpha(hex: string, alpha: number): string {
    const v = parseInt(hex.slice(1), 16);
    return `rgba(${(v >> 16) & 255},${(v >> 8) & 255},${v & 255},${alpha})`;
  }
</script>

<div class="overlay" bind:this={host}>
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 5;
  }
  canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
