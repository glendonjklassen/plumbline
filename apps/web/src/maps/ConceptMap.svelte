<script lang="ts">
  // Concept map (manifest §Concept map popup): radial spokes + canon
  // dispersion strip, with the optional cross-testament bridge row (indigo)
  // beneath the gold one. The whole model comes from
  // pure_engine_concept_map_json; spoke clicks recenter, centre opens the
  // code's study card.
  import MapFrame from "./MapFrame.svelte";
  import { getSession } from "../state/session.svelte";
  import type { ZoomState } from "./zoomable";

  interface Props {
    code: string;
  }
  let { code }: Props = $props();

  const s = getSession();
  const W = 720;
  const H = 560;

  const model = $derived(s.engine.conceptMap(code));

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement | undefined = $state();
  let zoom: ZoomState = $state({ scale: 1, x: 0, y: 0 });
  let spokePos: { x: number; y: number; code: string }[] = [];

  const caption = $derived.by(() => {
    const b = model?.bridge;
    if (!b?.partners?.length) return "";
    return `across the testaments: ${b.partners.map((p: any) => p.label.replace("\n", " ")).join(" · ")}`;
  });

  $effect(() => {
    void model;
    void zoom;
    if (!canvas || !host || !model) return;
    const cssW = host.clientWidth;
    const dpr = devicePixelRatio || 1;
    const cssH = host.clientHeight;
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
    const stripH = 40;
    const bridgeH = m.bridge ? 52 : 0;
    const cx = W / 2;
    const cy = (H - stripH - bridgeH) / 2;
    const radius = Math.min(W, H - stripH - bridgeH) / 2 - 95;

    // Spokes: semantic gold, community green.
    spokePos = [];
    ctx.textAlign = "center";
    const nSpokes = m.spokes.length || 1;
    m.spokes.forEach((sp: any, i: number) => {
      const angle = (i / nSpokes) * Math.PI * 2 - Math.PI / 2;
      const x = cx + Math.cos(angle) * radius;
      const y = cy + Math.sin(angle) * radius;
      spokePos.push({ x, y, code: sp.code });
      const color = sp.semantic ? "#9e7d38" : "#6f8f6a";
      ctx.strokeStyle = `${color}55`;
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(x, y);
      ctx.stroke();
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(x, y, 5, 0, Math.PI * 2);
      ctx.fill();
      const [gloss, lemma] = String(sp.label).split("\n");
      const ly = y + (Math.sin(angle) > 0.3 ? 16 : -22);
      ctx.fillStyle = "#211f1a";
      ctx.font = '13px "EB Garamond", Georgia, serif';
      ctx.fillText(gloss ?? "", x, ly);
      if (lemma) {
        ctx.font = 'italic 12px "EB Garamond", Georgia, serif';
        ctx.fillStyle = "#8a7a52";
        ctx.fillText(lemma, x, ly + 14);
      }
    });
    // Centre node + label.
    ctx.fillStyle = "#9e7d38";
    ctx.beginPath();
    ctx.arc(cx, cy, 8, 0, Math.PI * 2);
    ctx.fill();
    const [cGloss, cLemma] = String(m.centerLabel ?? m.code).split("\n");
    ctx.font = 'bold 15px "EB Garamond", Georgia, serif';
    ctx.fillStyle = "#211f1a";
    ctx.fillText(cGloss ?? m.code, cx, cy + 26);
    if (cLemma) {
      ctx.font = 'italic 13px "EB Garamond", Georgia, serif';
      ctx.fillStyle = "#8a7a52";
      ctx.fillText(cLemma, cx, cy + 42);
    }

    // Dispersion strips (gold; bridge row indigo beneath).
    const strip = (byBook: number[], y0: number, h: number, rgb: [number, number, number], base: number, span: number) => {
      const max = Math.max(1, ...byBook);
      const cw = W / m.bookCount;
      byBook.forEach((cnt, bi) => {
        if (!cnt) return;
        ctx.fillStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${base + span * (cnt / max)})`;
        ctx.fillRect(bi * cw, y0, Math.max(cw - 0.5, 0.8), h);
      });
      const seamX = (m.otNtDivide / m.bookCount) * W;
      ctx.strokeStyle = "rgba(138,130,118,0.9)";
      ctx.beginPath();
      ctx.moveTo(seamX, y0);
      ctx.lineTo(seamX, y0 + h);
      ctx.stroke();
    };
    strip(m.byBook, H - stripH - bridgeH, stripH - 6, [158, 125, 56], 0.15, 0.75);
    if (m.bridge) strip(m.bridge.byBook, H - bridgeH, bridgeH - 6, [74, 95, 165], 0.18, 0.72);
  }

  function onClick(e: MouseEvent): void {
    if (!host) return;
    const rect = canvas.getBoundingClientRect();
    const mx = (e.clientX - rect.left - zoom.x) / (zoom.scale * (rect.width / W));
    const my = (e.clientY - rect.top - zoom.y) / (zoom.scale * (rect.height / H));
    for (const sp of spokePos) {
      if (Math.hypot(sp.x - mx, sp.y - my) < 22) {
        s.mapPopup = { kind: "conceptMap", code: sp.code };
        return;
      }
    }
    // Dispersion strip → jump to that book.
    if (my > H - 40 - (model.bridge ? 52 : 0)) {
      const toc = s.engine.toc();
      const idx = Math.min(model.bookCount - 1, Math.max(0, Math.floor((mx / W) * model.bookCount)));
      const book = toc.books[idx];
      if (book) {
        s.navigate(s.activePane, book.id, 1);
        s.mapPopup = null;
      }
    }
  }
</script>

<MapFrame title="Concept map — {code}" {caption} width={W} height={H} onZoom={(z) => (zoom = z)}>
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
