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
    const t = threads.find((x: any) => x.name === s.presentThreadName);
    if (t) {
      thread = t;
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
    // The same link the header's Share gives — Present used to hand over a
    // bare app URL, dropping the church exactly where it matters most, since
    // this is the screen you show someone face to face (feedback 2026-07-27).
    lines.push(`Shared from Plumbline — ${s.presentShareLink}`);
    if (hasChurch(s.church)) {
      lines.push("");
      lines.push(s.church.info ? `${s.church.name} — ${s.church.info}` : s.church.name);
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
      } catch {
        /* fall through to clipboard */
      }
    }
    await navigator.clipboard.writeText(url);
    s.showToast("Link copied");
  }

  // Sharing a passage is a QR, not the phone's share sheet. Present is the
  // screen you hold up to someone standing in front of you: a share sheet sends
  // a wall of text to an app they then have to leave, while a QR they scan puts
  // the passage on THEIR phone, in the reader, at the verse (feedback
  // 2026-07-27). `shareText()` stays for the copy fallback — handy when the
  // person isn't in front of you.
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
    s.showToast("Copied to clipboard");
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
       presentation itself stays fixed-light sunlight (feedback 2026-07-26). -->
  <div class="present" class:picking={!thread}>
    {#if !thread}
      <div class="bar">
        <button class="close" onclick={close} aria-label="Close">✕</button>
        <span class="title">Present</span>
      </div>
      {#if threads.length === 0}
        <p class="empty">
          No threads yet — build one from a verse's “＋ add to thread”, then present it here.
        </p>
      {:else}
        <div class="picker">
          {#each threads as t (t.name)}
            <button
              class="pick"
              onclick={() => {
                thread = t;
                focus = null;
              }}
            >
              <span class="name">{t.name}</span>
              <span class="n">{t.entries?.length ?? 0} passages</span>
            </button>
          {/each}
        </div>
      {/if}
    {:else if focus === null}
      <div class="bar">
        <button class="close" onclick={() => (thread = null)} aria-label="Back">‹</button>
        <span class="title">{thread.name}</span>
        <span class="spacer"></span>
        <button class="sharebtn" onclick={() => (showQr = !showQr)}>
          {showQr ? "Hide QR" : "Share"}
        </button>
      </div>
      {#if showQr}
        <div class="qr sharesheet">
          <QrCode size={148} text={passageLink} />
          <span class="qrnote">scan to open {entries[0]?.display ?? thread.name} on their phone</span>
          <button class="linkbtn" onclick={copyPassages}>Copy the passages</button>
        </div>
      {/if}
      <div class="overview">
        {#each entries as e, i (e.ref)}
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
        <button class="ovbtn" onclick={(e) => (e.stopPropagation(), (focus = null))}>overview</button>
        <span>{focus + 1} / {entries.length}</span>
        <button onclick={(e) => (e.stopPropagation(), (focus = Math.min(focus! + 1, entries.length)))}>›</button>
      </div>
    {:else}
      <div class="endcard">
        <p class="mark" aria-hidden="true">✦</p>
        <p class="fref">{thread.name}</p>
        <p class="endnote">— the whole thread, yours to keep —</p>
        <!-- ONE QR, and it carries the passage: the end card's job is to hand
             this thread over, so the code opens the app AT its first verse
             rather than at whatever the recipient last read (2026-07-27). The
             app-link button stays for the person who isn't in front of you —
             and is the only way to grab either link for a test. -->
        <div class="qr">
          <QrCode size={148} text={passageLink} />
          <span class="qrnote">
            scan for {entries[0]?.display ?? thread.name} — free, offline, no account
          </span>
          <button class="linkbtn" onclick={copyPassages}>Copy the passages</button>
          <button class="linkbtn" onclick={shareAppLink}>Copy the app link</button>
        </div>
        <button class="ovbtn" onclick={() => (focus = null)}>back to overview</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Sunlight palette — deliberately fixed light, maximum contrast. Because it is
     fixed, every value here is a literal rather than a `var(--…)`, and the
     literals are the light palette's (crates/core/src/theme.rs). They were the
     OLD light palette, and measured against this paper (#fcf9f4) the muted ones
     failed WCAG AA: #8a8276 was 3.61:1 and .linkbtn's #d8cba8 was 1.53:1 —
     effectively invisible, on the one screen most likely to be projected in front
     of a room (2026-07-29). Restated at 5.4:1 and 6.9:1. */
  .present {
    position: fixed;
    /* Stops ABOVE the bottom bar rather than covering it — both the picker and
       the passage being presented. Present was the one surface that took the
       whole screen, which left a reader mid-presentation with no destinations
       and only a ✕ to find (feedback 2026-07-28). `--bottomNavH` is 0 at desktop
       widths, where there is no bar, so this is `inset: 0` there. */
    top: 0;
    left: 0;
    right: 0;
    bottom: var(--bottomNavH, 0px);
    z-index: 60;
    background: #fcf9f4;
    color: #211f1a;
    display: flex;
    flex-direction: column;
    font-family: "EB Garamond", Georgia, serif;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid #d8cba8;
  }
  .close {
    font-size: 18px;
    color: #211f1a;
  }
  .title {
    font-size: 19px;
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .sharebtn {
    border: 1.5px solid #7d632c;
    border-radius: 8px;
    padding: 4px 14px;
    font-size: 15px;
    color: #6b5417;
  }
  .empty {
    margin: auto;
    max-width: 26em;
    text-align: center;
    color: #6c665d;
    font-size: 17px;
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
    font-size: 22px;
    font-weight: 600;
  }
  .pick .n {
    color: #6c665d;
    font-size: 14px;
  }
  .overview {
    overflow-y: auto;
    padding: 14px 18px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .entry {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    text-align: left;
  }
  .entry .ref {
    font-weight: 700;
    font-size: 15px;
    color: #6b5417;
    letter-spacing: 0.06em;
    font-variant: small-caps;
    text-transform: lowercase;
  }
  .entry .body {
    font-size: 21px;
    line-height: 1.45;
  }
  .focus {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 6vh 7vw;
    gap: 3vh;
    cursor: pointer;
  }
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
    font-size: 15px;
    color: #6c665d;
  }
  .stepbar button {
    font-size: 22px;
    color: #101010;
    padding: 0 12px;
  }
  .ovbtn {
    font-size: 14px;
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
    font-size: 26px;
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
    padding: 12px 0 4px;
    border-bottom: 1px solid #e0d6bd;
  }
  .linkbtn {
    margin-top: 8px;
    font-size: 13px;
    font-weight: 600;
    color: #6b5417;
    border: 1px solid #7d632c;
    border-radius: 6px;
    padding: 4px 12px;
  }
  .qrnote {
    color: #6c665d;
    font-size: 14px;
  }
</style>
