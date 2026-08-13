<script lang="ts">
  // The app frame: header (browse buttons · live search · ≡ menu), pane row,
  // study surface, and global keyboard bindings. Wide screens follow the
  // desktop layout; narrow screens use the Compose-phone patterns.
  import ReaderPane from "../reader/ReaderPane.svelte";
  import StudyPanel from "../study/StudyPanel.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import Shortcuts from "./Shortcuts.svelte";
  import BookNav from "./BookNav.svelte";
  import ExploreScreen from "./ExploreScreen.svelte";
  import PlansScreen from "./PlansScreen.svelte";
  import VizScreen from "./VizScreen.svelte";
  import PreachScreen from "./PreachScreen.svelte";
  import HymnalScreen from "../hymnal/HymnalScreen.svelte";
  import MarkReadDialog from "./MarkReadDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CanonStrip from "./CanonStrip.svelte";
  import PlanChip from "./PlanChip.svelte";
  import HistorySheet from "./HistorySheet.svelte";
  import SettingsDialog from "./SettingsDialog.svelte";
  import MemorizeHost from "../memorize/MemorizeHost.svelte";
  import PassagePicker from "../memorize/PassagePicker.svelte";
  import PresentHost from "../present/PresentHost.svelte";
  import ConnectorsOverlay from "./ConnectorsOverlay.svelte";
  import MapsHost from "../maps/MapsHost.svelte";
  import ContextMenu from "../reader/ContextMenu.svelte";
  import TagPicker from "../study/TagPicker.svelte";
  import ThreadPicker from "../study/ThreadPicker.svelte";
  import TagWeave from "../study/TagWeave.svelte";
  import ShareScreen from "./ShareScreen.svelte";
  import { t } from "../lib/i18n.svelte";
  import { uiScale } from "../lib/uiScale";
  import { DEFAULT_FONT, FONT_SCALE } from "../engine/fonts.generated";
  import { getSession } from "../state/session.svelte";
  import { startReadingTracker } from "../state/readingTracker";

  const s = getSession();

  // Reading time for the navigator's map (core::reading). The FIRST pane only: a
  // second pane is usually a parallel reference being consulted, not a chapter
  // being read through. Nothing is tracked while a modal surface is up — Present,
  // the memorize drill and the maps are not reading.
  $effect(() =>
    startReadingTracker({
      target: () => {
        // Concept-study skimming is not reading (docs/READING-PLANS.md §Concept Study):
        // the mode suspends the tracker so a sweep credits no dwell to the
        // reading map or any schedule plan derived from it.
        if (s.showPresent || s.screen !== "read" || s.mapPopup || s.inConceptStudy) return null;
        const p = s.panes[0];
        return p ? { book: p.book, chapter: p.chapter } : null;
      },
      reached: () => s.panes[0]?.reached ?? 0,
      // One sample per second into the core's tracker, which owns grace, idle,
      // the cadence and the tail (H-11). The old pair of calls is gone: there is
      // no `spec` fetch, because the thresholds never leave the core, and no
      // `readingRecord` from here, because the core banks its own reports.
      tick: (book, chapter, reached, step, interacted) =>
        s.rpc
          .call("readingTick", book, chapter, reached, step, interacted, new Date().toISOString())
          .catch(() => null),
      // NO completion toast. Finishing a chapter is the reader's own moment —
      // "you should know you read it" (maintainer UAT call, 2026-08-11); a
      // notification popping over the text at exactly that moment is the app
      // congratulating someone for something they were present for. The read
      // still lands in the map, the plans and the streak — silently.
    }),
  );

  const subtitle = $derived.by(() => {
    const p = s.panes[s.activePane];
    return p ? `${s.bookName(p.book)} ${p.chapter}` : "";
  });

  // The sweep's progress, on the banner. The reading map's glow is deliberately
  // frozen in the mode (the tracker is suspended), so without this the mode had
  // no visible progress at all outside the Plans panel. Live: every sweep is an
  // authoring write, and authored invalidates the plans read this derives from.
  const sweepProgress = $derived.by(() => {
    if (!s.inConceptStudy) return null;
    const run = ((s.q("plans", "")?.running ?? []) as any[]).find((p) => p.id === s.conceptStudyId);
    return (run?.sweepProgress as [number, number] | undefined) ?? null;
  });

  function openWordStudy(refKey: string, tokenIndex: number): void {
    s.panel = { kind: "wordStudy", refKey, tokenIndex };
  }

  // ── live search: per keystroke; empty query closes (manifest §Search) ──
  // The field shows `searchDraft` and never lags a keystroke; `setSearch` starts
  // the 180 ms trailing timer that moves it into `searchQuery`, which is what the
  // panel asks the engine for. Opening the panel keys off the DRAFT so the sheet
  // appears as soon as there is something to search for, not a fifth of a second
  // later — only the engine call is debounced.
  function onSearchInput(e: Event): void {
    s.setSearch((e.currentTarget as HTMLInputElement).value);
    if (s.searchDraft.trim()) s.panel = { kind: "search" };
    else if (s.panel?.kind === "search") s.panel = null;
  }

  // A permanent search field wrapped the bar onto a second row on a phone, so
  // on narrow screens it collapses to a magnifying glass and takes the row only
  // while it is being used.
  let searchOpen = $state(false);
  let searchEl = $state<HTMLInputElement | null>(null);
  function openSearch(): void {
    searchOpen = true;
    queueMicrotask(() => searchEl?.focus());
  }
  function closeSearch(): void {
    searchOpen = false;
    s.clearSearch();
    if (s.panel?.kind === "search") s.panel = null;
  }
  // Surfaces are exclusive: picking a destination closes the others
  // (Memorize left open over Explore was disorienting). Shared by the
  // header's destination buttons and the ≡ utilities.
  //
  // `dismissTransient` owns the closing list, beside the declarations it has to
  // keep up with — so a transient surface added later is still escaped by
  // tapping a destination.
  function go(action: () => void): () => void {
    return () => {
      // `dismissTransient` closes the ≡ menu too (it is in the TRANSIENT table).
      s.dismissTransient();
      action();
    };
  }

  // The bottom bar's five ROLES — Read · Study · Preach · Share · Sing. Not a
  // list of features but of the hats a reader wears; the tools live one layer
  // down (Memorize is a card inside Study, the church rides Share). Icon paths
  // are copied verbatim from the Compose shell's NavIcons.kt (standard Material
  // Symbols: book, school, present_to_all, share, music_note) so both shells
  // draw the same glyphs.
  // Ids, not labels. This array is built ONCE, so a label in it would be a
  // snapshot of whatever language the app booted in; storing ids and rendering
  // them through `t()` below keeps the nav live across a language change.
  const NAV = [
    {
      key: "read",
      path:
        "M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" +
        "M6 4h5v8l-2.5-1.5L6 12V4z",
      // Read opens nothing — `dismissTransient` has already cleared every layer,
      // which IS arriving at the text.
      go: () => {},
    },
    {
      key: "study",
      // Material Symbols "school" — the study role's glyph.
      path: "M5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82zM12 3L1 9l11 6 11-6-11-6z",
      go: () => (s.screen = "explore"),
    },
    {
      key: "preach",
      path:
        "M21 3H3c-1.11 0-2 .89-2 2v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2z" +
        "m0 16.02H3V4.98h18v14.04zM10 12H8l4-4 4 4h-2v4h-4v-4z",
      // A hub like Study, not a straight raise of Present: the role holds the
      // presentation AND its materials (weaves, tags, notes).
      go: () => (s.screen = "preach"),
    },
    {
      key: "share",
      // Material Symbols "share".
      path:
        "M18 16.08c-.76 0-1.44.3-1.96.77L8.91 12.7c.05-.23.09-.46.09-.7s-.04-.47-.09-.7l7.05-4.11" +
        "c.54.5 1.25.81 2.04.81 1.66 0 3-1.34 3-3s-1.34-3-3-3-3 1.34-3 3c0 .24.04.47.09.7L8.04 9.81" +
        "C7.5 9.31 6.79 9 6 9c-1.66 0-3 1.34-3 3s1.34 3 3 3c.79 0 1.5-.31 2.04-.81l7.12 4.16" +
        "c-.05.21-.08.43-.08.65 0 1.61 1.31 2.92 2.92 2.92s2.92-1.31 2.92-2.92-1.31-2.92-2.92-2.92z",
      go: () => (s.screen = "share"),
    },
    {
      key: "sing",
      // Material Symbols "music_note".
      path: "M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z",
      go: () => (s.screen = "hymnal"),
    },
  ] as const;

  // Which tab reads as current. Present wins because it covers everything, then
  // Memorize, then Explore — matching the layering, so the highlighted tab is
  // always the surface actually in front of the reader. Read is what is left.
  // How much room the bottom bar takes, published as `--bottomNavH` so a
  // full-screen surface can stop above it instead of underlapping it.
  //
  // MEASURED, not restated. Writing the height into a CSS constant — `calc(52px
  // + safe-area)` — gets it wrong, because the bar is a button min-height plus
  // padding plus a border, and none of those are the number anyone would think
  // to copy. Two declarations of one length drift the moment either side is
  // touched; an observer cannot.
  //
  // Zero when the bar is `display: none` (desktop widths), which is exactly what
  // a surface wants there.
  let navEl = $state<HTMLElement | null>(null);
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
  // below it the screen decides — Memorize lights Study, because that is the
  // role it belongs to now that it is a card inside the Study hub.
  const dest = $derived(
    s.showPresent || s.screen === "preach"
      ? "preach"
      : s.screen === "explore" || s.screen === "memorize" || s.screen === "plans" || s.screen === "viz"
        ? "study"
        : s.screen === "hymnal"
          ? "sing"
          : s.screen === "share"
            ? "share"
            : "read",
  );

  // The reader's text size as a factor of the 18px the chrome was drawn at,
  // TIMES the chrome face's optical scale (FONT_SCALE — faces differ in
  // x-height, and switching faces must change the chrome's voice, not its
  // apparent size). Composed here rather than published as a second variable:
  // `--uiScale` stays the one number the whole chrome multiplies by. The scale
  // itself is published on `:root` by `use:uiScale` on the probe below — this
  // is the app's half of it; the browser's own font preference is the other
  // half and the probe measures that. See lib/uiScale.ts.
  const readerScale = $derived(
    (Number(s.config.bodySize ?? 18) / 18) * (FONT_SCALE[s.config.chromeFont ?? DEFAULT_FONT] ?? 1),
  );

  // ── global keys (manifest §Keyboard + wheel) ──
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
    const fontPx = Number(s.config.bodySize ?? 18);
    const line = fontPx * 3;
    const page = 0.85 * (innerHeight - 120);
    const scroll = (dy: number, all = false) => {
      for (const p of all ? s.panes : [pane]) {
        p.scrollY = Math.max(0, p.scrollY + dy);
        p.pendingScroll = false;
      }
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
      // Escape peels ONE layer, outermost first, so it never skips past something
      // the reader can see. The destination screens are the last layer before the
      // text, which is why they come after the panel: Escape out of a study panel
      // opened from Explore should land back in Explore, not in Genesis.
      //
      // THE DIALOGS ANSWER FIRST NOW. Every `aria-modal` surface carries
      // `use:modal` (lib/modal.ts), which takes focus and stops Escape at the
      // dialog — so while one is open this ladder is not reached at all, and the
      // press cannot peel a second layer out from under it. The dialog rungs stay
      // as the fallback for a press that arrives with focus outside any of them.
      // What the ladder still OWNS is everything that is not a dialog: the map
      // popup, the study panel, and the destination screens.
      //
      // Note the early return at the top of this function: it drops every key
      // that came from a field, so Escape pressed while the reader is typing in
      // one never reaches this ladder.
      case "Escape":
        if (s.menuOpen) s.menuOpen = false;
        else if (s.promptReq) s.cancelPrompt();
        else if (s.mapPopup) s.mapPopup = null;
        else if (s.bookNavFor !== null) s.bookNavFor = null;
        else if (s.markReadFor) s.markReadFor = null;
        else if (s.threadPickFor) s.threadPickFor = null;
        else if (s.tagPickFor) s.tagPickFor = null;
        else if (s.showSettings) s.showSettings = false;
        else if (s.showHistory) s.showHistory = false;
        else if (s.showShortcuts) s.showShortcuts = false;
        else if (s.panel) {
          s.panel = null;
          s.clearSearch();
        } else if (s.screen !== "read") s.goRead();
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
  <!-- One rem wide, and that is the whole of it: the browser's own default text
       size, as a box a ResizeObserver can watch. lib/uiScale.ts turns it and the
       reader's setting into `--uiScale`. -->
  <div class="rem-probe" aria-hidden="true" use:uiScale={readerScale}></div>
  <header>
    <!-- NO APP TITLE, at any width. Android has never had one, and on a phone it
         was the widest thing in the bar: it pushed Welcome, Church and Share
         into a second row of chrome above a reader who already knows which app
         they opened. The tab title and the install manifest carry the name.

         On a PHONE the chapter nav lives here, as it does on Android — one bar,
         not a header above a pane strip. A phone is capped at one pane
         (`maxPanes`), so this is that pane, and the pane's own strip is hidden
         at this width. -->
    <div class="chapter-nav">
      <button onclick={() => s.stepChapter(s.activePane, -1)} aria-label={t("common.previousChapter")}>‹</button>
      <button class="passage" onclick={() => (s.bookNavFor = s.activePane)}>{subtitle} ▾</button>
      <button onclick={() => s.stepChapter(s.activePane, 1)} aria-label={t("common.nextChapter")}>›</button>
    </div>
    <!-- Which pane is ACTIVE. Hidden on a phone, where the chapter nav above
         says it already, and on every DESTINATION, where the passage is not what
         the screen is about — the Hymnal advertising "1 Corinthians 7" in the
         top-left is the wide-screen version of the stacked-bars problem the
         phone rule below fixes (Android never shows it: its TopBar is drawn
         inside Dest.Read only). Kept in the DOM at every width on purpose: 21
         e2e files use it as the "text is on screen" boot signal. -->
    <span class="subtitle">{subtitle}</span>
    <!-- The ROLES are first-class in the top bar (Compose bottom-nav parity:
         Read is the base layer, then Study · Preach · Share · Sing); the ≡ menu
         holds utilities only. The tools live one layer down — Threads/Tags/
         Weaves/Memorize inside Study, the church and the QR on Share. -->
    <nav class="browse">
        <button onclick={go(() => (s.screen = "explore"))}>{t("nav.study")}</button>
        <button onclick={go(() => (s.screen = "preach"))}>{t("nav.preach")}</button>
        <button onclick={go(() => (s.screen = "share"))}>{t("nav.share")}</button>
        <button onclick={go(() => (s.screen = "hymnal"))}>{t("nav.sing")}</button>
      </nav>
    <span class="spacer"></span>
    <button class="glass" class:searching={searchOpen} onclick={openSearch} aria-label={t("common.openSearch")}>⌕</button>
    <input
      class="search"
      class:open={searchOpen}
      type="search"
      placeholder={t("shell.searchPlaceholder")}
      bind:this={searchEl}
      value={s.searchDraft}
      oninput={onSearchInput}
      onkeydown={(e) => e.key === "Escape" && closeSearch()}
      aria-label={t("common.search")}
    />
    {#if searchOpen}
      <button class="glass narrow-close" onclick={closeSearch} aria-label={t("common.closeSearch")}>✕</button>
    {/if}
    <button class="menu-btn" onclick={() => (s.menuOpen = !s.menuOpen)} aria-label={t("common.menu")}>≡</button>
  </header>

  <!-- One destination at a time, Android's model: a screen REPLACES the reader
       rather than sliding over it. The study panel still layers on top, because it
       is a panel about the verse you are looking at, not a place you go. -->
  <div class="body">
    {#if s.screen === "explore"}
      <ExploreScreen />
    {:else if s.screen === "memorize"}
      <MemorizeHost />
    {:else if s.screen === "plans"}
      <PlansScreen />
    {:else if s.screen === "viz"}
      <VizScreen />
    {:else if s.screen === "preach"}
      <PreachScreen />
    {:else if s.screen === "hymnal"}
      <HymnalScreen />
    {:else if s.screen === "share"}
      <ShareScreen />
    {:else}
      <div class="reading">
        {#if s.inConceptStudy}
          <!-- Concept-study mode: a persistent banner so the reader always knows a
               tap tags rather than opens study, and can leave with one press. -->
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
            <!-- A connector goes BETWEEN panes, so one pane has none — and a
                 phone only ever has one (`addPane` refuses when narrow). Not
                 mounting it there is the difference between a full-viewport
                 canvas re-allocated on every scroll frame to draw nothing and
                 no canvas at all. -->
            <ConnectorsOverlay />
          {/if}
        </div>
        <PlanChip />
        <CanonStrip />
      </div>
    {/if}
    <StudyPanel />
  </div>

  <!-- THE BOTTOM BAR (narrow only) — the five roles, in thumb reach: Read ·
       Study · Preach · Share · Sing. The icons are the very same Material paths
       the Compose shell draws (apps/android/.../NavIcons.kt), so the two shells
       look like one product rather than two interpretations.

       Read is not a destination so much as the absence of one: the reader is
       always mounted underneath, so its tap just clears whatever is layered
       over it. -->
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
  <!-- The ≡ utilities, raised from the header OR any destination's ScreenBar —
       fixed to the top-right so it works over every screen, not just Read.
       UTILITIES ONLY: the roles live in the bottom bar / browse row. Welcome is
       here for EVERY reader (an established believer never had `intro` set, so
       a conditional entry hid it from them — it falls back to the new-believer
       welcome). The church is no longer a menu trip: it rides the Share screen. -->
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.menuOpen = false)}></div>
  <div class="menu" role="menu" aria-label={t("common.menu")}>
    <button onclick={go(() => (s.reopenIntro = s.intro ?? "new"))}>{t("shell.welcome")}</button>
    <button onclick={go(() => (s.showHistory = true))}>{t("shell.history")}</button>
    <button onclick={go(() => (s.panel = { kind: "guide" }))}>{t("shell.guideAndAbout")}</button>
    <button onclick={go(() => (s.showShortcuts = true))}>{t("shell.shortcuts")}</button>
    <button onclick={go(() => (s.showSettings = true))}>{t("shell.settings")}</button>
  </div>
{/if}

<!-- `role="status"`, as the update and storage notices below already have. Every
     confirmation this app gives — "Copied", "Tagged Isaiah 53:5", "Couldn't make
     the backup" — arrives here and nowhere else, and without a live region a
     screen reader was told none of them: the toast appears, sits for 2.2
     seconds, and goes, all in silence. -->
{#if s.toast}
  <div class="toast" role="status">{s.toast}</div>
{/if}

<!-- A deploy landed while this session was open (an installed PWA can sit for
     weeks). Offered, never taken automatically — reloading someone mid-verse
     to save them a tap is not a kindness. -->
{#if s.updateReady}
  <div class="toast update" role="status">
    <span>{t("update.ready")}</span>
    <button class="upd" onclick={() => s.applyUpdate()}>{t("update.action")}</button>
    <button class="dismiss" aria-label={t("update.notNow")} onclick={() => (s.updateReady = false)}>✕</button>
  </div>
{/if}

<!-- This device would not take the write. Sticky, like the update notice and for
     a stronger reason: a fading toast would tell a reader their work is at risk
     while they are looking at the note they just typed. `title` carries the
     browser's own words for a bug report; the sentence carries what to do. -->
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

<style>
  .frame {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
    /* LANDSCAPE, which is a reading posture and not an edge case: rotate a
       notched phone and the cutout eats a 47px column out of one side of the
       page. Insetting the frame rather than each surface inside it is what
       protects the TEXT — the reader paints into whatever width it is given, and
       nothing else in the tree would have covered it. The chrome's own
       backgrounds stop at the inset, which is the honest thing to show: the
       strip beside them is not screen the app can use. */
    padding-left: var(--safeLeft);
    padding-right: var(--safeRight);
  }
  /* Measured, never seen: out of flow, out of the accessibility tree, and out of
     the way of a tap. `visibility: hidden` and not `display: none` on purpose —
     a display:none box has no size to observe. */
  .rem-probe {
    position: fixed;
    top: 0;
    left: 0;
    width: 1rem;
    height: 1rem;
    visibility: hidden;
    pointer-events: none;
  }
  /* Android sets the standard for this bar: 48dp touch targets and text you do
     not lean in for. Every control below is sized off that rather than off how
     little room it can be squeezed into — this bar holds the passage the reader
     taps most. */
  header {
    /* Stated once and added to below: two declarations of one padding drift the
       moment either is touched, which is the lesson `--bottomNavH` was taught. */
    --headerPadY: 10px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: var(--headerPadY) 14px;
    /* Under the status bar in an installed PWA — the title and the ≡ were behind
       the clock. Horizontal insets are the frame's job (see `.frame`). */
    padding-top: calc(var(--headerPadY) + var(--safeTop));
    min-height: 52px;
    background: var(--paneNavBg, #efeae1);
    border-bottom: 1px solid var(--rule, #d8cba8);
    flex-wrap: wrap;
    /* Above every surface backdrop (memorize 38, settings 40, tag sheets 44):
       the chrome must stay reachable — switching destinations from the menu
       closes whatever is open. Present (60) deliberately covers it. */
    position: relative;
    z-index: 46;
  }
  /* The chapter nav is the PHONE's copy of the pane strip, hoisted into the one
     bar. Above 700px the panes carry their own (there can be three of them, and
     one header cannot steer three), so it is not drawn at all. */
  .chapter-nav {
    display: none;
    align-items: center;
    gap: 2px;
  }
  .chapter-nav button {
    font-size: calc(19px * var(--uiScale, 1));
    line-height: 1;
    padding: 8px 12px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .chapter-nav .passage {
    /* 19, not 16. The bar's height is set by its 44px tap floor, and the
       passage — what the bar is ABOUT, and its widest target — filled about a
       third of it and read as lost (maintainer, on a Pixel, 2026-08-13). The
       Android twin took the same number. */
    font-size: calc(19px * var(--uiScale, 1));
    padding: 8px 10px;
    color: var(--ink, #211f1a);
    white-space: nowrap;
  }
  .subtitle {
    color: var(--faded, #8a8276);
    font-size: calc(16px * var(--uiScale, 1));
    /* The passage name changes length with every pane switch ("John 3" ↔
       "1 Corinthians 13"), and on an open fold the row runs close to full: a
       name that cannot shrink tips it over and Welcome/Church/Share drop to a
       wrapped second row — the whole bar jostles on a tap that navigated
       nothing. It is a status readout, so ellipsis beats reflow; the pane's
       own strip still carries the full name. */
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A destination is not a passage. Above 700px the header stays (it IS the
     destination switcher up there), but the reader's passage goes with the
     reader — otherwise the Hymnal, Study, Preach and Share all wear a chapter
     name they have nothing to do with. */
  .frame:not([data-screen="read"]) .subtitle {
    display: none;
  }
  .browse {
    display: flex;
    gap: 2px;
    margin-left: 8px;
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
  /* The bottom bar is a PHONE affordance: on a wide screen the destinations are
     already first-class in the top bar, and a nav bar pinned to the bottom of a
     desktop window is just a strip of wasted height. */
  .bottom-nav {
    display: none;
  }
  @media (max-width: 700px) {
    .bottom-nav {
      display: flex;
      background: var(--paneNavBg, #efeae1);
      border-top: 1px solid var(--rule, #d8cba8);
      /* Above the surface backdrops, like the header — the destinations have to
         stay reachable from whatever is open, since tapping one closes it.
         Present stops above this bar (`--bottomNavH`, published from the measured
         height here) rather than covering it, so nothing needs to out-stack it. */
      position: relative;
      z-index: 46;
      /* Clear of the home indicator / gesture bar on a notched phone. Reads the
         shared variable (app.css) so every surface says it the same way and a
         test can drive all of them. */
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
      /* A comfortable thumb target: Material's bottom-nav height, and taller
         than the 44px minimum on its own. */
      min-height: 52px;
    }
    .bottom-nav svg {
      width: 24px;
      height: 24px;
      fill: currentColor;
      /* The selected pill, Compose's indicatorColor (gold α0.14). */
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
    /* A DESTINATION REPLACES THE TOP BAR, it does not stack under it: otherwise
       Explore, Memorize and the Hymnal sit below the READER'S bar — its chapter
       nav, its search, its share — and the phone shows "‹ 1 Corinthians 7 ›"
       above a second bar saying "Explore", advertising a passage the screen has
       nothing to do with. Present has always replaced the lot and looks right
       for it; Android does the same, because its destinations own the whole
       column.
       Only on a phone: above 700px the header IS the destination switcher (there
       is no bottom bar up there), so hiding it would strand the reader. The bar
       that takes over needs the status-bar inset the header was carrying. */
    .frame:not([data-screen="read"]) > header {
      display: none;
    }
    .frame:not([data-screen="read"]) {
      --screenBarTop: var(--safeTop);
    }
    /* ONE BAR on a phone, as on Android. The chapter nav comes up into the
       header and the pane's own strip goes away — two stacked strips of chrome
       over a single pane was the "second row" that made this look janky, and it
       cost ~40px of a 780px screen to say what one row already said. */
    .chapter-nav {
      display: flex;
    }
    .reading :global(.pane > .nav) {
      display: none;
    }
    /* One row where it fits, a second row where it does not.
       The glass stands in for the field until it's wanted, and while searching
       the field owns the row, so at the default text size this is one row.

       `flex-wrap: nowrap` would not mean "one row" — it means the row overflows
       and what runs off the end is the ≡, i.e. the way to Settings. Because the
       chrome follows the reader's text size, someone reading at 36px reaches
       that width on any phone, and a control pushed off the screen is worse than
       a header two rows tall. Wrapping is the graceful version of the same
       overflow. */
    header {
      /* The air between controls gives before the controls do: with the chapter
         nav, the glass and the ≡ in one nowrap row, the 44px tap floor (app.css)
         costs width a 360px phone has not got — and what runs off the end is the
         ≡, which is the way to Settings. At 44px each they are their own
         separation and do not need 10px of it. */
      gap: 6px;
    }
    /* `header` prefix so these beat the base rules further down the file. */
    header .glass {
      display: block;
    }
    header .search {
      display: none;
    }
    header .search.open {
      display: block;
      flex: 1;
      width: auto;
      min-width: 0;
    }
    .glass.searching {
      display: none;
    }
    /* Searching, the field owns the row: everything that is not the field or
       the way out of it stands down, so the row never has to wrap to hold a
       query. The chapter nav is the widest of them and goes first. */
    header:has(.search.open) .chapter-nav {
      display: none;
    }
  }
  .spacer {
    flex: 1;
  }
  .glass {
    display: none; /* wide screens keep the field itself */
    font-size: calc(20px * var(--uiScale, 1));
    line-height: 1;
    padding: 0 6px;
    color: var(--gold, #9e7d38);
  }
  .search {
    width: min(240px, 38vw);
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 4px 9px;
    font-size: calc(14px * var(--uiScale, 1));
    /* The width above is the field's IDEAL, not a floor: on an open fold it is
       the widest flexible thing in the row, and holding 240px while the
       subtitle grows is what used to wrap the bar. It gives ground down to a
       still-typable minimum before anything falls to a second row. */
    flex-shrink: 1;
    min-width: 110px;
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
  /* FIXED to the top-right, not anchored to the header: the ≡ can be raised
     from any destination's ScreenBar, and on a phone the header is not even
     mounted there. One position serves every caller. */
  .menu {
    position: fixed;
    right: 8px;
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
    text-align: left;
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
  /* Concept-study banner: the app's one alarm colour (tierResearch) tints it, so a
     reader glances down and knows they are in a mode where a tap tags. */
  .concept-study-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.35rem 0.9rem;
    /* --tierResearch, not --tier-research: the token every other surface uses
       (ConfirmDialog); the misspelt name silently fell back to the hard-coded
       colour on every theme. */
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
    border-left: 1px solid var(--rule, #d8cba8);
  }
  .toast {
    /* Stated once so the stacked notice below can be expressed in terms of it. */
    --toastBottom: 22px;
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
  }
  /* A toast that stays until acted on carries its own buttons. */
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
  /* The app's one alarm colour (--tierResearch, as ConfirmDialog's destructive
     button uses) as a left edge — enough to read as "wrong" without turning the
     reader's page into a warning banner. */
  .toast.warn {
    border-left: 4px solid var(--tierResearch, #b04a3a);
  }
  .toast.warn .upd {
    background: var(--tierResearch, #b04a3a);
  }
  /* Both sticky notices at once: this one sits above the update.
     Its clearance is the update toast's own height, spelled out — a 44px control
     (the tap floor in app.css) inside 8px of padding top and bottom — plus a gap.
     Spelled out rather than a fixed number, so it cannot go stale against the
     control's real height. */
  .toast.warn.stacked {
    bottom: calc(var(--toastBottom) + 44px + 16px + 12px);
  }
</style>
