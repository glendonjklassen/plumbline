<script lang="ts">
  // Shared popup frame for the three analytical maps: light popup paper (by
  // design, even in dark themes — matches the desktop shells), title bar,
  // close button, optional pager, zoomable canvas host.
  import type { Snippet } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { zoomable, type ZoomState } from "./zoomable";

  interface Props {
    title: string;
    caption?: string;
    width: number;
    height: number;
    onZoom?: (z: ZoomState) => void;
    pager?: { page: number; maxPage: number; onPage: (d: -1 | 1) => void } | null;
    /** True while the map's model is still being computed — the frame paints
     *  the wait instead of leaving blank paper. */
    loading?: boolean;
    children: Snippet;
  }
  let {
    title,
    caption = "",
    width,
    height,
    onZoom,
    pager = null,
    loading = false,
    children,
  }: Props = $props();

  const s = getSession();

  // The first analytical map of a session pays for a corpus-wide sweep; the
  // rest are instant. Blank paper for several seconds reads as broken
  // (feedback 2026-07-27), so once the wait is real, name it and say it is
  // one-time. See the twin in StudyPanel.svelte.
  const SLOW_MAP_MS = 600;
  let slow = $state(false);
  $effect(() => {
    if (!loading) {
      slow = false;
      return;
    }
    const t = setTimeout(() => (slow = true), SLOW_MAP_MS);
    return () => clearTimeout(t);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="backdrop" onclick={() => (s.mapPopup = null)}></div>
<div class="popup" style:max-width="min({width}px, 96vw)">
  <div class="bar">
    <span class="title">{title}</span>
    {#if pager}
      <span class="pager">
        <button onclick={() => pager!.onPage(-1)} disabled={pager.page <= 0} aria-label="Previous page">‹</button>
        <span>{pager.page + 1} / {pager.maxPage + 1}</span>
        <button onclick={() => pager!.onPage(1)} disabled={pager.page >= pager.maxPage} aria-label="Next page">›</button>
      </span>
    {/if}
    <span class="caption">{caption}</span>
    <button class="close" onclick={() => (s.mapPopup = null)} aria-label="Close">✕</button>
  </div>
  <div
    class="host"
    style:aspect-ratio="{width} / {height}"
    use:zoomable={(z) => onZoom?.(z)}
  >
    {@render children()}
    {#if loading}
      <div class="wait" aria-live="polite">
        <span class="waitline">— building —</span>
        {#if slow}
          <span class="waitnote">
            The first map of a session takes a few seconds: the whole text is being swept for this.
            The maps you open after it appear at once.
          </span>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.4);
    z-index: 34;
  }
  .popup {
    position: fixed;
    z-index: 35;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 96vw;
    /* Analytical popups keep light paper in every theme (parity with GTK/WinUI). */
    background: #f2eee6;
    color: #211f1a;
    border: 1px solid #d8cba8;
    border-radius: 12px;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.35);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid #d8cba8;
  }
  .title {
    font-weight: 600;
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: #8a8276;
  }
  .pager button {
    padding: 0 8px;
    font-size: 16px;
    color: #211f1a;
  }
  .pager button:disabled {
    opacity: 0.35;
  }
  .caption {
    flex: 1;
    font-size: 12.5px;
    color: #8a8276;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .close {
    color: #8a8276;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .close:hover {
    background: rgba(158, 125, 56, 0.14);
  }
  .host {
    position: relative;
    width: 100%;
    max-height: 82vh;
    overflow: hidden;
    touch-action: none;
  }
  .wait {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 0 12%;
    text-align: center;
    /* The popup's own paper, so the canvas underneath never shows through. */
    background: #f2eee6;
  }
  .waitline {
    color: #8a8276;
    font-size: 13.5px;
    animation: waitpulse 1.1s ease-in-out infinite;
  }
  .waitnote {
    color: #8a8276;
    font-size: 12.5px;
    line-height: 1.5;
    max-width: 46ch;
  }
  @keyframes waitpulse {
    0%,
    100% {
      opacity: 0.35;
    }
    50% {
      opacity: 1;
    }
  }
  .host :global(canvas) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
