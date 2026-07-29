<script lang="ts">
  // The app frame: header (browse buttons · live search · ≡ menu), pane row,
  // study surface, and global keyboard bindings. Wide screens follow the
  // desktop layout; narrow screens use the Compose-phone patterns.
  import ReaderPane from "../reader/ReaderPane.svelte";
  import StudyPanel from "../study/StudyPanel.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import Shortcuts from "./Shortcuts.svelte";
  import BookNav from "./BookNav.svelte";
  import MarkReadDialog from "./MarkReadDialog.svelte";
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
  import { hasChurch, safeChurchUrl } from "./church";
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
        if (s.showPresent || s.memorize || s.mapPopup) return null;
        const p = s.panes[0];
        return p ? { book: p.book, chapter: p.chapter } : null;
      },
      reached: () => s.panes[0]?.reached ?? 0,
      record: (book, chapter, reached, seconds) =>
        s.rpc.call("readingRecord", book, chapter, reached, seconds, new Date().toISOString()),
      spec: () =>
        s.rpc
          .call("readingBooks", new Date().toISOString())
          .then((r: any) => r?.spec ?? null)
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
  function onSearchInput(): void {
    if (s.searchQuery.trim()) s.panel = { kind: "search" };
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
    s.searchQuery = "";
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
  function go(action: () => void): () => void {
    return () => {
      menuOpen = false;
      s.memorize = null;
      s.showHistory = false;
      s.showSettings = false;
      s.bookNavFor = null;
      // A destination tap also dismisses the fullscreen maps — on the chord
      // map, hitting Explore previously did nothing (feedback 2026-07-26).
      s.mapPopup = null;
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
      // Read clears the layers rather than opening anything: `go` already drops
      // memorize, history, settings and the maps, so this only has to close the
      // study panel and Present.
      go: () => {
        s.showPresent = false;
        s.panel = null;
      },
    },
    {
      key: "explore",
      label: "Explore",
      path:
        "M12 10.9c-.61 0-1.1.49-1.1 1.1s.49 1.1 1.1 1.1c.61 0 1.1-.49 1.1-1.1s-.49-1.1-1.1-1.1z" +
        "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z" +
        "m2.19 12.19L6 18l3.81-8.19L18 6l-3.81 8.19z",
      go: () => {
        s.showPresent = false;
        s.panel = { kind: "explore" };
      },
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
      // `go` clears s.memorize first, so this has to set it after.
      go: () => {
        s.showPresent = false;
        s.memorize = { view: "hub" };
      },
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

  const dest = $derived(
    s.showPresent ? "present" : s.memorize ? "memorize" : s.panel?.kind === "explore" ? "explore" : "read",
  );

  // The church button opens their site; with no site to open it at least
  // tells the reader who and when, which is all we were given.
  const churchTitle = $derived(
    [s.church.name, s.church.info].filter(Boolean).join(" — ") || "Your church",
  );
  function visitChurch(): void {
    const url = safeChurchUrl(s.church.url);
    if (url) window.open(url, "_blank", "noopener,noreferrer");
    else s.showToast(churchTitle);
  }

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
      case "Escape":
        if (shareApp) shareApp = false;
        else if (s.mapPopup) s.mapPopup = null;
        else if (s.bookNavFor !== null) s.bookNavFor = null;
        else if (s.showSettings) s.showSettings = false;
        else if (s.memorize) s.memorize = null;
        else if (s.showHistory) s.showHistory = false;
        else if (s.showShortcuts) s.showShortcuts = false;
        else if (s.panel) {
          s.panel = null;
          s.searchQuery = "";
        }
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
  <header>
    <span class="title">Plumbline</span>
    <span class="subtitle">{subtitle}</span>
    <!-- Destinations are first-class in the top bar (Compose bottom-nav
         parity: Read is the base layer, then Explore · Present · Memorize);
         the ≡ menu holds utilities only. Threads/Tags/Weaves live inside
         Explore, as on Android. -->
    <nav class="browse">
        <button onclick={go(() => (s.panel = { kind: "explore" }))}>Explore</button>
        <button onclick={go(() => (s.showPresent = true))}>Present</button>
        <button onclick={go(() => (s.memorize = { view: "hub" }))}>Memorize</button>
      </nav>
    <span class="spacer"></span>
    <button class="glass" class:searching={searchOpen} onclick={openSearch} aria-label="Open search">⌕</button>
    <input
      class="search"
      class:open={searchOpen}
      type="search"
      placeholder="Search or reference…"
      bind:this={searchEl}
      bind:value={s.searchQuery}
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

  <div class="body">
    <div class="reading">
      <div class="panes">
        {#each s.panes as _, i (i)}
          <ReaderPane paneIdx={i} onWordStudy={openWordStudy} />
        {/each}
        <ConnectorsOverlay />
      </div>
      <CanonStrip />
    </div>
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
  <div class="share-dialog" role="dialog" aria-modal="true" aria-label="Share Plumbline">
    <h2>Share Plumbline</h2>
    <p class="share-sub">
      {hasChurch(s.church)
        ? `Free, offline, no account — and your church's details travel with it.`
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

{#if s.toast}
  <div class="toast">{s.toast}</div>
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
<PromptDialog />
<Shortcuts />
<MapsHost />
<ContextMenu />
<TagPicker />
<TagWeave />
<MemorizeHost />
<PassagePicker />
<HistorySheet />
<PresentHost />
<SettingsDialog />
<BookNav />
<MarkReadDialog />
<ThreadPicker />

<style>
  .frame {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
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
  }
  .subtitle {
    color: var(--faded, #8a8276);
    font-size: 14px;
  }
  .browse {
    display: flex;
    gap: 2px;
    margin-left: 8px;
  }
  .browse button {
    font-size: 13.5px;
    padding: 3px 9px;
    border-radius: 5px;
    color: var(--gold, #9e7d38);
  }
  .browse button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .share-first {
    font-size: 13.5px;
    padding: 4px 12px;
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
      /* Clear of the home indicator / gesture bar on a notched phone. */
      padding-bottom: env(safe-area-inset-bottom, 0px);
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
      font-size: 11px;
      letter-spacing: 0.01em;
    }
    .browse,
    .subtitle {
      display: none;
    }
    /* One row, always: the glass stands in for the field until it's wanted,
       and while searching the field owns the row. */
    header {
      flex-wrap: nowrap;
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
    font-size: 20px;
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
    font-size: 14px;
  }
  .menu-host {
    position: relative;
  }
  .menu-btn {
    font-size: 20px;
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
    font-size: 18px;
    font-weight: 600;
  }
  .share-sub,
  .share-url {
    color: #5a564e;
    font-size: 13px;
  }
  .share-with {
    font-size: 13px;
    font-weight: 600;
    color: #9e7d38;
  }
  .church-btn {
    font-size: 13.5px;
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
    position: fixed;
    bottom: 22px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 50;
    background: var(--ink, #211f1a);
    color: var(--paper, #fcf9f4);
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 14px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.25);
  }
  /* The update toast stays until acted on, so it carries its own buttons. */
  .toast.update {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px 8px 16px;
  }
  .toast.update .upd {
    background: var(--gold, #9e7d38);
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 5px 12px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
  }
  .toast.update .dismiss {
    background: none;
    border: none;
    color: inherit;
    opacity: 0.7;
    font-size: 14px;
    cursor: pointer;
  }
</style>
