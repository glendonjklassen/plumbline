<script lang="ts">
  // Present mode (Android parity, the top-priority request): a thread as a
  // fullscreen, high-contrast ("sunlight") large-type presentation for showing
  // someone in person. Picker → scrollable overview → tap-to-focus huge → end
  // card with plain-text share + a scannable QR of the hosted PWA.
  // Deliberately hard-coded light — the phone/laptop gets handed across in
  // daylight.
  import { getSession } from "../state/session.svelte";
  import QrCode, { PWA_URL } from "../shell/QrCode.svelte";
  import { hasChurch, shareUrl } from "../shell/church";
  import { plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  interface Entry {
    ref: string;
    display: string;
    body: string;
  }

  const threads = $derived.by(() => {
    void s.studyEpoch;
    return s.showPresent ? ((s.q("threads")?.threads ?? []) as any[]) : [];
  });

  let thread = $state<any | null>(null);
  let focus = $state<number | null>(null); // null = overview; entries.length = end card

  // A preselected thread (first-run "Sharing the gospel" → the Romans Road)
  // skips the picker; unknown names fall through to it.
  $effect(() => {
    if (!s.showPresent || !s.presentThreadName || threads.length === 0) return;
    const named = threads.find((x: any) => x.name === s.presentThreadName);
    if (named) {
      thread = named;
      focus = null;
    }
    s.presentThreadName = null;
  });

  const entries = $derived.by((): Entry[] => {
    if (!thread) return [];
    return (thread.entries ?? []).map((e: any) => {
      const v = s.q("verse", e.verse);
      return {
        ref: e.verse,
        display: v?.display ?? e.display ?? e.verse,
        body: v?.body || (e.text ?? []).join(" "),
      };
    });
  });

  // What is under the STATUS BAR, which is not the same question as "is Present
  // open": the picker below restates the palette (`.present.picking`) while the
  // presentation itself keeps the fixed sunlight paper, so only the second one
  // may pull the chrome off the reader's theme. See Session.applyChrome.
  $effect(() => {
    s.presentingThread = thread !== null;
  });

  // CLOSED BY ANY PATH — the ✕, Escape's back-peel, a destination tap from the
  // menu — this screen starts over. It did not: only `close()` nulled these, and
  // this component is mounted unconditionally (Shell.svelte) with only its
  // template gated on `s.showPresent`. So `thread` outlived a back-peel, and
  // reopening Present resumed a presentation the reader had walked away from.
  //
  // Reset the SOURCE, not the projection. `s.presentingThread` is derived from
  // `thread` by the effect above, so clearing just the flag — adding it to
  // Session.TRANSIENT, which is the fix that looks obvious — would leave the
  // stale presentation still painting its cream while the chrome went back to
  // the dark theme's polarity: light icons on a light surface. The reported
  // washout, manufactured by the repair.
  $effect(() => {
    if (!s.showPresent && thread !== null) {
      thread = null;
      focus = null;
    }
  });

  function close(): void {
    s.showPresent = false;
    thread = null;
    focus = null;
  }
  function back(): void {
    if (focus !== null) focus = null;
    else if (thread) thread = null;
    else close();
  }

  function shareText(): string {
    const lines = [thread.name, ""];
    for (const e of entries) {
      lines.push(e.display, e.body, "");
    }
    // The same link the header's Share gives: the church rides along, and this
    // is the screen you show someone face to face — exactly where it matters
    // most.
    lines.push(`Shared from Plumbline — ${s.presentShareLink}`);
    if (hasChurch(s.church)) {
      lines.push("");
      lines.push(s.church.service !== null ? `${s.church.name} — ${s.churchMeets(s.church)}` : s.church.name);
    }
    return lines.join("\n");
  }
  /** Hand over the APP (with the church, and marked for a new believer) —
   *  the Present twin of the header's Share. */
  async function shareAppLink(): Promise<void> {
    const url = s.presentShareLink;
    if (navigator.share) {
      try {
        await navigator.share({ title: "Plumbline", url });
        return;
      } catch (e) {
        // A dismissed sheet is an answer, not a failure — falling through would
        // overwrite the reader's clipboard for a share they just cancelled (and
        // writeText throws anyway: the closing sheet still holds the focus).
        // Every other rejection still gets the fallback. (ContextMenu's rule.)
        if ((e as DOMException | undefined)?.name === "AbortError") return;
      }
    }
    try {
      await navigator.clipboard.writeText(url);
      s.showToast(t("share.copied"));
    } catch {
      s.showToast(t("settings.copyBlocked"));
    }
  }

  // Sharing a passage is a QR, not the phone's share sheet. Present is the
  // screen you hold up to someone standing in front of you: a share sheet sends
  // a wall of text to an app they then have to leave, while a QR they scan puts
  // the passage on THEIR phone, in the reader, at the verse. `shareText()`
  // stays for the copy fallback — handy when the person isn't in front of you.
  let showQr = $state(false);
  const firstRef = $derived(entries[0]?.ref ?? null);
  /** The app link plus the verse this thread opens on. */
  const passageLink = $derived(
    shareUrl(PWA_URL, s.church, {
      startAsNewBeliever: s.config.presentSharesAsNew !== false,
      at: firstRef,
    }),
  );

  async function copyPassages(): Promise<void> {
    await navigator.clipboard.writeText(`${shareText()}\n\n${passageLink}`);
    s.showToast(t("present.copied"));
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!s.showPresent) return;
    if (e.key === "Escape") back();
    else if (focus !== null && (e.key === "ArrowRight" || e.key === " " || e.key === "PageDown"))
      focus = Math.min(focus + 1, entries.length);
    else if (focus !== null && (e.key === "ArrowLeft" || e.key === "PageUp"))
      focus = Math.max(focus - 1, 0);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if s.showPresent}
  <!-- The picker is the OWNER's screen — it follows the app theme; the
       presentation itself stays fixed-light sunlight. -->
  <div class="present" class:picking={!thread}>
    {#if !thread}
      <div class="bar">
        <button class="close" onclick={close} aria-label={t("common.close")}>✕</button>
        <span class="title">{t("present.title")}</span>
      </div>
      {#if threads.length === 0}
        <p class="empty">{t("present.empty")}</p>
      {:else}
        <div class="picker">
          <!-- `th`, not `t`: the catalogue lookup is called `t`, and an
               each-block binding of the same name would shadow it. -->
          {#each threads as th (th.name)}
            <button
              class="pick"
              onclick={() => {
                thread = th;
                focus = null;
              }}
            >
              <span class="name">{th.name}</span>
              <span class="n">{plural("present.passages.one", "present.passages.other", th.entries?.length ?? 0)}</span>
            </button>
          {/each}
        </div>
      {/if}
    {:else if focus === null}
      <div class="bar">
        <button class="close" onclick={() => (thread = null)} aria-label={t("bar.back")}>‹</button>
        <span class="title">{thread.name}</span>
        <span class="spacer"></span>
        <button class="sharebtn" onclick={() => (showQr = !showQr)}>
          {showQr ? t("present.hideQr") : t("present.share")}
        </button>
      </div>
      {#if showQr}
        <div class="qr sharesheet">
          <QrCode size={148} text={passageLink} />
          <span class="qrnote">{t("present.scanToOpen", { passage: entries[0]?.display ?? thread.name })}</span>
          <button class="linkbtn" onclick={copyPassages}>{t("present.copyPassages")}</button>
        </div>
      {/if}
      <div class="overview">
        <!-- KEYED BY POSITION, not by refKey. A thread may legitimately hold the
             same verse twice — a road can come back to a verse, and nothing in
             the format or the authoring path forbids it — and a duplicate key
             makes Svelte throw `each_key_duplicate`, which kills this component
             mid-render. What the reader saw was a Present that would not open
             its thread and a page that looked half-drawn (maintainer, "I added
             a couple of verses and it's all smushed"). The list is replaced
             wholesale on every change, so position is a sound identity here. -->
        {#each entries as e, i (i)}
          <button class="entry" onclick={() => (focus = i)}>
            <span class="ref">{e.display}</span>
            <span class="body">{e.body}</span>
          </button>
        {/each}
      </div>
    {:else if focus < entries.length}
      <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
      <div class="focus" onclick={() => (focus = Math.min(focus! + 1, entries.length))}>
        <p class="fref">{entries[focus].display}</p>
        <p class="fbody">{entries[focus].body}</p>
      </div>
      <div class="stepbar">
        <button onclick={(e) => (e.stopPropagation(), (focus = Math.max(focus! - 1, 0)))}>‹</button>
        <button class="ovbtn" onclick={(e) => (e.stopPropagation(), (focus = null))}>{t("present.overview")}</button>
        <span>{focus + 1} / {entries.length}</span>
        <button onclick={(e) => (e.stopPropagation(), (focus = Math.min(focus! + 1, entries.length)))}>›</button>
      </div>
    {:else}
      <div class="endcard">
        <p class="mark" aria-hidden="true">✦</p>
        <p class="fref">{thread.name}</p>
        <p class="endnote">{t("present.endNote")}</p>
        <!-- ONE QR, and it carries the passage: the end card's job is to hand
             this thread over, so the code opens the app AT its first verse
             rather than at whatever the recipient last read. The app-link button
             stays for the person who isn't in front of you — and is the only way
             to grab either link for a test. -->
        <div class="qr">
          <QrCode size={148} text={passageLink} />
          <span class="qrnote">{t("present.scanFor", { passage: entries[0]?.display ?? thread.name })}</span>
          <button class="linkbtn" onclick={copyPassages}>{t("present.copyPassages")}</button>
          <button class="linkbtn" onclick={shareAppLink}>{t("present.copyAppLink")}</button>
        </div>
        <button class="ovbtn" onclick={() => (focus = null)}>{t("present.backToOverview")}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Sunlight palette — deliberately fixed light, maximum contrast. Because it is
     fixed, every value here is a literal rather than a `var(--…)`, restated from
     the light palette (crates/core/src/theme.rs) to clear WCAG AA against this
     paper (#fcf9f4): the muted tones sit at 5.4:1 and 6.9:1, on the one screen
     most likely to be projected in front of a room. */
  .present {
    position: fixed;
    /* Stops ABOVE the bottom bar rather than covering it — both the picker and
       the passage being presented. Covering it would leave a reader
       mid-presentation with no destinations and only a ✕ to find. `--bottomNavH`
       is 0 at desktop widths, where there is no bar, so this is `inset: 0`
       there. */
    top: 0;
    left: 0;
    right: 0;
    /* `max` and not a sum: in portrait `--bottomNavH` is the bar's MEASURED
       height and the bar already carries the inset inside it, so adding the
       inset here would count the home indicator twice. In landscape the bar is
       gone (`--bottomNavH` is 0) and the inset is all there is — and landscape
       is exactly how this screen gets held up to someone. */
    bottom: max(var(--bottomNavH, 0px), var(--safeBottom));
    z-index: 60;
    background: #fcf9f4;
    color: #211f1a;
    display: flex;
    flex-direction: column;
    font-family: "EB Garamond", Georgia, serif;
    /* Present is `position: fixed`, so it escapes the frame's insets and has to
       carry its own: it is the one surface that covers the status bar and both
       edges — without them the ✕ sits under the clock and a verse set in 54px
       type runs under the camera cutout. Bottom is the `max()` above, not
       padding. */
    padding: var(--safeTop) var(--safeRight) 0 var(--safeLeft);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid #d8cba8;
  }
  .close {
    font-size: calc(18px * var(--uiScale, 1));
    color: #211f1a;
  }
  .title {
    font-size: calc(19px * var(--uiScale, 1));
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .sharebtn {
    border: 1.5px solid #7d632c;
    border-radius: 8px;
    padding: 4px 14px;
    font-size: calc(15px * var(--uiScale, 1));
    color: #6b5417;
  }
  .empty {
    margin: auto;
    max-width: 26em;
    text-align: center;
    color: #6c665d;
    font-size: calc(17px * var(--uiScale, 1));
    padding: 24px;
  }
  .picker {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 12px;
    gap: 10px;
  }
  .pick {
    /* `flex: none` for `.entry`'s reason — a button row in a scrollable column. */
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    border: 1.5px solid #d8cba8;
    border-radius: 12px;
    padding: 16px;
    background: #fffdf8;
    box-shadow: 0 1px 4px rgba(60, 45, 10, 0.05);
  }
  .pick:hover {
    border-color: #7d632c;
  }
  /* Theme-aware picker stage (dark mode was jarringly white). */
  .present.picking {
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
  }
  .present.picking .bar {
    border-bottom-color: var(--rule, #d8cba8);
  }
  .present.picking .close {
    color: var(--ink, #211f1a);
  }
  .present.picking .empty {
    color: var(--faded, #6c665d);
  }
  .present.picking .pick {
    border-color: var(--rule, #d8cba8);
    background: var(--popupPaper, #fffdf8);
    box-shadow: none;
  }
  .present.picking .pick:hover {
    border-color: var(--gold, #7d632c);
  }
  .present.picking .pick .n {
    color: var(--faded, #6c665d);
  }
  .pick .name {
    font-size: calc(22px * var(--uiScale, 1));
    font-weight: 600;
  }
  .pick .n {
    color: #6c665d;
    font-size: calc(14px * var(--uiScale, 1));
  }
  .overview {
    /* `flex: 1; min-height: 0` — the list takes what is left and scrolls INSIDE
       itself. Without it the default `flex: 0 1 auto` sized this to its content,
       so the column overflowed its own fixed height and the share sheet above
       drew over the verses. It fit in English by luck: German's longer strings
       ("Scannen, um … auf dem eigenen Telefon zu öffnen") make the sheet taller
       and tipped it over. `min-height: 0` is the half that
       actually matters — a flex item will not shrink below its content without
       it, whatever `flex` says. */
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 18px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .entry {
    /* NEVER SHRINKS. These rows are flex items of a scrollable column, and a
       flex item's `min-height: auto` floor does not hold for a <button> —
       Chromium's button layout reports a one-line minimum — so under a phone
       viewport the rows were shrunk to ~40% of their content and every verse
       painted its tail over the entry below it (maintainer, "the present still
       looks smushed"). Rigid rows make the overview genuinely overflow, which
       is what its `overflow-y: auto` is for. */
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    text-align: start;
  }
  .entry .ref {
    font-weight: 700;
    font-size: calc(15px * var(--uiScale, 1));
    color: #6b5417;
    letter-spacing: 0.06em;
    font-variant: small-caps;
    text-transform: lowercase;
  }
  .entry .body {
    font-size: calc(21px * var(--uiScale, 1));
    line-height: 1.45;
  }
  .focus {
    /* `min-height: 0` + `overflow-y: auto` for the reason `.overview` has them:
       a flex item will not shrink below its content without the first, so a
       long verse at this type size ran off the bottom of the screen with no way
       to reach it. Psalm 119:176 is short; John 3:16 at 54px on a phone in
       landscape is not.

       Centring is done by the AUTO MARGINS below, not `justify-content: center`
       — and not `safe center` either. Plain `center` centres content taller
       than the box, pushing its first line above the top edge where scrolling
       cannot reach it; `safe center` fixes that but is a recent flexbox keyword
       WebKit shipped late, and an unsupported keyword drops the whole
       declaration — top-aligning every short verse on exactly the iPhones the
       PWA is the install path for. Auto margins are as old as flexbox: they
       absorb the free space when the verse fits (centred) and resolve to zero
       when it does not (top-aligned, scrollable). */
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 6vh 7vw;
    gap: 3vh;
    cursor: pointer;
  }
  .focus .fref {
    margin-top: auto;
    flex-shrink: 0;
  }
  .focus .fbody {
    margin-bottom: auto;
    /* <p>, not <button>, so the `min-height: auto` floor holds today — but this
       pair sits in the same shrink position as `.entry`, so pin it anyway. */
    flex-shrink: 0;
  }
  /* The two sizes in this shell that `--uiScale` deliberately does not touch.
     A passage held up for someone else to read is sized by the SCREEN it is on,
     not by the owner's reading preference, and it is already as large as the
     viewport allows — multiplying a clamp whose ceiling is 54px by 2 would push
     a verse off the sides of the phone it is being shown on. */
  .fref {
    font-size: clamp(18px, 3.2vw, 30px);
    font-weight: 700;
  }
  .fbody {
    font-size: clamp(26px, 5vw, 54px);
    line-height: 1.35;
  }
  .stepbar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 22px;
    padding: 12px;
    border-top: 1px solid #d8cba8;
    font-size: calc(15px * var(--uiScale, 1));
    color: #6c665d;
  }
  .stepbar button {
    font-size: calc(22px * var(--uiScale, 1));
    color: #101010;
    padding: 0 12px;
  }
  .ovbtn {
    font-size: calc(14px * var(--uiScale, 1));
    color: #6c665d;
    text-decoration: underline;
  }
  .endcard {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 24px;
    text-align: center;
  }
  .endcard .mark {
    color: #7d632c;
    font-size: calc(26px * var(--uiScale, 1));
  }
  .endnote {
    color: #6c665d;
    font-style: italic;
  }
  .qr {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  /* The overview's Share reveals the same QR inline, above the passage list. */
  .sharesheet {
    /* Never shrinks and never grows: it is a fixed-size sheet above a list that
       does both (see `.overview`). */
    flex: none;
    padding: 12px 0 4px;
    border-bottom: 1px solid #e0d6bd;
  }
  .linkbtn {
    margin-top: 8px;
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
    color: #6b5417;
    border: 1px solid #7d632c;
    border-radius: 6px;
    padding: 4px 12px;
  }
  .qrnote {
    color: #6c665d;
    font-size: calc(14px * var(--uiScale, 1));
  }
</style>
