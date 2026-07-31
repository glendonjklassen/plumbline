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
  // Must stay in step with engine/home.ts's USER_DIRS and the Android shell's
  // BACKUP_DIRS: a dir missing from this restore filter is a dir that exports
  // into the zip and is then silently dropped on the way back in.
  const BACKUP_DIRS = ["tags/", "threads/", "weaves/", "notes/", "memory/", "reading/", ".config/"];

  // Archives written before the Plumbline rename carry the config under
  // ".config/pure-study/"; the live home reads ".config/plumbline/". Remapped on
  // restore only — nothing ever writes the old name back. Without this an older
  // backup silently drops the user's settings (the authored dirs above are
  // unaffected: their names never moved).
  const LEGACY_CONFIG = ".config/pure-study/";
  const currentConfigPath = (p: string): string =>
    p.startsWith(LEGACY_CONFIG) ? ".config/plumbline/" + p.slice(LEGACY_CONFIG.length) : p;

  // A restore that fails after it has muted this session's writes cannot be
  // reported IN that session: `s.restoring` silences every config persist and
  // `home.freeze()` silences every authoring persist, and neither has an undo
  // (the worker has a `freeze` op and nothing that lifts it). A session left
  // standing there takes notes, tags and threads all day, looks like it saved
  // them, and keeps none. So a failure past that point reloads regardless, and
  // the message rides across the reload — the reader is owed an explanation,
  // not a blink.
  const RESTORE_FAILED = "plumbline:restoreFailed";
  /** A failed restore to tell the reader about, blocking (see below). */
  let restoreFailed = $state<string | null>(null);
  // Read on mount, and this dialog is mounted for the whole session, so the
  // notice lands whether or not Settings was open when the page came back.
  try {
    const carried = sessionStorage.getItem(RESTORE_FAILED);
    if (carried) {
      sessionStorage.removeItem(RESTORE_FAILED);
      restoreFailed = carried;
    }
  } catch {
    /* no session storage — then nothing was carried across either */
  }

  async function backup(): Promise<void> {
    const name = `plumbline-backup-${nowStamp().slice(0, 10)}.zip`;
    try {
      const files = new Map<string, Uint8Array>(await s.rpc.exportUserData());
      files.set(
        "plumbline-backup.json",
        new TextEncoder().encode(JSON.stringify({ format: 1, app: "web", exported: nowStamp() })),
      );
      const blob = new Blob([zipWrite(files) as unknown as BlobPart], { type: "application/zip" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = name;
      a.click();
      URL.revokeObjectURL(a.href);
      // The file is NAMED, so a reader who looked away while the browser saved
      // it knows what to go and find. Handing the anchor to the browser is the
      // last thing we can see — there is no completion signal for a download —
      // so this says what we did, not that the file is on the device.
      const n = files.size - 1;
      s.showToast(`Backed up ${n} file${n === 1 ? "" : "s"} as ${name}`);
    } catch (err) {
      // A toast, and deliberately NOT the blocking notice a failed restore gets.
      // A backup that fails is not a data-loss event: nothing was written, the
      // reader's own study data is exactly as it was, and they are standing in
      // this dialog looking at the button that is also the repair. A restore
      // failure has to block because it can land on a reloaded page with the
      // phone already in a pocket; this one cannot.
      // What it must not do is stay silent. Unguarded, a rejection anywhere
      // above — the export, the zip write, the browser refusing the save — made
      // the button do nothing at all, which is indistinguishable from a browser
      // that saved the file somewhere the reader hasn't looked.
      const why = err instanceof Error ? err.message : String(err);
      s.showToast(`Couldn't make the backup — no file was saved: ${why}`);
    }
  }

  async function restore(e: Event): Promise<void> {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    // Whether this session's writes have been muted yet — set BEFORE they are,
    // so a `freeze()` that itself fails still ends in a reload.
    let muted = false;
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
      muted = true;
      s.restoring = true;
      await s.rpc.freeze(); // the debounced authoring persist must not fire either
      await idbApply("user", safe);
      location.reload(); // the engine re-opens over the restored home
    } catch (err) {
      const why = err instanceof Error ? err.message : String(err);
      if (!muted) {
        // The zip never got as far as the home; this session is still whole.
        restoreFailed = `That backup couldn't be read, so nothing changed: ${why}`;
        return;
      }
      // `idbApply` is ONE transaction and IndexedDB rolls an aborted one back
      // whole, so the reader's own study data is exactly as it was.
      try {
        sessionStorage.setItem(
          RESTORE_FAILED,
          `Restoring that backup didn't finish, so nothing changed — your own study data is as it was, and you can try again: ${why}`,
        );
      } catch {
        /* storage refused the note; the reload still matters more than it does */
      }
      location.reload();
    }
  }

  function toggleGate(key: "humanAnalysis" | "machineAnalysis"): void {
    // `!== true`, not `=== false`. The tiers are opt-in, so an ABSENT value means
    // off — and `undefined === false` is false, which left the first click on a
    // never-set toggle doing nothing at all.
    s.config[key] = s.config[key] !== true;
    s.saveConfig();
    // Machine tier switched on: pull the deferred R&D pack in (no-op if the
    // idle path already did).
    if (key === "machineAnalysis" && s.config[key] === true) void s.ensureRnd();
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
      ? "The analysis is already downloaded and is being prepared."
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
      s.showToast("Couldn't finish downloading. Please check your connection.");
    } finally {
      offlineBusy = false;
      offlineProgress = 0;
    }
  }

  // Fresh timings whenever the dialog opens: the background stages keep
  // appending to the boot trace (Strong's, warm steps, the analysis pack),
  // and the turn split describes whichever chapter was last laid out — turn
  // a few pages, then open this.
  //
  // Read with PERF OFF too, which is the whole of D-20: `fromPin` — did this
  // launch come off the device with no request? — is measured by nothing, boot.ts
  // just records the rung it took, and the `diagnostics` op is not PERF-gated
  // either. So the bug-report header below reads the same in a release build as
  // in a measuring one; with the flag off the numbers alongside it come back
  // empty and zeroed, and `reportMeasurements` refuses to print them.
  let diag = $state<WorkerDiagnostics | null>(null);
  let copied = $state(false);
  $effect(() => {
    if (!s.showSettings) return;
    // ONE round trip, so the trace, the stall total and the per-file costs are
    // all from the same instant. Three separate reads drift while the background
    // load is still running, which is exactly when someone is reading this.
    void s.rpc
      .diagnostics()
      .then((d) => {
        diag = d;
        s.bootTrace = d.trace;
        s.turnTrace = d.turn;
      })
      // A dead worker rejects every call, and this one now runs in every session
      // rather than only in a measuring build. Nothing in the header needs it
      // (version, build, engine and pack version are all held on this thread), so
      // a failure here costs the reader one warm-boot note — it must not become an
      // unhandled rejection while they are reporting the very crash that caused it.
      .catch(() => {});
  });

  /** kilobytes, counts and milliseconds all shared one " ms" suffix, so the
   *  panel confidently reported `home evict after open (KB)  36367 ms`. */
  function unitOf(label: string): string {
    if (/\(KB\)$/.test(label)) return " KB";
    if (/^(worker font faces|items|wasm→JS)/.test(label)) return "";
    return " ms";
  }

  /** The header of a bug report: which build, which data, what kind of device.
   *
   *  NOT PERF-gated, and no line in here may become so (D-20). Every value below
   *  is a fact the app knows whatever the perf switch says — so flipping PERF
   *  cannot change one character of this, and a release build with measurement
   *  off still pastes something answerable. While this lived inside the PERF
   *  block the choice was to ship a debug build or to ship with nothing to paste.
   *
   *  Screenshots are not the fallback: they cost a round trip every time and cut
   *  off exactly the rows that mattered (2026-07-28). */
  function reportHeader(): string[] {
    const nav = navigator as any;
    const c = nav.connection ?? {};
    const L: string[] = [];
    L.push(`Plumbline ${__APP_VERSION__} · build ${__BUILD_ID__} · engine ${s.engineVersion}`);
    // The pack version off the SESSION (it arrives in BootInfo), not out of the
    // diagnostics round trip: the two are the same `manifest.version`, and this
    // one is still there when the worker is gone — which is when a report matters
    // most. `fromPin` has no such twin, and needs none: boot.ts records the rung
    // it took whether or not anything is being timed.
    L.push(`data pack ${s.packVersion || "?"}${diag?.fromPin ? " (warm: stage 1 off the device)" : ""}`);
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
    return L;
  }

  /** The measured half of a report — present only when this build measured
   *  itself. Missing whole rather than zeroed: with PERF off the stall meter is
   *  never started and no engine call is timed, so `total 0 ms across 0 stalls`
   *  would read as a device that never stalled instead of one nobody watched, and
   *  the boot trace would be just those pushes that happen not to be gated — a
   *  trace missing its stages, which is worse than no trace. One line says which
   *  kind of build it was, so nobody reads silence as good news. */
  function reportMeasurements(): string[] {
    if (!PERF) return ["", "(this build is not measuring itself — no boot trace, no stall meter)"];
    const L: string[] = [];
    if (diag) {
      L.push("");
      L.push("ENGINE THREAD UNAVAILABLE");
      L.push(`  total           ${Math.round(diag.stall.totalMs)} ms across ${diag.stall.count} stalls`);
      L.push(`  worst single    ${Math.round(diag.stall.worstMs)} ms`);
      // Reported, and EXCLUDED from the numbers above. A hidden tab has its
      // timers and its downloads frozen by the browser, and counting that as
      // engine work invented a 25-second stall on a launch that did none.
      L.push(`  page hidden     ${Math.round(diag.stall.hiddenMs)} ms (excluded above)`);
      L.push("  (time this thread could not answer a tap, a layout, OR its own downloads)");
      if (diag.slowCalls.length) {
        L.push("");
        L.push("SLOWEST ENGINE CALLS");
        for (const [name, ms] of diag.slowCalls) L.push(`  ${String(ms).padStart(7)} ms  ${name}`);
      }
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
    return L;
  }

  /** The whole paste: the facts first, then whatever was measured. */
  function report(): string {
    return [...reportHeader(), ...reportMeasurements()].join("\n");
  }
  /** Shown as well as copied. The clipboard can be refused (see below), and
   *  "select the text and copy it by hand" needs text on the screen to select —
   *  the diagnostic tables used to be that text, and they are gone in a release
   *  build. Derived, so what the reader reads is what the button copies. */
  const reportText = $derived(report());

  async function copyReport(): Promise<void> {
    const text = reportText;
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
  <div class="dialog" role="dialog" aria-modal="true" data-surface="settings">
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
          checked={s.config.humanAnalysis === true}
          onchange={() => toggleGate("humanAnalysis")}
        />
      </label>
      <label class="toggle">
        <span class="body">
          <span class="name">Machine analysis</span>
          <span class="desc">Appears-alongside, where a word concentrates, leitwort.</span>
        </span>
        <input
          type="checkbox"
          checked={s.config.machineAnalysis === true}
          onchange={() => toggleGate("machineAnalysis")}
        />
      </label>
      {#if s.config.machineAnalysis === true && s.rndState !== "ready"}
        <div class="rnd-status">
          {#if s.rndState === "loading"}
            <span>
              {s.rndPreparing
                ? "Preparing the analysis…"
                : `Downloading the analysis pack — ${Math.round(s.rndProgress * 100)}%`}
            </span>
          {:else}
            <span>Analysis pack not downloaded (~1.5 MB).</span>
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
          <span class="desc">Threads, tags, and weaves that come with the app. Changing this reloads the app.</span>
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
      <hr />
      <p class="label">Report a problem</p>
      <!-- ALWAYS here, whether or not this build is measuring itself. This used to
           live inside the PERF block below, so shipping with the perf switch off —
           which is how it must ship — shipped with nothing to paste, and the only
           other option was handing readers a debug build (D-20). -->
      <p class="desc-note">
        Which build you're running, which data pack, and what kind of device — the facts that make
        a report answerable. Nothing about your notes, your study or your church is in it.
      </p>
      <div class="row">
        <button class="action" onclick={copyReport}>{copied ? "Copied ✓" : "Copy bug report"}</button>
      </div>
      <details class="diag">
        <summary>Show what gets copied</summary>
        <pre class="report">{reportText}</pre>
      </details>
      {#if PERF && s.bootTrace.length}
        <hr />
        <details class="diag">
          <!-- The numbers only, and only in a measuring build: the button above
               already copies them when they exist. -->
          <summary>Boot diagnostics — this device</summary>
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
          {#if diag?.slowCalls.length}
            <p class="diag-sub">Slowest engine calls</p>
            <table>
              <tbody>
                {#each diag.slowCalls as [name, ms], i (i)}
                  <tr><td>{name}</td><td class="ms">{ms} ms</td></tr>
                {/each}
              </tbody>
            </table>
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

{#if restoreFailed}
  <!-- The one thing here that must not be a toast. A restore can fail with the
       phone already back in a pocket, and a reader who missed the 2.2 seconds is
       left believing their backup went in. No backdrop dismiss either — a stray
       tap must not take the message away before it has been read. -->
  <div class="err-backdrop"></div>
  <div
    class="err-dialog"
    role="alertdialog"
    aria-modal="true"
    aria-label="Restore didn't finish"
    data-surface="restore-failed"
  >
    <h2>Restore didn't finish</h2>
    <p class="err-body">{restoreFailed}</p>
    <button class="done" onclick={() => (restoreFailed = null)}>Close</button>
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
    /* Less the destination bar, or the last setting sits under it. */
    max-height: calc(84vh - var(--bottomNavH, 0px));
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
  /* Above every other surface, the confirmation included: this one reports a
     failure the reader already lived through, and it arrives on a page that has
     just reloaded under them. */
  .err-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.4);
    z-index: 52;
  }
  .err-dialog {
    position: fixed;
    z-index: 53;
    top: 22vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(400px, 92vw);
    max-height: calc(70vh - var(--bottomNavH, 0px));
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 22px 14px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.32);
  }
  .err-body {
    font-size: 14.5px;
    line-height: 1.5;
    color: var(--faded, #8a8276);
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
  /* The report itself, on screen: the answer to "what are you sending?" and the
     fallback when the clipboard is refused, so it has to be selectable. */
  .report {
    margin-top: 6px;
    max-height: 38vh;
    overflow: auto;
    white-space: pre-wrap;
    /* The user-agent string is one ~130-character token with no spaces in it, and
       without this it widens the dialog past the edge of a phone. */
    overflow-wrap: anywhere;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    line-height: 1.35;
    color: var(--faded, #8a8276);
    user-select: text;
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
    /* "Restore from backup…" is a `<label>` wrapping a file input, so the 44px
       tap floor (app.css) does not reach it — but it stretches to the row's
       height beside a button that the floor DID reach, and a label does not
       centre its own text the way a button does. Without this the two controls
       in the row are the same size with their words at different heights. */
    align-content: center;
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
