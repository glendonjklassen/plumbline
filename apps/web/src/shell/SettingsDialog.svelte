<script lang="ts">
  // One Settings dialog (Android IA): analysis switches, theme, text size /
  // margin / line-spacing sliders, copy format, bundled stock set.
  import { getSession } from "../state/session.svelte";
  import { zipRead, zipWrite } from "../engine/zip";
  import { idbApply } from "../engine/idb";
  import { nowStamp } from "../engine/StudyEngine";

  const s = getSession();

  // ── backup / restore: the authored home dirs as a zip, the same layout the
  //    Android backup writes — one archive restores across devices. ──────────
  const BACKUP_DIRS = ["tags/", "threads/", "weaves/", "notes/", "memory/", ".config/"];

  // Archives written before the Plumbline rename carry the config under
  // ".config/pure-study/"; the live home reads ".config/plumbline/". Remapped on
  // restore only — nothing ever writes the old name back. Without this an older
  // backup silently drops the user's settings (the authored dirs above are
  // unaffected: their names never moved).
  const LEGACY_CONFIG = ".config/pure-study/";
  const currentConfigPath = (p: string): string =>
    p.startsWith(LEGACY_CONFIG) ? ".config/plumbline/" + p.slice(LEGACY_CONFIG.length) : p;

  function backup(): void {
    const files = s.home.exportUserData();
    files.set(
      "plumbline-backup.json",
      new TextEncoder().encode(JSON.stringify({ format: 1, app: "web", exported: nowStamp() })),
    );
    const blob = new Blob([zipWrite(files) as unknown as BlobPart], { type: "application/zip" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `plumbline-backup-${nowStamp().slice(0, 10)}.zip`;
    a.click();
    URL.revokeObjectURL(a.href);
    s.showToast(`Backed up ${files.size - 1} files`);
  }

  async function restore(e: Event): Promise<void> {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    try {
      const entries = await zipRead(new Uint8Array(await file.arrayBuffer()));
      // Only home-relative authored paths — no traversal, nothing else.
      const safe = new Map<string, Uint8Array>();
      for (const [path, bytes] of entries)
        if (BACKUP_DIRS.some((d) => path.startsWith(d)) && !path.includes(".."))
          safe.set(currentConfigPath(path), bytes);
      if (safe.size === 0) {
        s.showToast("No study data found in that zip");
        return;
      }
      // The restored files are now the truth — nothing (incl. the pagehide
      // flush) may persist the current session over them; just reload.
      s.restoring = true;
      s.home.freeze(); // the debounced authoring persist must not fire either
      await idbApply("user", safe);
      location.reload(); // the engine re-opens over the restored home
    } catch (err) {
      s.showToast(err instanceof Error ? err.message : String(err));
    }
  }

  function toggleGate(key: "humanAnalysis" | "machineAnalysis"): void {
    s.config[key] = s.config[key] === false;
    s.saveConfig();
  }
  function setTheme(theme: string): void {
    s.config.theme = theme;
    s.applyTheme();
    s.saveConfig();
  }
  function setNum(key: "bodySize" | "sideMargin" | "lineSpacing", v: number): void {
    s.config[key] = v;
    s.saveConfig();
  }
  async function toggleBundled(): Promise<void> {
    await s.home.setBundled(!s.home.bundledOn);
    s.flushConfig();
    location.reload(); // the engine re-opens with/without the stock set
  }

  const themes = [
    ["system", "Follow system"],
    ["light", "Light"],
    ["dark", "Dark"],
    ["night", "Night (true black)"],
  ] as const;
  const copyOpts = [
    ["verse", "Verse text only"],
    ["verseRef", "Verse with reference"],
    ["verseMarkdown", "Markdown blockquote"],
  ] as const;
</script>

{#if s.showSettings}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.showSettings = false)}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    <h2>Settings</h2>
    <div class="content">
      <label class="toggle">
        <span class="body">
          <span class="name">Scholars' analysis</span>
          <span class="desc">Renderings, morphology, same-root, treasury cross-references.</span>
        </span>
        <input
          type="checkbox"
          checked={s.config.humanAnalysis !== false}
          onchange={() => toggleGate("humanAnalysis")}
        />
      </label>
      <label class="toggle">
        <span class="body">
          <span class="name">Machine analysis</span>
          <span class="desc">Similar concepts, appears-alongside, verses-like-this, concept maps.</span>
        </span>
        <input
          type="checkbox"
          checked={s.config.machineAnalysis !== false}
          onchange={() => toggleGate("machineAnalysis")}
        />
      </label>
      <label class="toggle">
        <span class="body">
          <span class="name">Verse per line</span>
          <span class="desc">Start every verse on a fresh line.</span>
        </span>
        <input
          type="checkbox"
          checked={!!s.config.versePerLine}
          onchange={() => {
            s.config.versePerLine = !s.config.versePerLine;
            s.saveConfig();
          }}
        />
      </label>
      <hr />
      <p class="label">Theme</p>
      {#each themes as [token, label] (token)}
        <label class="radio">
          <input
            type="radio"
            name="theme"
            checked={(s.config.theme ?? "system") === token}
            onchange={() => setTheme(token)}
          />
          {label}
        </label>
      {/each}
      <hr />
      <p class="label">Text size — reader & study</p>
      <p class="aa" style:font-size="{Number(s.config.bodySize ?? 18)}px">Aa</p>
      <input
        type="range"
        min="12"
        max="40"
        value={Number(s.config.bodySize ?? 18)}
        oninput={(e) => setNum("bodySize", Number((e.target as HTMLInputElement).value))}
      />
      <p class="label">Margin — space either side of the text</p>
      <input
        type="range"
        min="8"
        max="96"
        value={Number(s.config.sideMargin ?? 28)}
        oninput={(e) => setNum("sideMargin", Number((e.target as HTMLInputElement).value))}
      />
      <p class="label">Line spacing</p>
      <input
        type="range"
        min="1"
        max="2.2"
        step="0.05"
        value={Number(s.config.lineSpacing ?? 1.35)}
        oninput={(e) => setNum("lineSpacing", Number((e.target as HTMLInputElement).value))}
      />
      <hr />
      <p class="label">Copy format</p>
      {#each copyOpts as [token, label] (token)}
        <label class="radio">
          <input
            type="radio"
            name="copystyle"
            checked={(s.config.copyStyle ?? "verseRef") === token}
            onchange={() => {
              s.config.copyStyle = token;
              s.saveConfig();
            }}
          />
          {label}
        </label>
      {/each}
      <hr />
      <label class="toggle">
        <span class="body">
          <span class="name">Bundled study set</span>
          <span class="desc">Ship-with-app threads, tags, and weaves (reloads the app).</span>
        </span>
        <input type="checkbox" checked={s.home.bundledOn} onchange={toggleBundled} />
      </label>
      <hr />
      <p class="label">Your study data — notes, tags, threads, weaves, memorization</p>
      <div class="row">
        <button class="action" onclick={backup}>Back up (.zip)</button>
        <label class="action">
          Restore from backup…
          <input type="file" accept=".zip,application/zip" onchange={restore} hidden />
        </label>
      </div>
      <p class="desc-note">
        The same zip restores on Android and the web. Restoring replaces items with the same
        name; everything else is kept.
      </p>
    </div>
    <button class="done" onclick={() => (s.showSettings = false)}>Done</button>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 40;
  }
  .dialog {
    position: fixed;
    z-index: 41;
    top: 8vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, 94vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    padding: 18px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    gap: 10px;
  }
  h2 {
    font-size: 17px;
    font-weight: 600;
  }
  .content {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
  }
  .toggle .body {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .toggle .name {
    font-size: 15px;
  }
  .toggle .desc {
    font-size: 12px;
    color: var(--faded, #8a8276);
  }
  .toggle input,
  .radio input {
    accent-color: var(--gold, #9e7d38);
    width: 17px;
    height: 17px;
  }
  hr {
    border: none;
    border-top: 1px solid var(--rule, #d8cba8);
    margin: 6px 0;
  }
  .label {
    font-size: 12px;
    color: var(--faded, #8a8276);
  }
  .aa {
    line-height: 1;
  }
  input[type="range"] {
    accent-color: var(--gold, #9e7d38);
  }
  .radio {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14.5px;
    cursor: pointer;
    padding: 2px 0;
  }
  .done {
    align-self: flex-end;
    padding: 6px 18px;
    border: 1px solid var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
    border-radius: 7px;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .action {
    padding: 5px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    cursor: pointer;
    font-size: 14px;
  }
  .action:hover {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
  }
  .desc-note {
    font-size: 11.5px;
    color: var(--faded, #8a8276);
  }
</style>
