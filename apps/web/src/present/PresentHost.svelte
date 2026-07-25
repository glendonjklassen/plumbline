<script lang="ts">
  // Present mode (Android parity, the top-priority request): a thread as a
  // fullscreen, high-contrast ("sunlight") large-type presentation for showing
  // someone in person. Picker → scrollable overview → tap-to-focus huge → end
  // card with plain-text share + BibleGateway link. Deliberately hard-coded
  // light — the phone/laptop gets handed across in daylight.
  import { getSession } from "../state/session.svelte";

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
    lines.push("Read online: https://www.biblegateway.com/passage/?version=KJV&search=" +
      encodeURIComponent(entries.map((e) => e.display).join("; ")));
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
  <div class="present">
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
        <p class="fref">{thread.name}</p>
        <p class="endnote">— the whole thread, yours to keep —</p>
        <button class="sharebig" onclick={share}>Share the passages</button>
        <a
          class="bglink"
          href={"https://www.biblegateway.com/passage/?version=KJV&search=" +
            encodeURIComponent(entries.map((e) => e.display).join("; "))}
          target="_blank"
          rel="noopener">Read online (BibleGateway KJV)</a
        >
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
    background: #ffffff;
    color: #101010;
    display: flex;
    flex-direction: column;
    font-family: "EB Garamond", Georgia, serif;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid #d9d2c2;
  }
  .close {
    font-size: 18px;
    color: #101010;
  }
  .title {
    font-size: 19px;
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .sharebtn {
    border: 1.5px solid #101010;
    border-radius: 8px;
    padding: 4px 14px;
    font-size: 15px;
  }
  .empty {
    margin: auto;
    max-width: 26em;
    text-align: center;
    color: #6a6a6a;
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
    border: 1.5px solid #d9d2c2;
    border-radius: 12px;
    padding: 16px;
  }
  .pick:hover {
    border-color: #101010;
  }
  .pick .name {
    font-size: 22px;
    font-weight: 600;
  }
  .pick .n {
    color: #6a6a6a;
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
    font-size: 17px;
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
    border-top: 1px solid #d9d2c2;
    font-size: 15px;
    color: #6a6a6a;
  }
  .stepbar button {
    font-size: 22px;
    color: #101010;
    padding: 0 12px;
  }
  .ovbtn {
    font-size: 14px;
    color: #6a6a6a;
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
  .endnote {
    color: #6a6a6a;
    font-style: italic;
  }
  .sharebig {
    border: 2px solid #101010;
    border-radius: 12px;
    padding: 12px 28px;
    font-size: 20px;
  }
  .bglink {
    color: #2a4d8f;
    font-size: 16px;
  }
</style>
