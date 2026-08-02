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
  import HymnalScreen from "../hymnal/HymnalScreen.svelte";
  import MarkReadDialog from "./MarkReadDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CanonStrip from "./CanonStrip.svelte";
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
  import QrCode from "./QrCode.svelte";
  import { churchTitle as churchLabel, hasChurch, visitChurch as openChurchSite } from "./church";
  import { modal } from "../lib/modal";
  import { uiScale } from "../lib/uiScale";
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
        if (s.showPresent || s.screen !== "read" || s.mapPopup) return null;
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
      onCompleted: (book, chapter) => s.showToast(`Read through — ${s.bookName(book)} ${chapter}`),
    }),
  );

  const subtitle = $derived.by(() => {
    const p = s.panes[s.activePane];
    return p ? `${s.bookName(p.book)} ${p.chapter}` : "";
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

  let menuOpen = $state(false);
  // Search used to be a permanent field. With Welcome, Church and Share
  // beside it the bar wrapped onto a second row on a phone (feedback
  // 2026-07-27), so on narrow screens it collapses to a magnifying glass and
  // takes the row only while it is being used.
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
  // Share the app: the PWA QR + link (Compose ShareAppDialog parity) — a
  // first-class header button (2026-07-26), not a menu trip.
  let shareApp = $state(false);
  // What we actually hand over: the app, plus this reader's church when they
  // have set one (Settings → Your church). One QR, both things.
  const link = $derived(s.shareLink);
  async function shareLink(): Promise<void> {
    const title = hasChurch(s.church) ? `Plumbline — from ${s.church.name}` : "Plumbline";
    if (navigator.share) {
      try {
        await navigator.share({ title, url: link });
        return;
      } catch {
        /* fall through to clipboard */
      }
    }
    await navigator.clipboard.writeText(link);
    s.showToast("Link copied");
  }
  // Surfaces are exclusive: picking a destination closes the others
  // (Memorize left open over Explore was disorienting). Shared by the
  // header's destination buttons and the ≡ utilities.
  //
  // The closing list used to live here and named five surfaces. There are
  // thirteen, so every one added since — the note editor above all — could not be
  // escaped by tapping a destination (feedback 2026-07-29). `dismissTransient`
  // owns it now, beside the declarations it has to keep up with.
  function go(action: () => void): () => void {
    return () => {
      menuOpen = false;
      s.dismissTransient();
      action();
    };
  }

  // The bottom bar's four destinations. Icon paths are copied verbatim from the
  // Compose shell's NavIcons.kt (standard Material Symbols: book, explore,
  // present_to_all, school) so both shells draw the same glyphs.
  const NAV = [
    {
      key: "read",
      label: "Read",
      path:
        "M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" +
        "M6 4h5v8l-2.5-1.5L6 12V4z",
      // Read opens nothing — `dismissTransient` has already cleared every layer,
      // which IS arriving at the text.
      go: () => {},
    },
    {
      key: "explore",
      label: "Explore",
      path:
        "M12 10.9c-.61 0-1.1.49-1.1 1.1s.49 1.1 1.1 1.1c.61 0 1.1-.49 1.1-1.1s-.49-1.1-1.1-1.1z" +
        "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z" +
        "m2.19 12.19L6 18l3.81-8.19L18 6l-3.81 8.19z",
      go: () => (s.screen = "explore"),
    },
    {
      key: "present",
      label: "Present",
      path:
        "M21 3H3c-1.11 0-2 .89-2 2v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2z" +
        "m0 16.02H3V4.98h18v14.04zM10 12H8l4-4 4 4h-2v4h-4v-4z",
      go: () => (s.showPresent = true),
    },
    {
      key: "memorize",
      label: "Memorize",
      path: "M5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82zM12 3L1 9l11 6 11-6-11-6z",
      // `dismissTransient` resets the screen first, so these set it after.
      go: () => {
        s.screen = "memorize";
        s.memorize = { view: "hub" };
      },
    },
    {
      key: "hymnal",
      label: "Hymnal",
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
  // MEASURED, not restated. The first version wrote the height into a CSS
  // constant — `calc(52px + safe-area)` — and was wrong by 5px on the first
  // device it met, because the bar is a button min-height plus padding plus a
  // border, and none of those are the number anyone would think to copy. Two
  // declarations of one length drift the moment either side is touched; an
  // observer cannot.
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

  // Which tab reads as current. Present wins because it covers everything; below
  // it, `s.screen` IS the destination now that Explore and Memorize are screens.
  const dest = $derived(s.showPresent ? "present" : s.screen);

  // The reader's text size as a factor of the 18px the chrome was drawn at. The
  // scale itself is published on `:root` by `use:uiScale` on the probe below —
  // this is only the reader's half of it; the browser's own font preference is
  // the other half and the probe measures that. See lib/uiScale.ts.
  const readerScale = $derived(Number(s.config.bodySize ?? 18) / 18);

  // The church button opens their site; with no site to open it at least
  // tells the reader who and when, which is all we were given.
  // Both of these live in church.ts now, which is pinned to `core::church` by a
  // shared vector table (H-10). The local copies here were the seventh and eighth
  // implementations of the same two lines.
  const churchTitle = $derived(churchLabel(s.church));
  const visitChurch = (): void => openChurchSite(s.church, s.showToast);

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
      // that came from a field, which is precisely why Escape used to do nothing
      // while the reader was typing in one.
      case "Escape":
        if (shareApp) shareApp = false;
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

<div class="frame">
  <!-- One rem wide, and that is the whole of it: the browser's own default text
       size, as a box a ResizeObserver can watch. lib/uiScale.ts turns it and the
       reader's setting into `--uiScale`. -->
  <div class="rem-probe" aria-hidden="true" use:uiScale={readerScale}></div>
  <header>
    <span class="title">Plumbline</span>
    <span class="subtitle">{subtitle}</span>
    <!-- Destinations are first-class in the top bar (Compose bottom-nav
         parity: Read is the base layer, then Explore · Present · Memorize);
         the ≡ menu holds utilities only. Threads/Tags/Weaves live inside
         Explore, as on Android. -->
    <nav class="browse">
        <button onclick={go(() => (s.screen = "explore"))}>Explore</button>
        <button onclick={go(() => (s.showPresent = true))}>Present</button>
        <button
          onclick={go(() => {
            s.screen = "memorize";
            s.memorize = { view: "hub" };
          })}>Memorize</button
        >
        <button onclick={go(() => (s.screen = "hymnal"))}>Hymnal</button>
      </nav>
    <span class="spacer"></span>
    <button class="glass" class:searching={searchOpen} onclick={openSearch} aria-label="Open search">⌕</button>
    <input
      class="search"
      class:open={searchOpen}
      type="search"
      placeholder="Search or reference…"
      bind:this={searchEl}
      value={s.searchDraft}
      oninput={onSearchInput}
      onkeydown={(e) => e.key === "Escape" && closeSearch()}
      aria-label="Search"
    />
    {#if searchOpen}
      <button class="glass narrow-close" onclick={closeSearch} aria-label="Close search">✕</button>
    {/if}
    {#if s.intro}
      <!-- The welcome a reader was given, on demand: they should not have to
           reinstall to read it twice (feedback 2026-07-27). -->
      <button class="church-btn" onclick={go(() => (s.reopenIntro = s.intro))}>Welcome</button>
    {/if}
    {#if hasChurch(s.church)}
      <!-- Front and centre, not in Settings: someone handed this to a reader
           along with their church, and the reader should be able to find them
           without going hunting (feedback 2026-07-27). -->
      <button class="church-btn" onclick={visitChurch} title={churchTitle}>Church</button>
    {/if}
    <button class="share-first" onclick={go(() => (shareApp = true))}>Share</button>
    <div class="menu-host">
      <button class="menu-btn" onclick={() => (menuOpen = !menuOpen)} aria-label="Menu">≡</button>
      {#if menuOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div class="backdrop" onclick={() => (menuOpen = false)}></div>
        <div class="menu">
          <!-- UTILITIES ONLY, at every width. The destinations used to fold in
               here on narrow screens; they live in the bottom bar now, in thumb
               reach, which is where Android has always had them. -->
          <button onclick={go(() => (s.showHistory = true))}>History</button>
          <button onclick={go(() => (s.panel = { kind: "guide" }))}>Guide & about</button>
          <button onclick={go(() => (s.showShortcuts = true))}>Keyboard shortcuts</button>
          <button onclick={go(() => (s.showSettings = true))}>Settings</button>
        </div>
      {/if}
    </div>
  </header>

  <!-- One destination at a time, Android's model: a screen REPLACES the reader
       rather than sliding over it. The study panel still layers on top, because it
       is a panel about the verse you are looking at, not a place you go. -->
  <div class="body">
    {#if s.screen === "explore"}
      <ExploreScreen />
    {:else if s.screen === "memorize"}
      <MemorizeHost />
    {:else if s.screen === "hymnal"}
      <HymnalScreen />
    {:else}
      <div class="reading">
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
        <CanonStrip />
      </div>
    {/if}
    <StudyPanel />
  </div>

  <!-- THE BOTTOM BAR (narrow only) — Android's IA, in thumb reach: Read ·
       Explore · Present · Memorize. Android has had this since it shipped; the
       web folded the same four into the ≡ menu, which put the whole information
       architecture two taps away behind a glyph. The icons are the very same
       Material paths the Compose shell draws (apps/android/.../NavIcons.kt), so
       the two shells look like one product rather than two interpretations.

       Read is not a destination so much as the absence of one: the reader is
       always mounted underneath, so its tap just clears whatever is layered
       over it. -->
  <nav class="bottom-nav" aria-label="Destinations" bind:this={navEl}>
    {#each NAV as item (item.key)}
      <button
        class:on={dest === item.key}
        aria-current={dest === item.key ? "page" : undefined}
        onclick={go(item.go)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d={item.path} /></svg>
        <span>{item.label}</span>
      </button>
    {/each}
  </nav>
</div>

{#if shareApp}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="share-backdrop" onclick={() => (shareApp = false)}></div>
  <div
    class="share-dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Share Plumbline"
    data-surface="share app"
    use:modal={{ close: () => (shareApp = false) }}
  >
    <h2>Share Plumbline</h2>
    <p class="share-sub">
      {hasChurch(s.church)
        ? `Free, private, offline, no account required.`
        : "Free, offline, no account."}
    </p>
    <QrCode size={220} text={link} />
    <p class="share-url">plumblinebible.org</p>
    {#if s.intro}
      <!-- The welcome a reader was given, on demand: they should not have to
           reinstall to read it twice (feedback 2026-07-27). -->
      <button class="church-btn" onclick={go(() => (s.reopenIntro = s.intro))}>Welcome</button>
    {/if}
    {#if hasChurch(s.church)}
      <p class="share-with">with {s.church.name}</p>
    {/if}
    <div class="share-actions">
      <button class="share-primary" onclick={shareLink}>Share the link</button>
      <button onclick={() => (shareApp = false)}>Close</button>
    </div>
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
    <span>A new version is ready.</span>
    <button class="upd" onclick={() => s.applyUpdate()}>Update</button>
    <button class="dismiss" aria-label="Not now" onclick={() => (s.updateReady = false)}>✕</button>
  </div>
{/if}

<!-- This device would not take the write. Sticky, like the update notice and for
     a stronger reason: a fading toast would tell a reader their work is at risk
     while they are looking at the note they just typed. `title` carries the
     browser's own words for a bug report; the sentence carries what to do. -->
{#if s.persistFailed}
  <div class="toast warn" class:stacked={s.updateReady} role="status" title={s.persistFailed.detail}>
    <span>
      {s.persistFailed.retrying
        ? "Couldn't save your last change — trying again."
        : "Couldn't save your last change — storage may be full. Free some space, then try again."}
    </span>
    <button class="upd" onclick={() => s.retryPersist()}>Try again</button>
    <button class="dismiss" aria-label="Dismiss" onclick={() => (s.persistFailed = null)}>✕</button>
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
  /* The top bar was too small to use comfortably (feedback 2026-07-29). Android
     sets the standard here: 48dp touch targets and text you do not lean in for.
     Every control below is sized off that rather than off how little room it can
     be squeezed into — this bar holds the passage the reader taps most. */
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
  .title {
    font-weight: 600;
    letter-spacing: 0.03em;
    font-size: calc(18px * var(--uiScale, 1));
  }
  .subtitle {
    color: var(--faded, #8a8276);
    font-size: calc(16px * var(--uiScale, 1));
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
  .share-first {
    font-size: calc(15px * var(--uiScale, 1));
    padding: 9px 14px;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
    font-weight: 600;
  }
  .share-first:hover {
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
         Present used to be the exception, covering the whole chrome; it now stops
         above this bar instead (`--bottomNavH`, published from the measured
         height here), so nothing needs to out-stack it. Raising this to 70 was
         tried first and mutation-testing showed it changed nothing once the
         geometry was right — one mechanism, not two. */
      position: relative;
      z-index: 46;
      /* Clear of the home indicator / gesture bar on a notched phone. The one
         surface that always did this; it reads the shared variable now (app.css)
         so every surface says it the same way and a test can drive all of them. */
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
    /* One row where it fits, a second row where it does not.
       The glass stands in for the field until it's wanted, and while searching
       the field owns the row, so at the default text size this is one row.

       It used to be `flex-wrap: nowrap`, which does not mean "one row" — it
       means the row overflows and what runs off the end is the ≡, i.e. the way
       to Settings, on the narrowest phones (below ~340px it already did). Now
       that the chrome follows the reader's text size, someone reading at 36px
       reaches that width on any phone, and a control pushed off the screen is
       worse than a header two rows tall. Wrapping is the graceful version of
       the same overflow. */
    header {
      /* The air between controls gives before the controls do. With the app's
         name, Welcome, Church, Share, the glass and the ≡ all in one nowrap row,
         the 44px tap floor (app.css) costs 40px that a 360px phone has not got —
         and what runs off the end is the ≡, which is the way to Settings. At
         44px each they are their own separation and do not need 10px of it. */
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
    header:has(.search.open) .title,
    header:has(.search.open) .church-btn,
    header:has(.search.open) .share-first {
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
  }
  .menu-host {
    position: relative;
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
  .menu {
    position: absolute;
    right: 0;
    top: 100%;
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
  .panes > :global(.pane + .pane) {
    border-left: 1px solid var(--rule, #d8cba8);
  }
  .share-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 49;
  }
  .share-dialog {
    position: fixed;
    z-index: 50;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    background: #ffffff; /* fixed light — the QR needs its white field */
    color: #101010;
    border-radius: 14px;
    padding: 22px 26px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.3);
  }
  .share-dialog h2 {
    font-size: calc(18px * var(--uiScale, 1));
    font-weight: 600;
  }
  .share-sub,
  .share-url {
    color: #5a564e;
    font-size: calc(13px * var(--uiScale, 1));
  }
  .share-with {
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
    color: #9e7d38;
  }
  .church-btn {
    font-size: calc(13.5px * var(--uiScale, 1));
    padding: 4px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
    white-space: nowrap;
  }
  .church-btn:hover {
    border-color: var(--gold, #9e7d38);
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .share-actions {
    display: flex;
    gap: 10px;
    margin-top: 6px;
  }
  .share-actions button {
    padding: 5px 14px;
    border: 1px solid #d8cba8;
    border-radius: 6px;
  }
  .share-actions .share-primary {
    background: #9e7d38;
    color: #ffffff;
    border-color: #9e7d38;
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
     The number that was here before was measured off a 27px button, so the two
     notices overlapped the moment the floor applied to Update and ✕. */
  .toast.warn.stacked {
    bottom: calc(var(--toastBottom) + 44px + 16px + 12px);
  }
</style>
