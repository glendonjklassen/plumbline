<script lang="ts">
  // Verse context menu (Tier-0 #1): Copy · Copy chapter · Share link · Note… ·
  // Tag… / Add to thread… · Memorize · Mark chapter read…. Opened by right-click
  // or long-press; the target verse rides in session state.
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";
  import { dispatchLink } from "../study/links";
  import { refDisplay } from "./refname";
  import { hasChurch, PWA_URL, shareUrl } from "../shell/church";
  import { lang, t } from "../lib/i18n.svelte";
  import { ttsSpeak, ttsSupported } from "./tts.svelte";

  const s = getSession();

  const menu = $derived(s.contextMenu);

  /** The verse as a reader says it ("Isaiah 53:5"), for the menu's own heading
   *  and for every sentence this menu speaks. `menu.refKey` stays the OSIS form
   *  wherever it is SENT — the engine calls and the `?at=` link below. */
  const shown = $derived(menu ? refDisplay(s, menu.refKey) : "");

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
      s.showToast(t("menu.copied"));
    }
  }

  /** Hand someone THIS verse.
   *
   *  `shareUrl` builds the link, never string concatenation: the reader's church
   *  rides along (Settings ▸ Your church) exactly as it does from the header and
   *  from Present, and the length clamps in `church.ts` are what keep a shared
   *  URL scannable. The refKey is passed WHOLE — it is the frozen compact form
   *  ("1John 3:16"), `sharedAtRef` shape-checks that form on arrival, and this
   *  file does not need to know where it splits.
   *
   *  Phone-first, so the platform share sheet where there is one. Where there
   *  isn't, the clipboard — and it SAYS so, because a share button that appears
   *  to do nothing reads as a broken app. */
  async function shareLink(): Promise<void> {
    const ref = menu!.refKey;
    // Read the display form BEFORE close(): `shown` derives from
    // `s.contextMenu`, which close() nulls, and a stale $derived recomputes the
    // moment it is read again (the bug PassagePicker.commit documents). What the
    // recipient READS is the book's name; what travels in `?at=` is `ref`.
    const said = shown;
    close();
    const url = shareUrl(PWA_URL, s.church, { at: ref });
    const title = hasChurch(s.church)
      ? t("menu.shareTitleChurch", { passage: said, church: s.church.name })
      : t("menu.shareTitle", { passage: said });
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
      s.showToast(t("menu.linkCopied", { passage: said }));
    } catch {
      s.showToast(t("menu.shareBlocked"));
    }
  }

  function note(): void {
    const ref = menu!.refKey;
    close();
    void dispatchLink(s, `editnote:${ref}`);
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
    const said = shown;
    close();
    void s.author("memoryAdd", ref, nowStamp()).then((err) => s.showToast(err ?? t("menu.memorizing", { passage: said })));
  }

  /** A whole section as one card — this verse starts it, the picker ends it. */
  function memorizePassage(): void {
    const ref = menu!.refKey;
    close();
    s.memorizePassageFrom = ref;
  }

  /** Read aloud (Web Speech API): the whole chapter, or from this verse to the
   *  chapter's end. Texts are fetched per verse from THIS PANE's own language
   *  handle (`callIn`), so a German pane is read in German — the voice must
   *  match the words, not the UI. */
  async function readAloud(wholeChapter: boolean): Promise<void> {
    const ref = menu!.refKey;
    const said = wholeChapter ? shown.slice(0, shown.lastIndexOf(":")) : shown;
    close();
    const colon = ref.lastIndexOf(":");
    const space = ref.lastIndexOf(" ", colon);
    const book = ref.slice(0, space);
    const chapter = Number(ref.slice(space + 1, colon));
    const from = wholeChapter ? 1 : Number(ref.slice(colon + 1));
    const paneLang = s.panes[s.activePane]?.lang ?? "";
    const count = Number(await s.rpc.callIn(paneLang, "chapterVerseCount", book, chapter));
    const bodies: string[] = [];
    for (let v = from; v <= count; v++) {
      const verse = await s.rpc.callIn(paneLang, "verse", `${book} ${chapter}:${v}`);
      if (verse?.body) bodies.push(verse.body);
    }
    ttsSpeak(said, bodies, paneLang || lang());
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
    <div class="ref">{shown}</div>
    <button onclick={() => copy(copyStyle)}>{t("menu.copy")}</button>
    <button onclick={() => copy("chapter")}>{t("menu.copyChapter")}</button>
    <button onclick={shareLink}>{t("menu.shareLink")}</button>
    <hr />
    <button onclick={note}>{t("menu.note")}</button>
    <hr />
    <button onclick={tagPick}>{t("menu.tag")}</button>
    <button onclick={addThread}>{t("menu.addThread")}</button>
    <hr />
    <button onclick={memorize}>{t("menu.memorizeVerse")}</button>
    <button onclick={memorizePassage}>{t("menu.memorizePassage")}</button>
    {#if ttsSupported()}
      <hr />
      <button onclick={() => void readAloud(false)}>{t("menu.readFromHere")}</button>
      <button onclick={() => void readAloud(true)}>{t("menu.readChapter")}</button>
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
    /* Nine rows at the 44px tap floor (app.css) plus separators is taller than a
       phone held sideways, and `pos` places this menu from its MEASURED height —
       so a menu taller than the viewport is pushed off the TOP, losing Copy and
       Note… rather than the last row. Capped, it keeps its first row on screen
       and scrolls the rest, as the ≡ menu already does. */
    max-height: calc(100dvh - 16px);
    overflow-y: auto;
  }
  .ref {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    padding: 4px 8px 6px;
    font-weight: 600;
  }
  .menu > button {
    text-align: left;
    padding: 5px 9px;
    border-radius: 5px;
    font-size: calc(14.5px * var(--uiScale, 1));
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
