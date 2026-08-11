<script lang="ts">
  import { t } from "./i18n.svelte";
  // ONE bar for every destination that has one.
  //
  // The metrics are the app header's on purpose, so the second bar reads as the
  // same piece of furniture one row down rather than a different designer's
  // work: same background, same rule, same 52px floor, same padding.

  interface Props {
    /** What this destination is called. */
    title: string;
    /** Where the back arrow goes. */
    onBack: () => void;
    /** What the arrow means here — "Back to the hymn list". Defaults to the
     *  catalogue's "back to reading", filled in at RENDER rather than as a
     *  default parameter, so a language change repaints it. */
    backLabel?: string;
    /** Controls on the right of the bar (language, chords, Sing…). */
    actions?: import("svelte").Snippet;
    /** Raise the ≡ utilities menu. Every DESTINATION passes this (Settings
     *  must not cost a trip back to Read); transient overlays leave it unset. */
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
    /* The hymnal's bar can carry language, chords, a transpose group and Sing.
       At a large text size that is more than a phone's width, and wrapping is
       the graceful version of pushing a control off the end — the same call the
       app header makes. */
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
