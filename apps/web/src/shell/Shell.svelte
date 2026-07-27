<script lang="ts">
  // The app frame: header (browse buttons · live search · ≡ menu), pane row,
  // study surface, and global keyboard bindings. Wide screens follow the
  // desktop layout; narrow screens use the Compose-phone patterns.
  import ReaderPane from "../reader/ReaderPane.svelte";
  import StudyPanel from "../study/StudyPanel.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import Shortcuts from "./Shortcuts.svelte";
  import BookNav from "./BookNav.svelte";
  import CanonStrip from "./CanonStrip.svelte";
  import HistorySheet from "./HistorySheet.svelte";
  import SettingsDialog from "./SettingsDialog.svelte";
  import MemorizeHost from "../memorize/MemorizeHost.svelte";
  import PresentHost from "../present/PresentHost.svelte";
  import ConnectorsOverlay from "./ConnectorsOverlay.svelte";
  import MapsHost from "../maps/MapsHost.svelte";
  import ContextMenu from "../reader/ContextMenu.svelte";
  import TagPicker from "../study/TagPicker.svelte";
  import TagWeave from "../study/TagWeave.svelte";
  import QrCode, { PWA_URL } from "./QrCode.svelte";
  import { hasChurch, shareUrl } from "./church";
  import { getSession } from "../state/session.svelte";

  const s = getSession();

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
  // Share the app: the PWA QR + link (Compose ShareAppDialog parity) — a
  // first-class header button (2026-07-26), not a menu trip.
  let shareApp = $state(false);
  // What we actually hand over: the app, plus this reader's church when they
  // have set one (Settings → Your church). One QR, both things.
  const link = $derived(shareUrl(PWA_URL, s.church));
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
    <input
      class="search"
      type="search"
      placeholder="Search or reference…"
      bind:value={s.searchQuery}
      oninput={onSearchInput}
      aria-label="Search"
    />
    <button class="share-first" onclick={go(() => (shareApp = true))}>Share</button>
    <div class="menu-host">
      <button class="menu-btn" onclick={() => (menuOpen = !menuOpen)} aria-label="Menu">≡</button>
      {#if menuOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div class="backdrop" onclick={() => (menuOpen = false)}></div>
        <div class="menu">
          <!-- Narrow screens: the header destinations fold in here. -->
          <button class="narrow-only" onclick={go(() => (s.panel = { kind: "explore" }))}>Explore</button>
          <button class="narrow-only" onclick={go(() => (s.showPresent = true))}>Present</button>
          <button class="narrow-only" onclick={go(() => (s.memorize = { view: "hub" }))}>Memorize</button>
          <div class="menu-rule narrow-only"></div>
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
    <p class="share-url">{link}</p>
    <div class="share-actions">
      <button class="share-primary" onclick={shareLink}>Share the link</button>
      <button onclick={() => (shareApp = false)}>Close</button>
    </div>
  </div>
{/if}

{#if s.toast}
  <div class="toast">{s.toast}</div>
{/if}
<PromptDialog />
<Shortcuts />
<MapsHost />
<ContextMenu />
<TagPicker />
<TagWeave />
<MemorizeHost />
<HistorySheet />
<PresentHost />
<SettingsDialog />
<BookNav />

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
  .menu .narrow-only,
  .menu-rule {
    display: none;
  }
  @media (max-width: 700px) {
    .browse,
    .subtitle {
      display: none;
    }
    .menu .narrow-only {
      display: block;
    }
    .menu .menu-rule {
      display: block;
      border-top: 1px solid var(--rule, #d8cba8);
      margin: 4px 2px;
    }
  }
  .spacer {
    flex: 1;
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
</style>
