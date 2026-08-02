<script lang="ts">
  // Set the date a chapter was last read — the by-hand entry for reading done in
  // a paper Bible. The Android twin is MarkReadDialog in ui/VerseActions.kt.
  //
  // A native <input type="date"> rather than a hand-built calendar: it is the
  // control every phone and desktop already knows how to show, it is keyboard
  // and screen-reader accessible for free, and `max` closes off the future
  // (nobody read anything tomorrow). Shortcuts cover the answers people actually
  // give — "today", "yesterday", "last week" — so the common case is one tap.
  //
  // Clearing lives here too, because this dialog is the only way back out of a
  // date set by mistake.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";

  const s = getSession();

  const target = $derived(s.markReadFor);
  const today = new Date().toISOString().slice(0, 10);
  let date = $state(today);

  $effect(() => {
    if (target) date = today;
  });

  const label = $derived(target ? `${s.bookName(target.book)} ${target.chapter}` : "");

  function close(): void {
    s.markReadFor = null;
  }

  function daysAgo(n: number): string {
    const d = new Date();
    d.setUTCDate(d.getUTCDate() - n);
    return d.toISOString().slice(0, 10);
  }

  async function set(when: string): Promise<void> {
    const t = target;
    close();
    if (!t) return;
    const err = await s.author("readingMarkRead", t.book, t.chapter, when);
    s.showToast(err ?? `Marked read — ${when}`);
  }

  async function clear(): Promise<void> {
    const t = target;
    if (!t) return;
    const ok = await s.askConfirm(
      `Clear the reading history for ${label}?`,
      "It goes back to unread — including a date you set here by hand, which nothing else records.",
      "Clear history",
    );
    if (!ok) return;
    close();
    const err = await s.author("readingForget", t.book, t.chapter);
    s.showToast(err ?? "Reading history cleared");
  }
</script>

{#if target}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Mark chapter read"
    data-surface="mark read"
    use:modal={{ close }}
  >
    <h2>When did you last read {label}?</h2>
    <p class="sub">
      For reading you did somewhere else — a paper Bible, or another app. It counts as a full read
      on the date you give.
    </p>
    <div class="quick">
      <button onclick={() => set(daysAgo(0))}>Today</button>
      <button onclick={() => set(daysAgo(1))}>Yesterday</button>
      <button onclick={() => set(daysAgo(7))}>Last week</button>
    </div>
    <label class="pick">
      <span>Or pick a date</span>
      <input type="date" bind:value={date} max={today} />
    </label>
    <div class="row">
      <button class="clear" onclick={clear}>Clear history</button>
      <span class="spacer"></span>
      <button onclick={close}>Cancel</button>
      <button class="primary" onclick={() => set(date)}>Set</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 46;
  }
  .dialog {
    position: fixed;
    z-index: 47;
    top: 14vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(420px, 94vw);
    /* Auto height, but never past the destination bar — the Set/Clear/Cancel row
       lives at the bottom and has to stay tappable. */
    max-height: calc(86vh - var(--bottomNavH, 0px));
    overflow-y: auto;
    padding: 18px 20px 14px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 14px 56px rgba(0, 0, 0, 0.3);
  }
  h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--ink, #211f1a);
    margin: 0 0 6px;
  }
  .sub {
    font-size: 14px;
    line-height: 1.45;
    color: var(--faded, #8a8276);
    margin: 0 0 14px;
  }
  .quick {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .quick button {
    padding: 8px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 999px;
    background: var(--paper, #fcf9f4);
    color: var(--gold, #9e7d38);
    font-size: 14px;
  }
  .quick button:hover {
    border-color: var(--gold, #9e7d38);
  }
  .pick {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 14px;
    color: var(--ink, #211f1a);
    padding-bottom: 14px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .pick input {
    padding: 7px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: 14px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 12px;
  }
  .spacer {
    flex: 1;
  }
  .row button {
    padding: 8px 14px;
    border-radius: 7px;
    font-size: 14px;
    color: var(--faded, #8a8276);
  }
  .row .clear {
    color: var(--tierResearch, #b04a3a);
  }
  .row .primary {
    border: 1px solid var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
    font-weight: 600;
  }
</style>
