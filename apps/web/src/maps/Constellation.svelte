<script lang="ts">
  // Constellation (manifest §Constellation popup): lanes/nodes/edges arrive
  // as fractions from plumbline_engine_constellation_json; the shell maps to the
  // same pixel constants as GTK/WinUI (plotLeft 162, topPad 18, gutter 150,
  // node 1.4+2.4·size) so all shells place a node alike. Hit priority
  // node > edge > pin-gutter; node navigates (stays open), edge opens the
  // compare card (closes), gutter toggles the pin.
  import MapFrame from "./MapFrame.svelte";
  import { getSession } from "../state/session.svelte";
  import type { ZoomState } from "./zoomable";

  const s = getSession();
  const W = 1200;
  const H = 640;
  const PLOT_LEFT = 162;
  const TOP_PAD = 18;
  const GUTTER = 150;

  let page = $state(0);
  let pins = $state<number[]>([]);

  const model = $derived.by(() => {
    void s.studyEpoch;
    return s.q("constellation", page, pins);
  });

  const COLORS = ["#8f6b28", "#5f7a94", "#7a8f5f", "#94655f", "#6b5f94", "#8f5f82", "#5f8f8a"];

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement | undefined = $state();
  let zoom: ZoomState = $state({ scale: 1, x: 0, y: 0 });
  let hover = $state("");

  interface NodePos {
    x: number;
    y: number;
    r: number;
    node: any;
    lane: any;
  }
  let nodePos: NodePos[] = [];
  let edgePos: { x1: number; y1: number; x2: number; y2: number; weaveIndex: number }[] = [];

  const laneH = $derived(model ? (H - TOP_PAD - 10) / model.laneCapacity : 0);

  $effect(() => {
    void model;
    void zoom;
    if (!canvas || !host || !model) return;
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

  function xOf(frac: number): number {
    return PLOT_LEFT + frac * (W - PLOT_LEFT);
  }

  function paint(ctx: CanvasRenderingContext2D): void {
    const m = model;
    nodePos = [];
    edgePos = [];

    // Canon ruler + OT/NT seam.
    const seg = s.q("canonSegments");
    if (!seg) return;
    const n = seg.segments.at(-1).last + 1;
    ctx.strokeStyle = "rgba(158,125,56,0.5)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(PLOT_LEFT, TOP_PAD - 8);
    ctx.lineTo(W, TOP_PAD - 8);
    ctx.stroke();
    const seamX = xOf(seg.otNtDivide / n);
    ctx.strokeStyle = "rgba(138,130,118,0.55)";
    ctx.setLineDash([3, 4]);
    ctx.beginPath();
    ctx.moveTo(seamX, TOP_PAD - 8);
    ctx.lineTo(seamX, H - 10);
    ctx.stroke();
    ctx.setLineDash([]);

    m.lanes.forEach((lane: any, li: number) => {
      const color = COLORS[li % COLORS.length];
      const yOfLane = (frac: number) => TOP_PAD + (li + frac) * laneH;
      // Pin gutter marker + lane name (≤22 chars).
      const py = yOfLane(0.5);
      if (lane.pinned) {
        ctx.fillStyle = "#9e7d38";
        ctx.fillRect(10, py - 4, 8, 8);
      } else {
        ctx.strokeStyle = "#8a8276";
        ctx.lineWidth = 1;
        ctx.strokeRect(10.5, py - 3.5, 7, 7);
      }
      ctx.font = '12px "EB Garamond", Georgia, serif';
      ctx.fillStyle = "#211f1a";
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      const name = String(lane.name).slice(0, 22);
      ctx.fillText(name, 26, py, GUTTER - 32);

      for (const e of lane.edges ?? []) {
        const x1 = xOf(e.aX);
        const y1 = yOfLane(e.aLaneFrac);
        const x2 = xOf(e.bX);
        const y2 = yOfLane(e.bLaneFrac);
        edgePos.push({ x1, y1, x2, y2, weaveIndex: lane.weaveIndex });
        ctx.strokeStyle = `${color}66`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.lineTo(x2, y2);
        ctx.stroke();
      }
      for (const nd of lane.nodes ?? []) {
        const x = xOf(nd.x);
        const y = yOfLane(nd.laneFrac);
        const r = 1.4 + 2.4 * nd.size;
        nodePos.push({ x, y, r, node: nd, lane });
        ctx.fillStyle = color;
        ctx.fillRect(x - r, y - r, r * 2, r * 2);
      }
    });
  }

  function toModel(e: MouseEvent): { mx: number; my: number } {
    const rect = canvas.getBoundingClientRect();
    return {
      mx: (e.clientX - rect.left - zoom.x) / (zoom.scale * (rect.width / W)),
      my: (e.clientY - rect.top - zoom.y) / (zoom.scale * (rect.height / H)),
    };
  }

  function hit(mx: number, my: number): { kind: "node" | "edge" | "pin"; value: any } | null {
    for (const np of nodePos)
      if (Math.abs(np.x - mx) < np.r + 4 && Math.abs(np.y - my) < np.r + 4)
        return { kind: "node", value: np };
    for (const ep of edgePos) {
      const d = pointSegDist(mx, my, ep.x1, ep.y1, ep.x2, ep.y2);
      if (d < 4) return { kind: "edge", value: ep };
    }
    if (mx < GUTTER && model) {
      const li = Math.floor((my - TOP_PAD) / laneH);
      const lane = model.lanes[li];
      if (lane) return { kind: "pin", value: lane };
    }
    return null;
  }

  function pointSegDist(px: number, py: number, x1: number, y1: number, x2: number, y2: number): number {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy || 1)));
    return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
  }

  function onClick(e: MouseEvent): void {
    const { mx, my } = toModel(e);
    const h = hit(mx, my);
    if (!h) return;
    if (h.kind === "node") {
      const nd = h.value.node;
      s.navigate(s.activePane, nd.book, nd.chapter, nd.verse ?? null);
    } else if (h.kind === "edge") {
      s.panel = { kind: "compare", index: h.value.weaveIndex };
      s.mapPopup = null;
    } else {
      const idx = h.value.weaveIndex;
      pins = pins.includes(idx) ? pins.filter((p) => p !== idx) : [...pins, idx];
    }
  }

  function onMove(e: MouseEvent): void {
    const { mx, my } = toModel(e);
    const h = hit(mx, my);
    hover = h?.kind === "node" ? `${h.value.node.display} · ${h.value.lane.name}` : "";
    canvas.style.cursor = h ? "pointer" : "default";
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "ArrowLeft") page = Math.max(0, page - 1);
    else if (e.key === "ArrowRight") page = Math.min(model?.maxPage ?? 0, page + 1);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<MapFrame
  title="Constellation"
  caption={hover || model?.caption || ""}
  width={W}
  height={H}
  loading={!model}
  onZoom={(z) => (zoom = z)}
  pager={model ? { page: model.page, maxPage: model.maxPage, onPage: (d) => (page = Math.min(Math.max(page + d, 0), model.maxPage)) } : null}
>
  <div class="fill" bind:this={host}>
    <canvas bind:this={canvas} onclick={onClick} onmousemove={onMove}></canvas>
  </div>
</MapFrame>

<style>
  .fill {
    position: absolute;
    inset: 0;
  }
</style>
