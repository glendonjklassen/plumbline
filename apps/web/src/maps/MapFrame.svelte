<script lang="ts">
  // Shared popup frame for the three analytical maps: light popup paper (by
  // design, even in dark themes — matches the desktop shells), title bar,
  // close button, optional pager, zoomable canvas host.
  import type { Snippet } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { zoomable, type ZoomState } from "./zoomable";
  import { t } from "../lib/i18n.svelte";

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

  // Night is the one theme where light popup paper is wrong: #f2eee6 against a
  // true-black reader is an 18:1 step, which glares (2026-07-29). Night is always
  // an explicit choice, never resolved from the system, so the config token is
  // exact.
  const night = $derived((s.config.theme ?? "system") === "night");

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
<div class="popup" class:night style:max-width="min({width}px, 96vw)">
  <div class="bar">
    <span class="title">{title}</span>
    {#if pager}
      <span class="pager">
        <button onclick={() => pager!.onPage(-1)} disabled={pager.page <= 0} aria-label={t("map.previousPage")}>‹</button>
        <span>{pager.page + 1} / {pager.maxPage + 1}</span>
        <button onclick={() => pager!.onPage(1)} disabled={pager.page >= pager.maxPage} aria-label={t("map.nextPage")}>›</button>
      </span>
    {/if}
    <span class="caption">{caption}</span>
    <button class="close" onclick={() => (s.mapPopup = null)} aria-label={t("common.close")}>✕</button>
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
    font-size: calc(13px * var(--uiScale, 1));
    color: #6c665d;
  }
  .pager button {
    padding: 0 8px;
    font-size: calc(16px * var(--uiScale, 1));
    color: #211f1a;
  }
  .pager button:disabled {
    opacity: 0.35;
  }
  .caption {
    flex: 1;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: #6c665d;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .close {
    color: #6c665d;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .close:hover {
    background: rgba(125, 99, 44, 0.14);
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
    color: #6c665d;
    font-size: calc(13.5px * var(--uiScale, 1));
    animation: waitpulse 1.1s ease-in-out infinite;
  }
  .waitnote {
    color: #6c665d;
    font-size: calc(12.5px * var(--uiScale, 1));
    line-height: 1.5;
    max-width: 46ch;
  }
  /* The breath used to be `opacity: 0.35` → 1, which put the words at 1.7:1 for
     most of the cycle. Between two solid tones instead: 4.9:1 at the quiet end,
     14.2:1 at the loud one. */
  @keyframes waitpulse {
    0%,
    100% {
      color: #6c665d;
    }
    50% {
      color: #211f1a;
    }
  }
  .host :global(canvas) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  /* NIGHT — dimmed, not inverted. Each map paints its own #f2eee6 paper onto the
     canvas, so the frame can't just darken itself: it would leave a bright
     rectangle inside a dark border. Dimming the canvas by the same factor the
     frame restates keeps the two exactly matched (#f2eee6 × 0.70 = #a9a7a1) and
     drops the popup's step from the black reader from 18.1:1 to 8.7:1.
     Every value below is measured against that #a9a7a1: ink 6.8:1, muted 5.0:1,
     both clear AA. It is one number — raise 0.70 for a brighter popup, lower it
     for a darker one, and re-check the two text tones against the result. */
  .popup.night {
    background: #a9a7a1;
    border-color: #544f45;
  }
  .popup.night .bar {
    border-bottom-color: #544f45;
  }
  .popup.night .pager,
  .popup.night .caption,
  .popup.night .close,
  .popup.night .waitnote {
    color: #3b362d;
  }
  .popup.night .close:hover {
    background: rgba(33, 31, 26, 0.14);
  }
  .popup.night .wait {
    background: #a9a7a1;
  }
  .popup.night .waitline {
    animation-name: waitpulse-night;
  }
  @keyframes waitpulse-night {
    0%,
    100% {
      color: #3b362d;
    }
    50% {
      color: #211f1a;
    }
  }
  .popup.night .host :global(canvas) {
    filter: brightness(0.7);
  }
</style>
