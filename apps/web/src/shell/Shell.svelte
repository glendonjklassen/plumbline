<script lang="ts">
  // The app frame: header (browse buttons · search · ≡ menu), the pane row,
  // and the study surface. Wide screens get the desktop layout (panes +
  // 380px sidebar); narrow screens get the Compose-phone patterns (single
  // pane, bottom-sheet study surface).
  import ReaderPane from "../reader/ReaderPane.svelte";
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  const subtitle = $derived(() => {
    const p = s.panes[s.activePane];
    return p ? `${p.book} ${p.chapter} · 1769 KJV` : "";
  });

  function openWordStudy(refKey: string, tokenIndex: number): void {
    s.panel = { kind: "wordStudy", refKey, tokenIndex };
  }
  function pinWord(refKey: string, tokenIndex: number): void {
    // Weave-authoring pin (Full study; wired in the authoring pass).
    void refKey;
    void tokenIndex;
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
</script>

<div class="frame">
  <header>
    <span class="title">pure study</span>
    <span class="subtitle">{subtitle()}</span>
    <span class="spacer"></span>
    <div class="menu-host">
      <button class="menu-btn" onclick={() => (menuOpen = !menuOpen)} aria-label="Menu">≡</button>
      {#if menuOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div class="backdrop" onclick={() => (menuOpen = false)}></div>
        <div class="menu">
          <div class="group">Reading</div>
          <button class:checked={!s.full} onclick={() => setMode("simple")}>Simple reader</button>
          <button class:checked={s.full} onclick={() => setMode("full")}>Full study</button>
          <button
            class:checked={!!s.config.versePerLine}
            onclick={() => {
              s.config.versePerLine = !s.config.versePerLine;
              s.saveConfig();
            }}>Verse per line</button
          >
          <div class="group">Theme</div>
          {#each ["light", "dark", "night", "system"] as t (t)}
            <button class:checked={(s.config.theme ?? "system") === t} onclick={() => setTheme(t)}>
              {t === "system" ? "Follow system" : t[0].toUpperCase() + t.slice(1)}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </header>

  <div class="panes">
    {#each s.panes as _, i (i)}
      <ReaderPane paneIdx={i} onWordStudy={openWordStudy} onWordPin={pinWord} />
    {/each}
  </div>
</div>

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
    gap: 12px;
    padding: 6px 12px;
    background: var(--paneNavBg, #efeae1);
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .title {
    font-weight: 600;
    letter-spacing: 0.03em;
  }
  .subtitle {
    color: var(--faded, #8a8276);
    font-size: 14px;
  }
  .spacer {
    flex: 1;
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
    z-index: 9;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 10;
    min-width: 180px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.12);
    padding: 6px;
    display: flex;
    flex-direction: column;
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
  .menu button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .menu button.checked::after {
    content: " ✓";
    color: var(--gold, #9e7d38);
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .panes > :global(.pane + .pane) {
    border-left: 1px solid var(--rule, #d8cba8);
  }
</style>
