<script lang="ts">
  // The app frame: header (browse buttons · live search · ≡ menu), pane row,
  // study surface, and global keyboard bindings.
  import ReaderPane from "../reader/ReaderPane.svelte";
  import StudyPanel from "../study/StudyPanel.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import Shortcuts from "./Shortcuts.svelte";
  import BookNav from "./BookNav.svelte";
  import ExploreScreen from "./ExploreScreen.svelte";
  import PlansScreen from "./PlansScreen.svelte";
  import DevotionalScreen from "./DevotionalScreen.svelte";
  import VizScreen from "./VizScreen.svelte";
  import TagsScreen from "./TagsScreen.svelte";
  import WeavesScreen from "./WeavesScreen.svelte";
  import PreachScreen from "./PreachScreen.svelte";
  import HymnalScreen from "../hymnal/HymnalScreen.svelte";
  import MarkReadDialog from "./MarkReadDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import PickDialog from "./PickDialog.svelte";
  import CanonStrip from "./CanonStrip.svelte";
  import PlanChip from "./PlanChip.svelte";
  import { fade } from "svelte/transition";
  import HistorySheet from "./HistorySheet.svelte";
  import SettingsDialog from "./SettingsDialog.svelte";
  import MemorizeHost from "../memorize/MemorizeHost.svelte";
  import PassagePicker from "../memorize/PassagePicker.svelte";
  import PresentHost from "../present/PresentHost.svelte";
  import ConnectorsOverlay from "./ConnectorsOverlay.svelte";
  import MapsHost from "../maps/MapsHost.svelte";
  import ContextMenu from "../reader/ContextMenu.svelte";
  import { ttsSpeaking, ttsStop } from "../reader/tts.svelte";
  import TagPicker from "../study/TagPicker.svelte";
  import ThreadPicker from "../study/ThreadPicker.svelte";
  import TagWeave from "../study/TagWeave.svelte";
  import ShareScreen from "./ShareScreen.svelte";
  import SearchScreen from "./SearchScreen.svelte";
  import { hasNativeIntros, t } from "../lib/i18n.svelte";
  import { uiScale } from "../lib/uiScale";
  import { DEFAULT_FONT, FONT_SCALE } from "../engine/fonts.generated";
  import { getSession } from "../state/session.svelte";
  import { startReadingTracker } from "../state/readingTracker";

  const s = getSession();

  // Reading time for the navigator's map (core::reading). The FIRST pane only —
  // a second pane is a parallel reference being consulted — and nothing while a
  // modal surface is up: Present, the memorize drill and the maps are not reading.
  $effect(() =>
    startReadingTracker({
      target: () => {
        // Concept-study skimming is not reading either: the mode suspends the
        // tracker so a sweep credits no dwell to the map or the plans over it.
        if (s.showPresent || s.screen !== "read" || s.mapPopup || s.inConceptStudy) return null;
        const p = s.panes[0];
        return p ? { book: p.book, chapter: p.chapter } : null;
      },
      reached: () => s.panes[0]?.reached ?? 0,
      // One sample per second into the core's tracker, which owns grace, idle,
      // cadence and tail. The thresholds never leave the core and it banks its
      // own reports, so this end has nothing to fetch and nothing to record.
      tick: (book, chapter, reached, step, interacted) =>
        s.rpc
          .call("readingTick", book, chapter, reached, step, interacted, new Date().toISOString())
          .catch(() => null),
      // No completion toast: the read lands in the map, the plans and the streak
      // silently. Finishing a chapter is not something to congratulate.
    }),
  );

  const subtitle = $derived.by(() => {
    const p = s.panes[s.activePane];
    return p ? `${s.bookName(p.book)} ${p.chapter}` : "";
  });
  // The phone bar's passage in two halves, so the book NAME ellipsizes while the
  // chapter number never leaves the screen. See `.pbook` / `.pchap` below.
  const passageBook = $derived.by(() => {
    const p = s.panes[s.activePane];
    return p ? s.bookName(p.book) : "";
  });
  const passageChapter = $derived(s.panes[s.activePane]?.chapter ?? "");

  // The sweep's progress, on the banner: the map's glow is frozen in the mode
  // (the tracker is suspended), so this is its only visible progress. Stays live
  // because every sweep is an authoring write, which invalidates the plans read.
  const sweepProgress = $derived.by(() => {
    if (!s.inConceptStudy) return null;
    const run = ((s.q("plans", "")?.running ?? []) as any[]).find((p) => p.id === s.conceptStudyId);
    return (run?.sweepProgress as [number, number] | undefined) ?? null;
  });

  function openWordStudy(refKey: string, tokenIndex: number, lang?: string): void {
    // The pane's own text: a word tapped in the German column is studied in
    // German, with that language's own dictionary. The engine resolves the word
    // from the token, so the tap needs no extra round trip.
    s.panel = { kind: "wordUsage", word: "", refKey, tokenIndex, scope: "all", page: 0, lang };
  }

  // Search is its own screen; the bar carries only the way in. The scope starts
  // fresh at "everywhere" each time — a chip left on from the last question is a
  // search that lies about what it looked at.
  function openSearch(): void {
    s.searchScope = "all";
    s.screen = "search";
  }
  // Surfaces are exclusive: picking a destination closes the others. Shared by
  // the header's destination buttons and the ≡ utilities. `dismissTransient`
  // owns the closing list, so a transient surface added later is escaped too.
  function go(action: () => void): () => void {
    return () => {
      // `dismissTransient` closes the ≡ menu too (it is in the TRANSIENT table).
      s.dismissTransient();
      action();
    };
  }

  // The bottom bar's five ROLES — Read · Study · Preach · Share · Sing: the hats
  // a reader wears, not a list of features. The tools live one layer down
  // (Memorize is a card inside Study, the church rides Share). Icons are standard
  // Material Symbols.
  //
  // Ids, not labels: this array is built ONCE, so a label would freeze whatever
  // language the app booted in. `t()` below keeps the nav live across a switch.
  const NAV = [
    {
      key: "read",
      path:
        "M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" +
        "M6 4h5v8l-2.5-1.5L6 12V4z",
      // Read opens nothing: `dismissTransient` has already cleared every layer.
      go: () => {},
    },
    {
      key: "study",
      path: "M5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82zM12 3L1 9l11 6 11-6-11-6z",
      go: () => (s.screen = "explore"),
    },
    {
      key: "preach",
      path:
        "M21 3H3c-1.11 0-2 .89-2 2v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2z" +
        "m0 16.02H3V4.98h18v14.04zM10 12H8l4-4 4 4h-2v4h-4v-4z",
      // A hub like Study, not a raise of Present: the role holds the presentation
      // AND its materials (weaves, tags, notes).
      go: () => (s.screen = "preach"),
    },
    {
      key: "share",
      path:
        "M18 16.08c-.76 0-1.44.3-1.96.77L8.91 12.7c.05-.23.09-.46.09-.7s-.04-.47-.09-.7l7.05-4.11" +
        "c.54.5 1.25.81 2.04.81 1.66 0 3-1.34 3-3s-1.34-3-3-3-3 1.34-3 3c0 .24.04.47.09.7L8.04 9.81" +
        "C7.5 9.31 6.79 9 6 9c-1.66 0-3 1.34-3 3s1.34 3 3 3c.79 0 1.5-.31 2.04-.81l7.12 4.16" +
        "c-.05.21-.08.43-.08.65 0 1.61 1.31 2.92 2.92 2.92s2.92-1.31 2.92-2.92-1.31-2.92-2.92-2.92z",
      go: () => (s.screen = "share"),
    },
    {
      key: "sing",
      path: "M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z",
      go: () => (s.screen = "hymnal"),
    },
  ] as const;

  // How much room the bottom bar takes, published as `--bottomNavH` so a
  // full-screen surface can stop above it instead of underlapping it. MEASURED,
  // never restated as `calc(52px + safe-area)`: the height is a button
  // min-height plus padding plus a border, and two declarations of one length
  // drift. Zero when the bar is `display: none` (desktop widths).
  let navEl = $state<HTMLElement | null>(null);
  /** Read once: the toast's fade is off for a reader who has asked for less
   *  motion (ExploreScreen does the same for its settle). */
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  $effect(() => {
    const el = navEl;
    if (!el) return;
    const publish = () =>
      document.documentElement.style.setProperty("--bottomNavH", `${el.offsetHeight}px`);
    publish();
    const ro = new ResizeObserver(publish);
    ro.observe(el);
    return () => {
      ro.disconnect();
      document.documentElement.style.removeProperty("--bottomNavH");
    };
  });

  // Which ROLE reads as current. Preach wins because Present covers everything;
  // below it the screen decides, and Memorize lights Study (it is a card there).
  const dest = $derived(
    s.showPresent || s.screen === "preach"
      ? "preach"
      : s.screen === "explore" ||
          s.screen === "memorize" ||
          s.screen === "plans" ||
          s.screen === "devotional" ||
          s.screen === "viz" ||
          s.screen === "tags" ||
          s.screen === "weaves"
        ? "study"
        : s.screen === "hymnal"
          ? "sing"
          : s.screen === "share"
            ? "share"
            : "read",
  );

  // The reader's text size over the 18px the chrome was drawn at, times the
  // chrome face's optical scale (faces differ in x-height, so switching one must
  // change the chrome's voice, not its apparent size). Composed here so
  // `--uiScale` stays the one number the whole chrome multiplies by; the probe
  // below measures the browser's own font preference. See lib/uiScale.ts.
  const readerScale = $derived(
    (Number(s.config.bodySize ?? 20) / 18) * (FONT_SCALE[s.config.chromeFont ?? DEFAULT_FONT] ?? 1),
  );

  // ── global keys ──
  function isEditable(t: EventTarget | null): boolean {
    return (
      t instanceof HTMLInputElement ||
      t instanceof HTMLTextAreaElement ||
      t instanceof HTMLSelectElement ||
      (t instanceof HTMLElement && t.isContentEditable)
    );
  }
  function onKeydown(e: KeyboardEvent): void {
    if (isEditable(e.target) || s.promptReq) return;
    // An open map popup owns the arrow keys (constellation paging).
    if (s.mapPopup && e.key !== "Escape") return;
    const pane = s.panes[s.activePane];
    if (!pane) return;
    const fontPx = Number(s.config.bodySize ?? 20);
    const line = fontPx * 3;
    const page = 0.85 * (innerHeight - 120);
    const scroll = (dy: number, all = false) => {
      for (const p of all ? s.panes : [pane]) {
        p.scrollY = Math.max(0, p.scrollY + dy);
        p.pendingScroll = false;
      }
      // Keyboard scrolling follows the ⛓ chain like touch does; shift already
      // scrolls everything, chained or not.
      if (!all) s.syncLinkedScroll(s.activePane);
    };
    switch (e.key) {
      case "ArrowUp":
        scroll(-line);
        break;
      case "ArrowDown":
        scroll(line);
        break;
      case "PageUp":
        scroll(-page, e.shiftKey);
        break;
      case "PageDown":
      case " ":
        scroll(page, e.shiftKey);
        break;
      case "Home":
        pane.scrollY = 0;
        pane.pendingScroll = false;
        break;
      case "End":
        pane.scrollY = Number.MAX_SAFE_INTEGER; // pane clamps on next frame
        pane.pendingScroll = false;
        break;
      case "ArrowRight":
        if (e.altKey) s.historyStep(s.activePane, 1);
        else s.stepChapter(s.activePane, 1);
        break;
      case "]":
        s.stepChapter(s.activePane, 1);
        break;
      case "ArrowLeft":
        if (e.altKey) s.historyStep(s.activePane, -1);
        else s.stepChapter(s.activePane, -1);
        break;
      case "[":
        s.stepChapter(s.activePane, -1);
        break;
      case "0":
        if (e.ctrlKey) s.setZoom(18);
        else return;
        break;
      case "+":
      case "=":
        if (e.ctrlKey) s.setZoom(fontPx + 1);
        else return;
        break;
      case "-":
        if (e.ctrlKey) s.setZoom(fontPx - 1);
        else return;
        break;
      // Escape peels ONE layer, outermost first. The ladder lives on the session
      // (`popOneLayer`) because the phone's Back button must climb the same one.
      // Dialogs answer first: every `aria-modal` surface carries `use:modal`,
      // which takes focus and stops Escape at the dialog, so this is only reached
      // when focus is outside them — and the early return above drops any key
      // that came from a field.
      case "Escape":
        s.popOneLayer();
        break;
      case "?":
      case "F1":
        s.showShortcuts = true;
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="frame" data-screen={dest}>
  <!-- One rem wide: the browser's own default text size, as a box a
       ResizeObserver can watch. lib/uiScale.ts turns it and the reader's setting
       into `--uiScale`. -->
  <div class="rem-probe" aria-hidden="true" use:uiScale={readerScale}></div>
  <header>
    <!-- No app title, at any width: on a phone it is the widest thing in the bar
         and pushes the destinations into a second row. The tab title and the
         install manifest carry the name.

         On a PHONE the chapter nav lives here — one bar, not a header above a
         pane strip. A phone is capped at one pane (`maxPanes`), so this is that
         pane, and the pane's own strip is hidden at this width. -->
    <div class="chapter-nav">
      <button onclick={() => s.stepChapter(s.activePane, -1)} aria-label={t("common.previousChapter")}>‹</button>
      <button class="passage" onclick={() => (s.bookNavFor = s.activePane)}>
        <span class="pbook">{passageBook}</span>
        <span class="pchap">{passageChapter} ▾</span>
      </button>
      <button onclick={() => s.stepChapter(s.activePane, 1)} aria-label={t("common.nextChapter")}>›</button>
    </div>
    <!-- Which pane is ACTIVE. Hidden on a phone (the chapter nav says it) and on
         every DESTINATION, where the passage is not what the screen is about.
         Kept in the DOM at every width: the e2e suite uses it as the "text is on
         screen" boot signal. -->
    <span class="subtitle">{subtitle}</span>
    <!-- The roles are first-class in the top bar (Read is the base layer, then
         Study · Preach · Share · Sing); the ≡ menu holds utilities only. Tools
         live one layer down — Threads/Tags/Weaves/Memorize inside Study, the
         church and the QR on Share. -->
    <nav class="browse">
        <button onclick={go(() => (s.screen = "explore"))}>{t("nav.study")}</button>
        <button onclick={go(() => (s.screen = "preach"))}>{t("nav.preach")}</button>
        <button onclick={go(() => (s.screen = "share"))}>{t("nav.share")}</button>
        <button onclick={go(() => (s.screen = "hymnal"))}>{t("nav.sing")}</button>
      </nav>
    <span class="spacer"></span>
    <button class="glass" onclick={openSearch} aria-label={t("common.openSearch")}>⌕</button>
    <button class="menu-btn" onclick={() => (s.menuOpen = !s.menuOpen)} aria-label={t("common.menu")}>≡</button>
  </header>

  <!-- One destination at a time: a screen REPLACES the reader rather than sliding
       over it. The study panel still layers on top — it is about the verse you
       are looking at, not a place you go. -->
  <div class="body">
    {#if s.screen === "explore"}
      <ExploreScreen />
    {:else if s.screen === "memorize"}
      <MemorizeHost />
    {:else if s.screen === "plans"}
      <PlansScreen />
    {:else if s.screen === "devotional"}
      <DevotionalScreen />
    {:else if s.screen === "viz"}
      <VizScreen />
    {:else if s.screen === "tags"}
      <TagsScreen />
    {:else if s.screen === "weaves"}
      <WeavesScreen />
    {:else if s.screen === "preach"}
      <PreachScreen />
    {:else if s.screen === "hymnal"}
      <HymnalScreen />
    {:else if s.screen === "share"}
      <ShareScreen />
    {:else if s.screen === "search"}
      <SearchScreen />
    {:else}
      <div class="reading">
        {#if s.inConceptStudy}
          <!-- Persistent, so the reader always knows a tap tags rather than opens
               study, and can leave with one press. -->
          <div class="concept-study-banner" role="status">
            <span class="tag">{t("conceptStudy.banner", { tag: s.conceptStudyTag ?? "" })}</span>
            {#if sweepProgress}
              <span class="prog">{t("plans.sweepProgress", { done: sweepProgress[0], total: sweepProgress[1] })}</span>
            {/if}
            <button class="exit" onclick={() => s.exitConceptStudy()}>{t("conceptStudy.exit")}</button>
          </div>
        {/if}
        <div class="panes">
          {#each s.panes as _, i (i)}
            <ReaderPane paneIdx={i} onWordStudy={openWordStudy} />
          {/each}
          {#if s.panes.length > 1}
            <!-- A connector goes BETWEEN panes, so one pane has none (and a phone
                 only ever has one). Mounting it there would re-allocate a
                 full-viewport canvas every scroll frame to draw nothing. -->
            <ConnectorsOverlay />
          {/if}
        </div>
        <PlanChip />
        <CanonStrip />
      </div>
    {/if}
    <StudyPanel />
  </div>

  <!-- The bottom bar (narrow only) — the five roles in thumb reach. Read is not a
       destination so much as the absence of one: the reader is always mounted
       underneath, so its tap just clears whatever is layered over it. -->
  <nav class="bottom-nav" aria-label={t("nav.destinations")} bind:this={navEl}>
    {#each NAV as item (item.key)}
      <button
        class:on={dest === item.key}
        aria-current={dest === item.key ? "page" : undefined}
        onclick={go(item.go)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d={item.path} /></svg>
        <span>{t(`nav.${item.key}`)}</span>
      </button>
    {/each}
  </nav>
</div>

{#if s.menuOpen}
  <!-- The ≡ utilities, raised from the header OR any destination's ScreenBar,
       fixed to the top-right so it works over every screen. Utilities only — the
       roles live in the bottom bar / browse row. Welcome is here for every
       reader: an established believer never had `intro` set, so it falls back to
       the new-believer welcome rather than disappearing. -->
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.menuOpen = false)}></div>
  <div class="menu" role="menu" aria-label={t("common.menu")}>
    <!-- Only where the welcome exists in this language, or a German menu reopens
         English paragraphs. See i18n::Lang::has_native_intros. -->
    {#if hasNativeIntros()}
      <button onclick={go(() => (s.reopenIntro = s.intro ?? "new"))}>{t("shell.welcome")}</button>
    {/if}
    <button onclick={go(() => (s.showHistory = true))}>{t("shell.history")}</button>
    <button onclick={go(() => (s.panel = { kind: "guide" }))}>{t("shell.guideAndAbout")}</button>
    <button onclick={go(() => (s.showShortcuts = true))}>{t("shell.shortcuts")}</button>
    <button onclick={go(() => (s.showSettings = true))}>{t("shell.settings")}</button>
  </div>
{/if}

<!-- Every confirmation this app gives arrives here and nowhere else, so it needs
     `role="status"`: without a live region a screen reader is told none of them.
     Fades unless the reader has asked for less motion. -->
{#if s.toast}
  <div class="toast brief" role="status" transition:fade={{ duration: reduceMotion ? 0 : 150 }}>{s.toast}</div>
{/if}

<!-- Read-aloud is invisible sound: this chip is the only sign it is running and
     the only way to stop it, so it is sticky while it speaks. -->
{#if ttsSpeaking()}
  <div class="toast update" class:stacked={s.updateReady} role="status">
    <span>{t("tts.reading")} · {ttsSpeaking()!.passage}</span>
    <button class="dismiss" aria-label={t("tts.stop")} onclick={ttsStop}>✕</button>
  </div>
{/if}

<!-- A deploy landed while this session was open (an installed PWA can sit for
     weeks). Offered, never taken automatically: no reloading someone mid-verse. -->
{#if s.updateReady}
  <div class="toast update" role="status">
    <span>{t("update.ready")}</span>
    <button class="upd" onclick={() => s.applyUpdate()}>{t("update.action")}</button>
    <button class="dismiss" aria-label={t("update.notNow")} onclick={() => (s.updateReady = false)}>✕</button>
  </div>
{/if}

<!-- This device would not take the write. Sticky: the reader's work is at risk
     and a fading toast would say so once. `title` carries the browser's own
     words for a bug report; the sentence carries what to do. -->
{#if s.persistFailed}
  <div class="toast warn" class:stacked={s.updateReady} role="status" title={s.persistFailed.detail}>
    <span>
      {s.persistFailed.retrying ? t("persist.retrying") : t("persist.failed")}
    </span>
    <button class="upd" onclick={() => s.retryPersist()}>{t("persist.tryAgain")}</button>
    <button class="dismiss" aria-label={t("boot.dismiss")} onclick={() => (s.persistFailed = null)}>✕</button>
  </div>
{/if}
<PromptDialog />
<Shortcuts />
<MapsHost />
<ContextMenu />
<TagPicker />
<TagWeave />
<PassagePicker />
<HistorySheet />
<PresentHost />
<SettingsDialog />
<BookNav />
<MarkReadDialog />
<ThreadPicker />
<ConfirmDialog />
<PickDialog />

<style>
  .frame {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
    /* Landscape on a notched phone: the cutout eats a ~47px column out of one
       side. Insetting the FRAME rather than each surface is what protects the
       text — the reader paints into whatever width it is given. */
    padding-inline-start: var(--safeLeft);
    padding-inline-end: var(--safeRight);
  }
  /* Measured, never seen. `visibility: hidden`, not `display: none` — a
     display:none box has no size to observe. */
  .rem-probe {
    position: fixed;
    top: 0;
    left: 0;
    width: 1rem;
    height: 1rem;
    visibility: hidden;
    pointer-events: none;
  }
  /* 48dp touch targets and text you do not lean in for: every control below is
     sized off that, not off how little room it can be squeezed into. */
  header {
    /* Stated once and added to below: two declarations of one padding drift. */
    --headerPadY: 10px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: var(--headerPadY) 14px;
    /* Clear of the status bar in an installed PWA. Horizontal insets are the
       frame's job (see `.frame`). */
    padding-top: calc(var(--headerPadY) + var(--safeTop));
    min-height: 52px;
    background: var(--paneNavBg, #efeae1);
    border-bottom: 1px solid var(--rule, #d8cba8);
    flex-wrap: wrap;
    /* Above every surface backdrop (memorize 38, settings 40, tag sheets 44): the
       chrome must stay reachable. Present (60) deliberately covers it. */
    position: relative;
    z-index: 46;
  }
  /* The phone's copy of the pane strip, hoisted into the one bar. Above 700px the
     panes carry their own (one header cannot steer three), so it is not drawn. */
  .chapter-nav {
    display: none;
    align-items: center;
    gap: 2px;
    /* Zero flex-basis AND a zero minimum. Under `flex-wrap: wrap`, line-breaking
       uses an item's UNSHRUNK size, so a content-sized nav drops the ⌕ and ≡ to a
       second bar however shrinkable it is; and `min-width: auto` is the nav's
       min-content, which hauls the whole book name back into line collection — a
       span's min-content contribution is its full text even under
       overflow:hidden (measured, chromium). At zero for both, the nav enters the
       line at nothing and grows into the spare width, and only `.pbook` gives
       ground. */
    flex: 1 1 0;
    min-width: 0;
  }
  .chapter-nav button {
    font-size: calc(19px * var(--uiScale, 1));
    line-height: 1;
    padding: 8px 12px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .chapter-nav .passage {
    /* 19, not 16: the bar's height comes from its 44px tap floor, and at 16 the
       passage — what the bar is about — fills a third of it and reads as lost. */
    font-size: calc(19px * var(--uiScale, 1));
    padding: 8px 10px;
    color: var(--ink, #211f1a);
    /* Two spans, so the book NAME ellipsizes and the chapter number never leaves
       the screen. Not `min-content` (that includes the whole name, see the nav's
       comment): `.pchap` is `flex: none` and `.pbook` alone gives ground. At
       320px × largest text the number can kiss the › arrow by a few px. */
    display: flex;
    align-items: baseline;
    gap: 0.3em;
    min-width: 0;
  }
  .chapter-nav .pbook {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .chapter-nav .pchap {
    flex: none;
    white-space: nowrap;
  }
  .subtitle {
    color: var(--faded, #8a8276);
    font-size: calc(16px * var(--uiScale, 1));
    /* The name changes length with every pane switch, and on a nearly-full row a
       name that cannot shrink wraps the destinations to a second line — the bar
       jostles on a tap that navigated nothing. A status readout, so ellipsis
       beats reflow; the pane's own strip still carries the full name. */
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A destination is not a passage. Above 700px the header stays (it is the
     destination switcher there) but the passage goes with the reader. */
  .frame:not([data-screen="read"]) .subtitle {
    display: none;
  }
  .browse {
    display: flex;
    gap: 2px;
    margin-inline-start: 8px;
  }
  .browse button {
    font-size: calc(16px * var(--uiScale, 1));
    padding: 8px 13px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .browse button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  /* Phone only: on a wide screen the destinations are already in the top bar. */
  .bottom-nav {
    display: none;
  }
  @media (max-width: 700px) {
    .bottom-nav {
      display: flex;
      background: var(--paneNavBg, #efeae1);
      border-top: 1px solid var(--rule, #d8cba8);
      /* Above the surface backdrops, like the header: the destinations have to
         stay reachable from whatever is open. Present stops above this bar
         (`--bottomNavH`) rather than covering it. */
      position: relative;
      z-index: 46;
      /* Clear of the home indicator on a notched phone; the shared variable
         (app.css) so every surface says it the same way. */
      padding-bottom: var(--safeBottom);
    }
    .bottom-nav button {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 2px;
      padding: 7px 0 5px;
      color: var(--faded, #8a8276);
      /* Material's bottom-nav height, taller than the 44px tap minimum. */
      min-height: 52px;
    }
    .bottom-nav svg {
      width: 24px;
      height: 24px;
      fill: currentColor;
      /* The selected pill (gold α0.14, applied on `.on svg` below). */
      border-radius: 999px;
      padding: 2px 14px;
      box-sizing: content-box;
    }
    .bottom-nav button.on {
      color: var(--gold, #9e7d38);
    }
    .bottom-nav button.on svg {
      background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
    }
    .bottom-nav span {
      font-size: calc(11px * var(--uiScale, 1));
      letter-spacing: 0.01em;
    }
    .browse,
    .subtitle {
      display: none;
    }
    /* A destination REPLACES the top bar rather than stacking under it: otherwise
       the phone shows the reader's chapter nav above a second bar saying
       "Explore". Phone only — above 700px the header IS the destination
       switcher, so hiding it would strand the reader. The bar that takes over
       needs the status-bar inset the header was carrying. */
    .frame:not([data-screen="read"]) > header {
      display: none;
    }
    .frame:not([data-screen="read"]) {
      --screenBarTop: var(--safeTop);
    }
    /* One bar on a phone: the chapter nav comes up into the header and the pane's
       own strip goes away, saving ~40px of a 780px screen. */
    .chapter-nav {
      display: flex;
    }
    .reading :global(.pane > .nav) {
      display: none;
    }
    /* One row where it fits, a second where it does not. `flex-wrap: nowrap`
       would not mean "one row" — it means the row overflows and the ≡ (the way
       to Settings) runs off the end, which the chrome following the reader's
       text size makes reachable on any phone. */
    header {
      /* The air between controls gives before the controls do: at the 44px tap
         floor (app.css) they are their own separation. */
      gap: 6px;
    }
    /* The chapter nav (flex-grow, above) owns the spare width on a phone: with
       the spacer still `flex: 1` the two split it and a long book name
       ellipsized beside blank space. */
    header .spacer {
      flex: 0 0 0;
    }
  }
  .spacer {
    flex: 1;
  }
  .glass {
    font-size: calc(20px * var(--uiScale, 1));
    line-height: 1;
    padding: 0 6px;
    color: var(--gold, #9e7d38);
  }
  .menu-btn {
    font-size: calc(20px * var(--uiScale, 1));
    padding: 0 8px;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 47;
  }
  /* Fixed to the top-right, not anchored to the header: the ≡ is raised from any
     destination's ScreenBar too, where the header is not mounted. */
  .menu {
    position: fixed;
    inset-inline-end: 8px;
    top: calc(var(--safeTop, 0px) + 52px);
    z-index: 48;
    min-width: 190px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.12);
    padding: 6px;
    display: flex;
    flex-direction: column;
    max-height: 80vh;
    overflow-y: auto;
  }
  .menu button {
    text-align: start;
    padding: 5px 8px;
    border-radius: 5px;
  }
  .menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .menu button:not(:disabled):hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .reading {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: flex;
    position: relative;
  }
  /* Tinted with the app's one alarm colour (tierResearch), so a glance says the
     reader is in a mode where a tap tags. */
  .concept-study-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.35rem 0.9rem;
    /* --tierResearch, not --tier-research: the misspelt name falls back to the
       hard-coded colour silently, on every theme. */
    background: color-mix(in srgb, var(--tierResearch, #b04a3a) 14%, var(--paper, #fcf9f4));
    border-bottom: 1px solid var(--tierResearch, #b04a3a);
    color: var(--ink, #211f1a);
    font-size: 0.95rem;
  }
  .concept-study-banner .tag {
    font-weight: 600;
    flex: 1;
    min-width: 0;
  }
  .concept-study-banner .prog {
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    color: var(--faded, #8a8276);
  }
  .concept-study-banner .exit {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 4px;
    padding: 0.2rem 0.6rem;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    cursor: pointer;
  }
  .panes > :global(.pane + .pane) {
    border-inline-start: 1px solid var(--rule, #d8cba8);
  }
  .toast {
    /* Stated once so the stacked notice below can be expressed in terms of it.
       Above the bottom bar on a phone (a snackbar never covers the
       destinations), above the home indicator in landscape, 22px off the edge on
       a desktop. `max`, not a sum: the bar carries the inset inside its measured
       height. */
    --toastBottom: calc(max(var(--bottomNavH, 0px), var(--safeBottom)) + 22px);
    position: fixed;
    bottom: var(--toastBottom);
    left: 50%;
    transform: translateX(-50%);
    z-index: 50;
    background: var(--ink, #211f1a);
    color: var(--paper, #fcf9f4);
    padding: 8px 16px;
    border-radius: 8px;
    font-size: calc(14px * var(--uiScale, 1));
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.25);
    /* A sentence wraps inside the screen instead of running edge to edge. */
    max-width: min(560px, calc(100vw - 24px));
    box-sizing: border-box;
  }
  /* The passing confirmation rides over EVERY surface, Present included (z 60):
     "Link copied" is raised from inside Present, and at the shell's 50 it landed
     behind the screen whose button raised it. Still under the failure bar (70);
     the sticky notices below keep the shell's level. */
  .toast.brief {
    z-index: 65;
    text-align: center;
  }
  .toast.update,
  .toast.warn {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px 8px 16px;
    max-width: min(560px, calc(100vw - 24px));
  }
  .toast.update .upd,
  .toast.warn .upd {
    background: var(--gold, #9e7d38);
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 5px 12px;
    font-size: calc(14px * var(--uiScale, 1));
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .toast.update .dismiss,
  .toast.warn .dismiss {
    background: none;
    border: none;
    color: inherit;
    opacity: 0.7;
    font-size: calc(14px * var(--uiScale, 1));
    cursor: pointer;
  }
  /* The alarm colour as a left edge only — enough to read as "wrong" without
     turning the reader's page into a warning banner. */
  .toast.warn {
    border-inline-start: 4px solid var(--tierResearch, #b04a3a);
  }
  .toast.warn .upd {
    background: var(--tierResearch, #b04a3a);
  }
  /* Both sticky notices at once: this one clears the update toast's own height —
     a 44px control (app.css's tap floor) inside 8px of padding — plus a gap,
     spelled out rather than a constant so it cannot go stale. */
  .toast.warn.stacked {
    bottom: calc(var(--toastBottom) + 44px + 16px + 12px);
  }
</style>
