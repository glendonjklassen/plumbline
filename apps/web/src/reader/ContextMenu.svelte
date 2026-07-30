<script lang="ts">
  // Verse context menu (Tier-0 #1): Copy · Copy chapter · Share link · Note… ·
  // Tag… / Add to thread… · Memorize · Mark chapter read…. Opened by right-click
  // or long-press; the target verse rides in session state.
  //
  // Trimmed 2026-07-29 on product feedback that it had become noisy. Two things
  // went:
  //
  //   * THREE copy variants collapsed into one "Copy" that honours the reader's
  //     chosen shape (Settings ▸ Copy format), which is what Android always did.
  //     A menu is not the place to re-ask a question the settings already answer.
  //   * The highlight tone swatches and "Remove highlight". Highlighting was then
  //     removed from the product outright — tags, notes and threads are the better
  //     way to annotate and tie scripture together, and three ways to mark a verse
  //     was two too many.
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";
  import { dispatchLink } from "../study/links";
  import { hasChurch, PWA_URL, shareUrl } from "../shell/church";

  const s = getSession();

  const menu = $derived(s.contextMenu);

  function close(): void {
    s.contextMenu = null;
  }

  /** The reader's chosen copy shape (Settings ▸ Copy format). */
  const copyStyle = $derived(s.config.copyStyle ?? "verseRef");

  async function copy(kind: string): Promise<void> {
    const ref = menu!.refKey;
    close();
    const text = await s.rpc.call("copyText", ref, kind);
    if (text) {
      await navigator.clipboard.writeText(text);
      s.showToast("Copied");
    }
  }

  /** Hand someone THIS verse. The `?at=` deep link has existed since 2026-07-27
   *  and, until now, only Present's QR produced one — so a reader looking at a
   *  verse had no way to send it to anybody, which is the most obvious sharing
   *  act the app has.
   *
   *  `shareUrl` builds the link, never string concatenation: the reader's church
   *  rides along (Settings ▸ Your church) exactly as it does from the header and
   *  from Present, and the length clamps in `church.ts` are what keep a shared
   *  URL scannable. The refKey is passed WHOLE — it is the frozen compact form
   *  ("1John 3:16"), `sharedAtRef` shape-checks that form on arrival, and this
   *  file does not need to know where it splits (see 51123f5: three shell sites
   *  hand-rolled that split and all three disagreed with the core's rule).
   *
   *  Phone-first, so the platform share sheet where there is one. Where there
   *  isn't, the clipboard — and it SAYS so, because a share button that appears
   *  to do nothing reads as a broken app. */
  async function shareLink(): Promise<void> {
    const ref = menu!.refKey;
    close();
    const url = shareUrl(PWA_URL, s.church, { at: ref });
    const title = hasChurch(s.church) ? `Plumbline — ${ref}, from ${s.church.name}` : `Plumbline — ${ref}`;
    // Reached with no await before it on purpose: the share sheet is gated on the
    // click's transient user activation, which awaiting anything first can lose.
    if (navigator.share) {
      try {
        await navigator.share({ title, url });
        return;
      } catch (e) {
        // A dismissed sheet is an answer, not a failure — falling through would
        // overwrite the reader's clipboard for a share they just cancelled.
        // Every other rejection still gets the fallback.
        if ((e as DOMException | undefined)?.name === "AbortError") return;
      }
    }
    try {
      await navigator.clipboard.writeText(url);
      s.showToast(`Link copied — it opens at ${ref}`);
    } catch {
      s.showToast("Couldn't share the link — this browser blocked the clipboard.");
    }
  }

  function note(): void {
    const ref = menu!.refKey;
    close();
    void dispatchLink(s, `editnote:${ref}`);
  }

  /** The chapter of the verse under the menu, when it is that chapter's FIRST
   *  verse — the only place "Mark chapter read…" is offered. Kept to verse 1 on
   *  purpose: findable when wanted, too fiddly to do across a whole Bible, which
   *  is exactly the balance the feature asks for. */
  const markable = $derived.by(() => {
    const ref = menu?.refKey;
    if (!ref) return null;
    const m = /^(.+) (\d+):(\d+)$/.exec(ref);
    if (!m || m[3] !== "1") return null;
    return { book: m[1], chapter: Number(m[2]) };
  });

  function markRead(): void {
    const t = markable;
    close();
    if (t) s.markReadFor = t;
  }

  function tagPick(): void {
    const ref = menu!.refKey;
    close();
    s.tagPickFor = ref;
  }

  function addThread(): void {
    const ref = menu!.refKey;
    close();
    void dispatchLink(s, `addthread:${ref}`);
  }

  function memorize(): void {
    const ref = menu!.refKey;
    close();
    void s.author("memoryAdd", ref, nowStamp()).then((err) => s.showToast(err ?? `Memorizing ${ref}`));
  }

  /** A whole section as one card — this verse starts it, the picker ends it. */
  function memorizePassage(): void {
    const ref = menu!.refKey;
    close();
    s.memorizePassageFrom = ref;
  }

  // Clamp the menu into the viewport.
  let el: HTMLDivElement | undefined = $state();
  const pos = $derived.by(() => {
    if (!menu) return { x: 0, y: 0 };
    const w = el?.offsetWidth ?? 230;
    const h = el?.offsetHeight ?? 320;
    return {
      x: Math.min(menu.x, innerWidth - w - 8),
      y: Math.min(menu.y, innerHeight - h - 8),
    };
  });
</script>

{#if menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close} oncontextmenu={(e) => (e.preventDefault(), close())}></div>
  <div class="menu" bind:this={el} style:left="{pos.x}px" style:top="{pos.y}px">
    <div class="ref">{menu.refKey}</div>
    <button onclick={() => copy(copyStyle)}>Copy</button>
    <button onclick={() => copy("chapter")}>Copy chapter</button>
    <button onclick={shareLink}>Share link</button>
    <hr />
    <button onclick={note}>Note…</button>
    <hr />
    <button onclick={tagPick}>Tag…</button>
    <button onclick={addThread}>Add to thread…</button>
    <hr />
    <button onclick={memorize}>Memorize this verse</button>
    <button onclick={memorizePassage}>Memorize passage…</button>
    {#if markable}
      <hr />
      <button onclick={markRead}>Mark chapter read…</button>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 44;
  }
  .menu {
    position: fixed;
    z-index: 45;
    min-width: 210px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 9px;
    box-shadow: 0 10px 36px rgba(0, 0, 0, 0.22);
    padding: 6px;
    display: flex;
    flex-direction: column;
  }
  .ref {
    font-size: 12px;
    color: var(--faded, #8a8276);
    padding: 4px 8px 6px;
    font-weight: 600;
  }
  .menu > button {
    text-align: left;
    padding: 5px 9px;
    border-radius: 5px;
    font-size: 14.5px;
  }
  .menu > button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  hr {
    border: none;
    border-top: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 70%, transparent);
    margin: 4px 6px;
  }
</style>
