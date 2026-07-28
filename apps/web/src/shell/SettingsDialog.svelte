<script lang="ts">
  // One Settings dialog (Android IA): analysis switches, theme, text size /
  // margin / line-spacing sliders, copy format, bundled stock set.
  import { getSession } from "../state/session.svelte";
  import { completeOffline, surveyOffline, type OfflineSurvey } from "../engine/offline";
  import { cleanChurch } from "./church";
  import { PERF } from "../engine/perf";
  import type { WorkerDiagnostics } from "../engine/worker-client";
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

  async function backup(): Promise<void> {
    const files = new Map<string, Uint8Array>(await s.rpc.exportUserData());
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
      await s.rpc.freeze(); // the debounced authoring persist must not fire either
      await idbApply("user", safe);
      location.reload(); // the engine re-opens over the restored home
    } catch (err) {
      s.showToast(err instanceof Error ? err.message : String(err));
    }
  }

  function toggleGate(key: "humanAnalysis" | "machineAnalysis"): void {
    s.config[key] = s.config[key] === false;
    s.saveConfig();
    // Machine tier switched on: pull the deferred R&D pack in (no-op if the
    // idle path already did).
    if (key === "machineAnalysis" && s.config[key] !== false) void s.ensureRnd();
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
    await s.rpc.setBundled(!s.bundledOn);
    s.flushConfig();
    location.reload(); // the engine re-opens with/without the stock set
  }

  // ── the reader's home church ────────────────────────────────────────────
  // Set it here, and every link this reader shares carries it, so a QR handed
  // out at a service leads back to that service (2026-07-27).
  let churchName = $state("");
  let churchInfo = $state("");
  let churchUrl = $state("");
  let churchLoaded = false;
  $effect(() => {
    if (!s.showSettings || churchLoaded) return;
    churchLoaded = true;
    const c = s.church;
    churchName = c.name;
    churchInfo = c.info;
    churchUrl = c.url;
  });
  function saveChurch(): void {
    s.setChurch(cleanChurch({ name: churchName, info: churchInfo, url: churchUrl }));
  }

  // ── offline completeness ────────────────────────────────────────────────
  // The reader's answer to "will this work with no signal?" — and the repair
  // when it wouldn't have.
  let offline = $state<OfflineSurvey | null>(null);
  let offlineBusy = $state(false);
  let offlineProgress = $state(0);
  const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;
  // "On this device" is a claim about BYTES, and only about bytes. It used to
  // also require `rndState === "ready"`, which is a claim about whether this
  // SESSION has finished preparing what it already downloaded — so a phone
  // holding every byte was told "Still to download: the analysis pack" for the
  // whole time the engine was busy parsing it (feedback 2026-07-28, with a
  // screenshot, on a launch whose own trace showed the analysis pack coming off
  // the device in 115 ms). Downloading and preparing are different waits and the
  // reader is owed the difference.
  const offlineComplete = $derived(!!offline && offline.missing.length === 0);
  /** What is still missing, in a sentence — the text itself never is. */
  const offlineSummary = $derived.by(() => {
    const files = offline?.missing.length ?? 0;
    if (files) {
      return `The Holy Bible is already on this device. Still to download: ${files} data file${
        files === 1 ? "" : "s"
      } (${mb(offline!.missingBytes)}).`;
    }
    return "The Holy Bible is already on this device. Still to download: nothing.";
  });
  /** Preparing is not downloading; say so separately or not at all. */
  const preparingNote = $derived(
    offlineComplete && s.rndState !== "ready"
      ? "The analysis is already downloaded and is being prepared — that finishes on its own."
      : "",
  );

  $effect(() => {
    if (s.showSettings && !offlineBusy) void surveyOffline().then((r) => (offline = r));
  });

  async function downloadEverything(): Promise<void> {
    offlineBusy = true;
    offlineProgress = 0;
    try {
      // The machine tier first: it is the piece deliberately left out on
      // phones, and loading it also puts its files in the cache.
      if (s.rndState !== "ready") await s.ensureRnd();
      offline = await completeOffline((f) => (offlineProgress = f));
    } catch {
      s.showToast("Couldn't finish downloading — check your connection.");
    } finally {
      offlineBusy = false;
      offlineProgress = 0;
    }
  }

  // Fresh timings whenever the dialog opens: the background stages keep
  // appending to the boot trace (Strong's, warm steps, the analysis pack),
  // and the turn split describes whichever chapter was last laid out — turn
  // a few pages, then open this.
  let diag = $state<WorkerDiagnostics | null>(null);
  let copied = $state(false);
  $effect(() => {
    if (!PERF || !s.showSettings) return;
    // ONE round trip, so the trace, the stall total and the per-file costs are
    // all from the same instant. Three separate reads drift while the background
    // load is still running, which is exactly when someone is reading this.
    void s.rpc.diagnostics().then((d) => {
      diag = d;
      s.bootTrace = d.trace;
      s.turnTrace = d.turn;
    });
  });

  /** kilobytes, counts and milliseconds all shared one " ms" suffix, so the
   *  panel confidently reported `home evict after open (KB)  36367 ms`. */
  function unitOf(label: string): string {
    if (/\(KB\)$/.test(label)) return " KB";
    if (/^(worker font faces|items|wasm→JS)/.test(label)) return "";
    return " ms";
  }

  /** The whole diagnostic picture as plain text, for pasting into a bug report.
   *  Screenshots of this panel cost a round trip every time and cut off exactly
   *  the rows that mattered (2026-07-28). */
  function report(): string {
    const nav = navigator as any;
    const c = nav.connection ?? {};
    const L: string[] = [];
    L.push(`Plumbline ${__APP_VERSION__} · build ${__BUILD_ID__} · engine ${s.engineVersion}`);
    L.push(`data pack ${diag?.packVersion ?? "?"}${diag?.fromPin ? " (warm: stage 1 off the device)" : ""}`);
    L.push("");
    L.push("DEVICE");
    L.push(`  ua              ${navigator.userAgent}`);
    // The browser's OWN estimate. Recorded so nobody ever again derives a
    // connection speed by dividing a byte count by a wall clock that was mostly
    // this thread being busy.
    L.push(`  connection      ${c.effectiveType ?? "?"} · downlink ${c.downlink ?? "?"} Mbps · rtt ${c.rtt ?? "?"} ms · saveData ${c.saveData ?? false}`);
    L.push(`  cpu threads     ${navigator.hardwareConcurrency ?? "?"}`);
    L.push(`  device memory   ${nav.deviceMemory ?? "?"} GB`);
    L.push(`  screen          ${screen.width}x${screen.height} @${devicePixelRatio}`);
    L.push(`  storage used    ${offline?.bytesOnDevice ? mb(offline.bytesOnDevice) : "?"} · persisted ${offline?.persisted ?? "?"}`);
    L.push(`  pack files      ${offline?.totalFiles ?? "?"} · missing ${offline?.missing.length ?? "?"}`);
    if (diag) {
      L.push("");
      L.push("ENGINE THREAD UNAVAILABLE (the stall meter)");
      L.push(`  total           ${Math.round(diag.stall.totalMs)} ms across ${diag.stall.count} stalls`);
      L.push(`  worst single    ${Math.round(diag.stall.worstMs)} ms`);
      // Reported, and EXCLUDED from the numbers above. A hidden tab has its
      // timers and its downloads frozen by the browser, and counting that as
      // engine work invented a 25-second stall on a launch that did none.
      L.push(`  page hidden     ${Math.round(diag.stall.hiddenMs)} ms (excluded above)`);
      L.push("  (time this thread could not answer a tap, a layout, OR its own downloads)");
      if (diag.packFiles.length) {
        L.push("");
        L.push("PACK FILES        ours = wall clock · net = the browser's own timing");
        for (const f of diag.packFiles) {
          const net = f.netMs == null ? "     -" : `${f.netMs}`.padStart(6);
          L.push(
            `  ${f.from.padEnd(7)} ours ${String(f.ms).padStart(6)} ms · net ${net} ms  ${(f.gzBytes / 1024).toFixed(0).padStart(6)} KB  ${f.path}`,
          );
        }
      }
      L.push("");
      L.push("BOOT");
      for (const [k, v] of diag.trace) L.push(`  ${k.padEnd(34)} ${v}${unitOf(k)}`);
      if (diag.turn.length) {
        L.push("");
        L.push("LAST CHAPTER TURN");
        for (const [k, v] of diag.turn) L.push(`  ${k.padEnd(34)} ${v}${unitOf(k)}`);
      }
    }
    return L.join("\n");
  }

  async function copyDiagnostics(): Promise<void> {
    const text = report();
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // Clipboard needs a secure context and a user gesture, and some phone
      // browsers refuse anyway. Falling back to a share sheet beats a dead
      // button, and failing that the reader can still select the text.
      try {
        await navigator.share?.({ text });
      } catch {
        s.showToast("Couldn't copy — select the text and copy it by hand.");
      }
    }
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
      {#if s.akjvAvailable}
        <!-- A reading aid over the SAME text, not a version picker: the words
             stay the KJV's everywhere it matters (memorize, Present, copy,
             share), and every marked word tells you what it replaced. -->
        <label class="toggle">
          <span class="body">
            <span class="name">Plain-English overlay</span>
            <span class="desc">
              Show where the American King James Version words a verse differently — marked with a
              dotted underline; tap one to see the KJV word it replaced.
            </span>
          </span>
          <input
            type="checkbox"
            checked={s.config.akjvOverlay === true}
            onchange={(e) => void s.setAkjvOverlay(e.currentTarget.checked)}
          />
        </label>
      {/if}
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
      {#if s.config.machineAnalysis !== false && s.rndState !== "ready"}
        <div class="rnd-status">
          {#if s.rndState === "loading"}
            <span>
              {s.rndPreparing
                ? "Preparing the analysis…"
                : `Downloading the analysis pack — ${Math.round(s.rndProgress * 100)}%`}
            </span>
          {:else}
            <span>Analysis pack not downloaded (~4 MB).</span>
            <button class="rnd-now" onclick={() => void s.ensureRnd()}>Download now</button>
          {/if}
        </div>
      {/if}
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
        <input type="checkbox" checked={s.bundledOn} onchange={toggleBundled} />
      </label>
      <hr />
      <p class="label">Your church</p>
      <p class="desc-note">
        Added to the links and QR codes you share, so whoever you hand this to also gets your
        church. Leave it blank to share the Bible on its own.
      </p>
      <input class="field" placeholder="Church name" bind:value={churchName} onchange={saveChurch} />
      <input
        class="field"
        placeholder="When and where — e.g. Sundays 10am, 12 Long Street"
        bind:value={churchInfo}
        onchange={saveChurch}
      />
      <input class="field" placeholder="Website" bind:value={churchUrl} onchange={saveChurch} />
      <label class="toggle">
        <span class="body">
          <span class="name">Present shares open for a new believer</span>
          <span class="desc">
            A link shared from the Present screen opens on the new-believer welcome, since that
            screen is what you show someone face to face. Your ordinary Share stays a plain link.
          </span>
        </span>
        <input
          type="checkbox"
          checked={s.config.presentSharesAsNew !== false}
          onchange={() => {
            s.config.presentSharesAsNew = s.config.presentSharesAsNew === false;
            s.saveConfig();
          }}
        />
      </label>
      <hr />
      <p class="label">Offline</p>
      <div class="offline">
        {#if offlineBusy}
          <span class="off-note">
            {s.rndPreparing
              ? "Preparing the analysis…"
              : `Downloading — ${Math.round((s.rndState === "ready" ? offlineProgress : s.rndProgress) * 100)}%`}
          </span>
          <div class="off-bar">
            <div
              class="off-fill"
              style:width={`${(s.rndState === "ready" ? offlineProgress : s.rndProgress) * 100}%`}
            ></div>
          </div>
        {:else if offlineComplete}
          <span class="off-ok">Everything is on this device ✓</span>
          <span class="off-note">
            Plumbline works with no connection at all.{offline?.bytesOnDevice
              ? ` Using ${mb(offline.bytesOnDevice)}.`
              : ""}
          </span>
          {#if preparingNote}
            <span class="off-note">{preparingNote}</span>
          {/if}
          <!-- "It is all downloaded" and "it will still be there" are different
               claims, and only the first one is ours to make: browsers evict
               storage under pressure. Say which of the two is true. -->
          {#if offline?.persisted === false}
            <span class="off-note">
              Your browser may still clear it if the device runs low on space. Installing Plumbline
              to your home screen usually makes it permanent.
            </span>
          {:else if offline?.persisted}
            <span class="off-note">Marked permanent — your browser won't clear it to save space.</span>
          {/if}
        {:else}
          <span class="off-note">{offlineSummary}</span>
          <button class="off-go" onclick={downloadEverything}>Download everything</button>
        {/if}
      </div>
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
      {#if PERF && s.bootTrace.length}
        <hr />
        <details class="diag">
          <summary>Boot diagnostics — this device</summary>
          <button class="action copy-diag" onclick={copyDiagnostics}>
            {copied ? "Copied ✓" : "Copy diagnostics"}
          </button>
          {#if diag}
            <p class="diag-sub">Engine thread unavailable</p>
            <table>
              <tbody>
                <tr>
                  <td>total, across {diag.stall.count} stalls</td>
                  <td class="ms">{Math.round(diag.stall.totalMs)} ms</td>
                </tr>
                <tr><td>worst single stall</td><td class="ms">{Math.round(diag.stall.worstMs)} ms</td></tr>
                <tr><td>page hidden (not counted)</td><td class="ms">{Math.round(diag.stall.hiddenMs)} ms</td></tr>
              </tbody>
            </table>
            <p class="diag-note">
              Time the engine could not answer a tap, a layout, or its own downloads. Time with the
              screen off or the tab in the background is excluded — the browser freezes both the
              engine and its downloads then, and counting it would read as a fault.
            </p>
          {/if}
          {#if diag?.packFiles.length}
            <p class="diag-sub">Pack files</p>
            <table>
              <tbody>
                {#each diag.packFiles as f, i (i)}
                  <tr>
                    <td>{f.from === "depot" ? "on device" : "downloaded"} · {f.path}</td>
                    <td class="ms">{f.ms} ms{f.netMs == null ? "" : ` · net ${f.netMs}`}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
          <p class="diag-sub">Boot</p>
          <table>
            <tbody>
              {#each s.bootTrace as [stage, n], i (i)}
                <tr><td>{stage}</td><td class="ms">{n}{unitOf(stage)}</td></tr>
              {/each}
            </tbody>
          </table>
          {#if s.turnTrace.length}
            <p class="diag-sub">Last chapter turn</p>
            <table>
              <tbody>
                {#each s.turnTrace as [stage, n], i (i)}
                  <tr><td>{stage}</td><td class="ms">{n}{unitOf(stage)}</td></tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </details>
      {/if}
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
  .rnd-status {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12.5px;
    color: var(--faded, #8a8276);
    padding: 0 0 6px 2px;
  }
  .rnd-now {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--gold, #9e7d38);
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 5px;
    padding: 1px 9px;
  }
  .diag summary {
    font-size: 13px;
    color: var(--faded, #8a8276);
    cursor: pointer;
  }
  .field {
    width: 100%;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 9px;
    font-size: 14px;
    margin-bottom: 6px;
    box-sizing: border-box;
  }
  .offline {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 2px 4px;
  }
  .off-ok {
    font-weight: 600;
    color: var(--tierHuman, #6f8f6a);
  }
  .off-note {
    font-size: 12.5px;
    color: var(--faded, #8a8276);
    line-height: 1.4;
  }
  .off-go {
    align-self: flex-start;
    font-size: 13px;
    font-weight: 600;
    color: var(--gold, #9e7d38);
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    padding: 3px 12px;
  }
  .off-go:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .off-bar {
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--gold, #9e7d38) 18%, transparent);
    overflow: hidden;
  }
  .off-fill {
    height: 100%;
    background: var(--gold, #9e7d38);
    transition: width 0.2s ease;
  }
  .diag-sub {
    margin-top: 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--faded, #8a8276);
  }
  .diag table {
    width: 100%;
    margin-top: 6px;
    font-size: 12.5px;
    color: var(--faded, #8a8276);
    border-collapse: collapse;
  }
  .diag td {
    padding: 1px 0;
  }
  .diag .ms {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--ink, #211f1a);
  }
  .copy-diag {
    margin-top: 8px;
    align-self: flex-start;
  }
  .diag-note {
    margin-top: 4px;
    font-size: 11.5px;
    line-height: 1.35;
    color: var(--faded, #8a8276);
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
