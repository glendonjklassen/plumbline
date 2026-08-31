<script lang="ts">
  import { t } from "./i18n.svelte";
  // One bar for every destination that has one. The metrics deliberately match
  // the app header's — same background, rule, 52px floor and padding — so it
  // reads as the same furniture one row down.

  interface Props {
    /** What this destination is called. */
    title: string;
    /** Where the back arrow goes. */
    onBack: () => void;
    /** What the arrow means here — "Back to the hymn list". Defaults to the
     *  catalogue's "back to reading", filled in at render rather than as a
     *  default parameter, so a language change repaints it. */
    backLabel?: string;
    /** Controls on the right of the bar (language, chords, Sing…). */
    actions?: import("svelte").Snippet;
    /** Raise the ≡ utilities menu. Every destination passes this; transient
     *  overlays leave it unset. */
    onMenu?: () => void;
  }
  const { title, onBack, backLabel, actions, onMenu }: Props = $props();
</script>

<div class="bar">
  <button class="back" onclick={onBack} aria-label={backLabel ?? t("bar.backToReading")}>‹</button>
  <h2>{title}</h2>
  <span class="spacer"></span>
  {#if actions}{@render actions()}{/if}
  {#if onMenu}
    <button class="menu" onclick={onMenu} aria-label={t("common.menu")}>≡</button>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    /* Set by Shell when this bar is the topmost chrome on a phone (the app
       header hides on a destination), so the status bar does not sit on the
       title. Zero everywhere else, where the header is still above us. */
    padding-top: calc(10px + var(--screenBarTop, 0px));
    min-height: 52px;
    background: var(--paneNavBg, #efeae1);
    border-bottom: 1px solid var(--rule, #d8cba8);
    /* The hymnal's bar carries language, chords, transpose and Sing, which at a
       large text size is wider than a phone. Wrapping beats pushing a control
       off the end. */
    flex-wrap: wrap;
  }
  .back {
    font-size: calc(22px * var(--uiScale, 1));
    line-height: 1;
    padding: 8px 14px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .back:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  .menu {
    font-size: calc(20px * var(--uiScale, 1));
    line-height: 1;
    padding: 8px 12px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .menu:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  /* The ≡ shows only where this bar is the topmost chrome (a phone hides the app
     header on a destination). Above 700px — the header's own breakpoint — the
     header's ≡ is still two rows up, and a second one reads as two menus. */
  @media (min-width: 701px) {
    .menu {
      display: none;
    }
  }
  h2 {
    margin: 0;
    font-size: calc(18px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .spacer {
    flex: 1;
  }
</style>
