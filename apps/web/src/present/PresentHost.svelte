<script lang="ts">
  // Present mode (Android parity, the top-priority request): a thread as a
  // fullscreen, high-contrast ("sunlight") large-type presentation for showing
  // someone in person. Picker → scrollable overview → tap-to-focus huge → end
  // card with plain-text share + a scannable QR of the hosted PWA.
  // Deliberately hard-coded light — the phone/laptop gets handed across in
  // daylight.
  import { getSession } from "../state/session.svelte";
  import QrCode, { PWA_URL } from "../shell/QrCode.svelte";

  const s = getSession();

  interface Entry {
    ref: string;
    display: string;
    body: string;
  }

  const threads = $derived.by(() => {
    void s.studyEpoch;
    return s.showPresent ? ((s.engine.threads()?.threads ?? []) as any[]) : [];
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
      const v = s.engine.verse(e.verse);
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
    lines.push(`Shared from Plumbline — ${PWA_URL}`);
    return lines.join("\n");
  }
  async function share(): Promise<void> {
    const text = shareText();
    if (navigator.share) {
      try {
        await navigator.share({ title: thread.name, text });
        return;
      } catch {
        /* fall through to clipboard */
      }
    }
    await navigator.clipboard.writeText(text);
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
        <button class="sharebtn" onclick={share}>Share</button>
      </div>
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
        <button class="sharebig" onclick={share}>Share the passages</button>
        <div class="qr">
          <QrCode size={148} />
          <span class="qrnote">scan for the app — free, offline, no account</span>
        </div>
        <button class="ovbtn" onclick={() => (focus = null)}>back to overview</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Sunlight palette — deliberately fixed light, maximum contrast. */
  .present {
    position: fixed;
    inset: 0;
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
    border: 1.5px solid #9e7d38;
    border-radius: 8px;
    padding: 4px 14px;
    font-size: 15px;
    color: #6b5417;
  }
  .empty {
    margin: auto;
    max-width: 26em;
    text-align: center;
    color: #8a8276;
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
    border-color: #9e7d38;
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
    color: var(--faded, #8a8276);
  }
  .present.picking .pick {
    border-color: var(--rule, #d8cba8);
    background: var(--popupPaper, #fffdf8);
    box-shadow: none;
  }
  .present.picking .pick:hover {
    border-color: var(--gold, #9e7d38);
  }
  .present.picking .pick .n {
    color: var(--faded, #8a8276);
  }
  .pick .name {
    font-size: 22px;
    font-weight: 600;
  }
  .pick .n {
    color: #8a8276;
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
    color: #8a8276;
  }
  .stepbar button {
    font-size: 22px;
    color: #101010;
    padding: 0 12px;
  }
  .ovbtn {
    font-size: 14px;
    color: #8a8276;
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
    color: #9e7d38;
    font-size: 26px;
  }
  .endnote {
    color: #8a8276;
    font-style: italic;
  }
  .sharebig {
    border: 2px solid #9e7d38;
    border-radius: 12px;
    padding: 12px 28px;
    font-size: 20px;
  }
  .qr {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .qrnote {
    color: #8a8276;
    font-size: 14px;
  }
</style>
