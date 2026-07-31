<script lang="ts">
  // Embedded study maps (Android StudyMaps parity): the concept map's canon
  // dispersion (+ the cross-testament bridge row) as a compact, first-class
  // card inside the word-study surface, tapping through to the fullscreen
  // map. Machine-tier — only rendered when that gate is on.
  import { getSession } from "../state/session.svelte";

  interface Props {
    code: string;
  }
  let { code }: Props = $props();

  const s = getSession();
  const model = $derived(s.q("conceptMap", code));

  let canvas: HTMLCanvasElement | undefined = $state();
  let host: HTMLDivElement | undefined = $state();

  $effect(() => {
    void model;
    void s.palette;
    if (!canvas || !host || !model) return;
    const cssW = host.clientWidth;
    const bridge = !!model.bridge;
    const cssH = bridge ? 64 : 40;
    const dpr = devicePixelRatio || 1;
    canvas.width = Math.round(cssW * dpr);
    canvas.height = Math.round(cssH * dpr);
    canvas.style.height = `${cssH}px`;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);
    const strip = (byBook: number[], y0: number, h: number, rgb: [number, number, number], base: number, span: number) => {
      const max = Math.max(1, ...byBook);
      const cw = cssW / model.bookCount;
      byBook.forEach((cnt: number, bi: number) => {
        if (!cnt) return;
        ctx.fillStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${base + span * (cnt / max)})`;
        ctx.fillRect(bi * cw, y0, Math.max(cw - 0.5, 0.8), h);
      });
      const seamX = (model.otNtDivide / model.bookCount) * cssW;
      ctx.strokeStyle = "rgba(138,130,118,0.9)";
      ctx.beginPath();
      ctx.moveTo(seamX, y0);
      ctx.lineTo(seamX, y0 + h);
      ctx.stroke();
    };
    strip(model.byBook, 2, 32, [158, 125, 56], 0.15, 0.75);
    if (bridge) strip(model.bridge.byBook, 38, 24, [74, 95, 165], 0.18, 0.72);
  });

  const caption = $derived.by(() => {
    if (!model) return "";
    const n = model.spokes?.length ?? 0;
    const b = model.bridge?.partners?.length
      ? ` · across the testaments: ${model.bridge.partners
          .slice(0, 3)
          .map((p: any) => String(p.label).replace("\n", " "))
          .join(" · ")}`
      : "";
    return `${n} related concept${n === 1 ? "" : "s"}${b}`;
  });
</script>

{#if model}
  <button class="card" onclick={() => (s.mapPopup = { kind: "conceptMap", code })}>
    <span class="head">
      <span class="title">most used in</span>
      <span class="open">▸ open concept map</span>
    </span>
    <div class="strip" bind:this={host}>
      <canvas bind:this={canvas}></canvas>
    </div>
    <span class="caption">{caption}</span>
  </button>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    text-align: left;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 9px;
    background: var(--paper, #fcf9f4);
    padding: 8px 10px;
    margin: 8px 0;
  }
  .card:hover {
    border-color: var(--gold, #9e7d38);
  }
  .head {
    display: flex;
    align-items: baseline;
  }
  .title {
    font-size: calc(11px * var(--uiScale, 1));
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--section, #a0894a);
    flex: 1;
  }
  .open {
    font-size: calc(11.5px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
  }
  .strip {
    width: 100%;
  }
  canvas {
    display: block;
    width: 100%;
  }
  .caption {
    font-size: calc(11.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
