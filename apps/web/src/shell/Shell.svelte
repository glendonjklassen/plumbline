<script lang="ts">
  // The app frame: header (browse buttons · live search · ≡ menu), pane row,
  // study surface, and global keyboard bindings. Wide screens follow the
  // desktop layout; narrow screens use the Compose-phone patterns.
  import ReaderPane from "../reader/ReaderPane.svelte";
  import StudyPanel from "../study/StudyPanel.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import FirstRun from "./FirstRun.svelte";
  import Shortcuts from "./Shortcuts.svelte";
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  const subtitle = $derived.by(() => {
    const p = s.panes[s.activePane];
    return p ? `${p.book} ${p.chapter} · 1769 KJV` : "";
  });

  function openWordStudy(refKey: string, tokenIndex: number): void {
    s.panel = { kind: "wordStudy", refKey, tokenIndex };
  }
  function pinWord(refKey: string, tokenIndex: number): void {
    // Weave-authoring pin (Full study) — lands with the authoring pass.
    void refKey;
    void tokenIndex;
  }

  // ── live search: per keystroke; empty query closes (manifest §Search) ──
  function onSearchInput(): void {
    if (s.searchQuery.trim()) s.panel = { kind: "search" };
    else if (s.panel?.kind === "search") s.panel = null;
  }

  function setTheme(theme: string): void {
    s.config.theme = theme;
    s.applyTheme();
    s.saveConfig();
  }
  function setMode(mode: "simple" | "full"): void {
    s.config.studyMode = mode;
    if (mode === "simple") s.panel = null;
    s.saveConfig();
  }

  let menuOpen = $state(false);
  function menu(action: () => void): () => void {
    return () => {
      menuOpen = false;
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
    const pane = s.panes[s.activePane];
    if (!pane) return;
    const fontPx = Number(s.config.bodySize ?? 18);
    const line = fontPx * 3;
    const page = 0.85 * (innerHeight - 120);
    const scroll = (dy: number, all = false) => {
      for (const p of all ? s.panes : [pane]) p.scrollY = Math.max(0, p.scrollY + dy);
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
        break;
      case "End":
        pane.scrollY = Number.MAX_SAFE_INTEGER; // pane clamps on next frame
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
        if (s.mapPopup) s.mapPopup = null;
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
    <span class="title">pure study</span>
    <span class="subtitle">{subtitle}</span>
    {#if s.full}
      <nav class="browse">
        <button onclick={() => (s.panel = { kind: "threads" })}>Threads</button>
        <button onclick={() => (s.panel = { kind: "tags" })}>Tags</button>
        <button onclick={() => (s.panel = { kind: "weaves" })}>Weaves</button>
      </nav>
    {/if}
    <span class="spacer"></span>
    <input
      class="search"
      type="search"
      placeholder="Search or reference…"
      bind:value={s.searchQuery}
      oninput={onSearchInput}
      aria-label="Search"
    />
    <div class="menu-host">
      <button class="menu-btn" onclick={() => (menuOpen = !menuOpen)} aria-label="Menu">≡</button>
      {#if menuOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div class="backdrop" onclick={() => (menuOpen = false)}></div>
        <div class="menu">
          <div class="group">Weave views</div>
          <button disabled={!s.full} onclick={menu(() => (s.panel = { kind: "suggested" }))}>Suggested</button>
          <button disabled={!s.full} onclick={menu(() => (s.mapPopup = { kind: "chord" }))}>Weave map</button>
          <button disabled={!s.full} onclick={menu(() => (s.mapPopup = { kind: "constellation" }))}>Constellation</button>
          <div class="group">Reading</div>
          <button class:checked={!s.full} onclick={menu(() => setMode("simple"))}>Simple reader</button>
          <button class:checked={s.full} onclick={menu(() => setMode("full"))}>Full study</button>
          <button
            class:checked={!!s.config.versePerLine}
            onclick={menu(() => {
              s.config.versePerLine = !s.config.versePerLine;
              s.saveConfig();
            })}>Verse per line</button
          >
          <div class="group">Theme</div>
          {#each ["light", "dark", "night", "system"] as t (t)}
            <button class:checked={(s.config.theme ?? "system") === t} onclick={menu(() => setTheme(t))}>
              {t === "system" ? "Follow system" : t[0].toUpperCase() + t.slice(1)}
            </button>
          {/each}
          <div class="group">Help</div>
          <button onclick={menu(() => (s.panel = { kind: "guide" }))}>Guide</button>
          <button onclick={menu(() => (s.showShortcuts = true))}>Keyboard shortcuts</button>
          <button onclick={menu(() => (s.panel = { kind: "about" }))}>About</button>
        </div>
      {/if}
    </div>
  </header>

  <div class="body">
    <div class="panes">
      {#each s.panes as _, i (i)}
        <ReaderPane paneIdx={i} onWordStudy={openWordStudy} onWordPin={pinWord} />
      {/each}
    </div>
    <StudyPanel />
  </div>
</div>

{#if s.toast}
  <div class="toast">{s.toast}</div>
{/if}
<PromptDialog />
<FirstRun />
<Shortcuts />

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
    z-index: 29;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 30;
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
  .menu .group {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faded, #8a8276);
    padding: 8px 8px 2px;
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
  .menu button.checked::after {
    content: " ✓";
    color: var(--gold, #9e7d38);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .panes {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  .panes > :global(.pane + .pane) {
    border-left: 1px solid var(--rule, #d8cba8);
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
