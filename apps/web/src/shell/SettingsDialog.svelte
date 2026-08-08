<script lang="ts">
  // One Settings dialog (Android IA): analysis switches, theme, text size /
  // margin / line-spacing sliders, copy format, bundled stock set.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { completeOffline, surveyOffline, type OfflineSurvey } from "../engine/offline";
  import { cleanChurch } from "./church";
  import { PERF } from "../engine/perf";
  import type { WorkerDiagnostics } from "../engine/worker-client";
  import { zipRead, zipWrite } from "../engine/zip";
  import { idbApply } from "../engine/idb";
  import { nowStamp } from "../engine/StudyEngine";
  import { deviceLocale, fill, languages, plural, t } from "../lib/i18n.svelte";

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
      s.showToast(plural("settings.backedUp.one", "settings.backedUp.other", n, { name }));
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
      s.showToast(t("settings.backupFailed", { why }));
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
        s.showToast(t("settings.restoreNothing"));
        return;
      }
      // The restored files are now the truth — nothing (incl. the pagehide
      // flush) may persist the current session over them; just reload.
      muted = true;
      s.restoring = true;
      await s.rpc.freeze(); // the debounced authoring persist must not fire either
      await idbApply("user", safe);
      // The close-safe theme cache is a mirror of THIS session's config; the
      // restored home carries its own theme, and the boot reconcile would
      // otherwise trust the stale cache over it. Drop it so the restore wins.
      try {
        localStorage.removeItem("plumbline:themeChoice");
      } catch {
        /* no storage: nothing cached to override the restore anyway */
      }
      location.reload(); // the engine re-opens over the restored home
    } catch (err) {
      const why = err instanceof Error ? err.message : String(err);
      if (!muted) {
        // The zip never got as far as the home; this session is still whole.
        restoreFailed = t("settings.restoreUnreadable", { why });
        return;
      }
      // `idbApply` is ONE transaction and IndexedDB rolls an aborted one back
      // whole, so the reader's own study data is exactly as it was.
      try {
        sessionStorage.setItem(
          RESTORE_FAILED,
          t("settings.restoreIncomplete", { why }),
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
  /** Choose a language, or `""` to go back to following the device.
   *
   *  RELOADS, like the bundled-study-set toggle above, and for a sharper reason
   *  than "it is simpler". The string table itself is reactive and would repaint
   *  on the spot — but book names do not come from it. They come from the TOC,
   *  which the engine hands over once on the boot reply and which
   *  `session.svelte.ts` PINS in its cache precisely because it cannot change
   *  while a session runs. Swapping the chrome and leaving "Genesis" in the
   *  passage navigator would be a half-translated app that looks like a bug. */
  /** The switch in progress: which language, and how far its Bible has come
   *  (null fraction = nothing to download). Drives the full-screen overlay at
   *  the bottom of this file. */
  let switching = $state<{
    endonym: string;
    fraction: number | null;
    /** The TARGET language's catalogue, falling back to the live one. */
    say: (id: string, args?: Record<string, string | number>) => string;
  } | null>(null);

  async function setLanguage(code: string): Promise<void> {
    if ((s.config.language ?? "") === code) return;
    s.config.language = code;
    // FLUSH, THEN AWAIT, THEN RELOAD. `flushConfig` posts the save and returns;
    // the worker still has to write it into the home and persist that to
    // IndexedDB, and a reload fired in the same tick tears the page down first —
    // so the reader picks German, watches the app reload, and gets English back.
    // The RPC is ordered, so awaiting the flush is awaiting the save with it.
    // (Caught by e2e/language.spec.ts, which is the only thing that ever
    // exercised this: every other setting here takes effect without a reload.)
    // THE OVERLAY GOES UP FIRST, before any await. Everything below can take
    // seconds — a config flush, a 2 MB download, a reload — and without it the
    // reader taps Deutsch and watches an unchanged English screen do nothing. It
    // is full-screen rather than a line in this dialog because the reader's
    // attention is on what they just tapped.
    // IN THE LANGUAGE BEING SWITCHED TO, not the one being left — "Wechsel zu
    // Deutsch…", not "Switching to Deutsch". The reader has already asked for
    // that language; answering in the old one is the app lagging behind its own
    // reader. `t()` still reads the live table, so the sentence is fetched from
    // the target catalogue up front.
    const target = await s.rpc
      .static("i18nCatalog", code, deviceLocale())
      .then((c: any) => c?.strings ?? null)
      .catch(() => null);
    const say = (id: string, args?: Record<string, string | number>) =>
      target?.[id] ? fill(target[id], args) : t(id, args);
    switching = {
      endonym: languages().find((l) => l.code === code)?.endonym ?? code,
      fraction: null,
      say,
    };

    // THE SPLASH AFTER THE RELOAD MUST SPEAK THE NEW LANGUAGE. `i18n.svelte.ts`
    // seeds it from this key, and it is only written when a catalogue ARRIVES —
    // which is after the boot the splash belongs to. So without this the reader
    // sees: German overlay, English splash, German app. Three languages in one
    // gesture, and the reason the transition read as broken.
    try {
      localStorage.setItem("plumbline.lang", code || (navigator.languages?.[0] ?? "en").split("-")[0]);
    } catch {
      // A private window costs one frame of the old language. Not worth failing.
    }

    s.flushConfig();
    await s.rpc.flush();
    // THE TEXT, not just the interface. A language with its own Bible needs it
    // on the device, and picking the language IS the ask — a separate "download
    // German scripture" row would be a second decision about one intention.
    //
    // Failure is not fatal and must not block the switch: `corpus_for` in the
    // core falls back to the KJV, so a reader with no connection gets a German
    // interface over the English text and can try again by re-picking. Silent
    // beyond the bar, because the alternative is an error about a download they
    // did not explicitly start.
    if (code === "de") {
      const state = await s.rpc.germanState().catch(() => null);
      if (state?.available && !state.installed) {
        switching = { ...switching!, fraction: 0 };
        s.rpc.onGermanProgress = (f) => (switching = { ...switching!, fraction: f });
        await s.rpc.installGerman().catch(() => false);
      }
    }
    location.reload();
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

  // ── the suggested weaves, on request ────────────────────────────────────
  // 194 machine-proposed links that ship inside the APK but are a download
  // here: too big for the boot path of a phone that may never open the weave
  // library, and genuinely optional — they are suggestions to judge, not text.
  let suggested = $state<{ available: boolean; installed: boolean; gzBytes: number } | null>(null);
  let installing = $state(false);
  let suggestedError = $state("");
  $effect(() => {
    void s.rpc.suggestedState().then(
      (st) => (suggested = st),
      // A pack with no bundle answers `available: false` rather than throwing,
      // so reaching here means the worker itself is unreachable — in which case
      // the row simply does not appear.
      () => (suggested = null),
    );
  });
  async function installSuggested(): Promise<void> {
    installing = true;
    suggestedError = "";
    try {
      await s.rpc.installSuggested();
      suggested = await s.rpc.suggestedState();
      // The weave library is one of the reads `authored` invalidates, and the
      // worker fires it — but this dialog holds its own copy of nothing, so
      // there is no reload to do here.
    } catch (e) {
      suggestedError =
        e instanceof Error && e.message ? t("settings.downloadFailedWhy", { why: e.message }) : t("settings.downloadFailed");
    } finally {
      installing = false;
    }
  }

  // ── the reader's home church ────────────────────────────────────────────
  // Set it here, and every link this reader shares carries it, so a QR handed
  // out at a service leads back to that service.
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
  // "On this device" is a claim about BYTES, and only about bytes — NOT about
  // whether this SESSION has finished preparing what it already downloaded.
  // Otherwise a phone holding every byte is told "Still to download: the
  // analysis pack" for the whole time the engine is busy parsing it.
  // Downloading and preparing are different waits and the reader is owed the
  // difference.
  const offlineComplete = $derived(!!offline && offline.missing.length === 0);
  /** What is still missing, in a sentence — the text itself never is. */
  const offlineSummary = $derived.by(() => {
    const files = offline?.missing.length ?? 0;
    if (files) {
      return plural("settings.offlineMissing.one", "settings.offlineMissing.other", files, {
        size: mb(offline!.missingBytes),
      });
    }
    return t("settings.offlineComplete");
  });
  /** Preparing is not downloading; say so separately or not at all. */
  const preparingNote = $derived(
    offlineComplete && s.rndState !== "ready"
      ? t("settings.offlinePreparing")
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
      s.showToast(t("settings.offlineFailed"));
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
   *  off still pastes something answerable.
   *
   *  Screenshots are not the fallback: they cost a round trip every time and cut
   *  off exactly the rows that mattered. */
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
   *  in a release build this is the only such text, since the diagnostic tables
   *  render only in a measuring build. Derived, so what the reader reads is what
   *  the button copies. */
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
        s.showToast(t("settings.copyBlocked"));
      }
    }
  }

  // Tokens only: the label is looked up at RENDER, so a language change
  // repaints the radio list instead of leaving last language's words beside a
  // live control.
  const themes = ["system", "light", "dark", "night", "dracula", "solarized-light", "solarized-dark", "gruvbox", "nord"] as const;
  const themeLabel: Record<(typeof themes)[number], string> = {
    system: "themeSystem",
    light: "themeLight",
    dark: "themeDark",
    night: "themeNight",
    dracula: "themeDracula",
    "solarized-light": "themeSolarizedLight",
    "solarized-dark": "themeSolarizedDark",
    gruvbox: "themeGruvbox",
    nord: "themeNord",
  };
  const copyOpts = ["verse", "verseRef", "verseMarkdown"] as const;
  const copyLabel = { verse: "copyVerse", verseRef: "copyVerseRef", verseMarkdown: "copyMarkdown" };
</script>

{#if s.showSettings}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.showSettings = false)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t("settings.title")}
    data-surface="settings"
    use:modal={{ close: () => (s.showSettings = false) }}
  >
    <h2>{t("settings.title")}</h2>
    <div class="content">
      <!-- FIRST, above everything: it is the setting that decides what the rest
           of this dialog is written in, and a reader who cannot read the labels
           should not have to hunt past twenty of them to find it. -->
      <p class="label">{t("settings.language")}</p>
      <p class="desc-note">{t("settings.languageDesc")}</p>
      <label class="radio">
        <input
          type="radio"
          name="language"
          checked={!(s.config.language ?? "")}
          onchange={() => void setLanguage("")}
        />
        {t("settings.languageDevice")}
      </label>
      {#each languages() as l (l.code)}
        <label class="radio">
          <input
            type="radio"
            name="language"
            checked={(s.config.language ?? "") === l.code}
            onchange={() => void setLanguage(l.code)}
          />
          <!-- The endonym, always: someone looking for German is looking for
               "Deutsch", and they are looking for it in a dialog they may not be
               able to read a word of. -->
          {l.endonym}
        </label>
      {/each}
      <hr />
      {#if s.akjvAvailable}
        <!-- A reading aid over the SAME text, not a version picker: the words
             stay the KJV's everywhere it matters (memorize, Present, copy,
             share), and every marked word tells you what it replaced. -->
        <label class="toggle">
          <span class="body">
            <span class="name">{t("settings.akjv")}</span>
            <span class="desc">{t("settings.akjvDesc")}</span>
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
          <span class="name">{t("settings.human")}</span>
          <span class="desc">{t("settings.humanDesc")}</span>
        </span>
        <input
          type="checkbox"
          checked={s.config.humanAnalysis === true}
          onchange={() => toggleGate("humanAnalysis")}
        />
      </label>
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.machine")}</span>
          <span class="desc">{t("settings.machineDesc")}</span>
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
                ? t("settings.rndPreparing")
                : t("settings.rndDownloading", { percent: Math.round(s.rndProgress * 100) })}
            </span>
          {:else}
            <span>{t("settings.rndAbsent")}</span>
            <button class="rnd-now" onclick={() => void s.ensureRnd()}>{t("settings.rndNow")}</button>
          {/if}
        </div>
      {/if}
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.versePerLine")}</span>
          <span class="desc">{t("settings.versePerLineDesc")}</span>
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
      <p class="label">{t("settings.theme")}</p>
      {#each themes as token (token)}
        <label class="radio">
          <input
            type="radio"
            name="theme"
            checked={(s.config.theme ?? "system") === token}
            onchange={() => setTheme(token)}
          />
          {t(`settings.${themeLabel[token]}`)}
        </label>
      {/each}
      <hr />
      <p class="label">{t("settings.textSize")}</p>
      <p class="aa" style:font-size="{Number(s.config.bodySize ?? 18)}px">Aa</p>
      <input
        type="range"
        min="12"
        max="40"
        value={Number(s.config.bodySize ?? 18)}
        oninput={(e) => setNum("bodySize", Number((e.target as HTMLInputElement).value))}
      />
      <p class="label">{t("settings.margin")}</p>
      <input
        type="range"
        min="8"
        max="96"
        value={Number(s.config.sideMargin ?? 28)}
        oninput={(e) => setNum("sideMargin", Number((e.target as HTMLInputElement).value))}
      />
      <p class="label">{t("settings.lineSpacing")}</p>
      <input
        type="range"
        min="1"
        max="2.2"
        step="0.05"
        value={Number(s.config.lineSpacing ?? 1.35)}
        oninput={(e) => setNum("lineSpacing", Number((e.target as HTMLInputElement).value))}
      />
      <hr />
      <p class="label">{t("settings.copyFormat")}</p>
      {#each copyOpts as token (token)}
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
          {t(`settings.${copyLabel[token]}`)}
        </label>
      {/each}
      <hr />
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.bundled")}</span>
          <!-- The reload note is WEB ONLY: Android applies it without one, so it
               is its own key rather than a second copy of the sentence. -->
          <span class="desc">{t("settings.bundledDesc")} {t("settings.bundledReloads")}</span>
        </span>
        <input type="checkbox" checked={s.bundledOn} onchange={toggleBundled} />
      </label>
      {#if suggested?.available}
        <div class="toggle">
          <span class="body">
            <span class="name">{t("settings.suggested")}</span>
            <span class="desc">
              {#if suggested.installed}
                {t("settings.suggestedInstalled")}
              {:else}
                {t("settings.suggestedOffer", { kb: Math.round(suggested.gzBytes / 1024) })}
              {/if}
            </span>
          </span>
          {#if !suggested.installed}
            <button class="action" disabled={installing} onclick={installSuggested}>
              {installing ? t("settings.downloading") : t("settings.download")}
            </button>
          {/if}
        </div>
        {#if suggestedError}
          <p class="desc-note err">{suggestedError}</p>
        {/if}
      {/if}
      <hr />
      <p class="label">{t("settings.church")}</p>
      <p class="desc-note">{t("settings.churchDesc")}</p>
      <input class="field" placeholder={t("settings.churchName")} bind:value={churchName} onchange={saveChurch} />
      <input
        class="field"
        placeholder={t("settings.churchInfo")}
        bind:value={churchInfo}
        onchange={saveChurch}
      />
      <input class="field" placeholder={t("settings.churchUrl")} bind:value={churchUrl} onchange={saveChurch} />
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.presentAsNew")}</span>
          <span class="desc">{t("settings.presentAsNewDesc")}</span>
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
      <p class="label">{t("settings.welcome")}</p>
      <p class="desc-note">{t("settings.welcomeDesc")}</p>
      <div class="row">
        <!-- Reachable for EVERY reader, not just the ones whose path set `intro`
             (an established believer never had one set, so the top-bar Welcome
             button never showed for them). Falls back to the new-believer
             welcome. Reopening changes no data — it only sets shell state. -->
        <button class="action" onclick={() => { s.reopenIntro = s.intro ?? "new"; s.showSettings = false; }}>
          {t("settings.welcomeShow")}
        </button>
      </div>
      <hr />
      <p class="label">{t("settings.offline")}</p>
      <div class="offline">
        {#if offlineBusy}
          <span class="off-note">
            {s.rndPreparing
              ? t("settings.rndPreparing")
              : t("settings.offlineDownloading", {
                  percent: Math.round((s.rndState === "ready" ? offlineProgress : s.rndProgress) * 100),
                })}
          </span>
          <div class="off-bar">
            <div
              class="off-fill"
              style:width={`${(s.rndState === "ready" ? offlineProgress : s.rndProgress) * 100}%`}
            ></div>
          </div>
        {:else if offlineComplete}
          <span class="off-ok">{t("settings.offlineOk")}</span>
          <span class="off-note">
            {t("settings.offlineNoConnection")}{offline?.bytesOnDevice
              ? ` ${t("settings.offlineUsing", { size: mb(offline.bytesOnDevice) })}`
              : ""}
          </span>
          {#if preparingNote}
            <span class="off-note">{preparingNote}</span>
          {/if}
          <!-- "It is all downloaded" and "it will still be there" are different
               claims, and only the first one is ours to make: browsers evict
               storage under pressure. Say which of the two is true. -->
          {#if offline?.persisted === false}
            <span class="off-note">{t("settings.offlineMayClear")}</span>
          {:else if offline?.persisted}
            <span class="off-note">{t("settings.offlinePermanent")}</span>
          {/if}
        {:else}
          <span class="off-note">{offlineSummary}</span>
          <button class="off-go" onclick={downloadEverything}>{t("settings.offlineGo")}</button>
        {/if}
      </div>
      <hr />
      <p class="label">{t("settings.data")}</p>
      <div class="row">
        <button class="action" onclick={backup}>{t("settings.backup")}</button>
        <label class="action">
          {t("settings.restore")}
          <input type="file" accept=".zip,application/zip" onchange={restore} hidden />
        </label>
      </div>
      <p class="desc-note">{t("settings.dataDesc")}</p>
      <hr />
      <p class="label">{t("settings.report")}</p>
      <!-- ALWAYS here, whether or not this build is measuring itself: the app
           ships with the perf switch off, and a bug report still needs something
           to paste without handing readers a debug build (D-20). -->
      <p class="desc-note">{t("settings.reportDesc")}</p>
      <div class="row">
        <button class="action" onclick={copyReport}>{copied ? t("settings.reportCopied") : t("settings.reportCopy")}</button>
      </div>
      <details class="diag">
        <summary>{t("settings.reportShow")}</summary>
        <pre class="report">{reportText}</pre>
      </details>
      <!-- i18n-ignore-start: PERF-only. This whole block renders only in a
           measuring build, so no reader in any language can reach it, and its
           contents are stage names straight out of the engine's own trace —
           translating "worst single stall" would be translating a variable
           name. See scripts/check-i18n.mjs. -->
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
      <!-- i18n-ignore-end -->
    </div>
    <button class="done" onclick={() => (s.showSettings = false)}>{t("settings.done")}</button>
  </div>
{/if}

{#if switching}
  <!-- THE LANGUAGE TRANSITION.
       Full-screen and above everything, because a language change is the one
       setting that takes the whole app with it: a config write, possibly a 2 MB
       download, then a reload.

       Styled like the SPLASH rather than like this dialog, and from the same
       palette variables — it hands straight over to the splash when the reload
       fires, and the two should read as one motion rather than two screens. -->
  <div class="switching" role="status" aria-live="polite">
    <div class="sw-mark" aria-hidden="true">✦</div>
    <p class="sw-what">{switching.say("settings.switchingTo", { language: switching.endonym })}</p>
    {#if switching.fraction !== null}
      <div class="sw-bar">
        <div class="sw-fill" style:width="{Math.round(switching.fraction * 100)}%"></div>
      </div>
      <p class="sw-detail">{switching.say("settings.gettingTheBible", { percent: Math.round(switching.fraction * 100) })}</p>
      <p class="sw-note">{switching.say("settings.gettingTheBibleNote")}</p>
    {/if}
  </div>
{/if}

{#if restoreFailed}
  <!-- The one thing here that must not be a toast. A restore can fail with the
       phone already back in a pocket, and a reader who missed the 2.2 seconds is
       left believing their backup went in. No backdrop dismiss either — a stray
       tap must not take the message away before it has been read. -->
  <div class="err-backdrop"></div>
  <!-- `use:modal` with NO close: focus comes here and Tab is held here, but
       Escape does not dismiss it. Same reasoning as the missing backdrop
       dismiss above — a stray key must not take the message away before it has
       been read — and the Escape is still swallowed, so it cannot reach past
       this and close something behind it instead.
       Focus goes to Close, which is the acknowledgement. -->
  <div
    class="err-dialog"
    role="alertdialog"
    aria-modal="true"
    aria-label={t("settings.restoreFailedTitle")}
    data-surface="restore-failed"
    use:modal
  >
    <h2>{t("settings.restoreFailedTitle")}</h2>
    <p class="err-body">{restoreFailed}</p>
    <button class="done" data-modal-focus onclick={() => (restoreFailed = null)}>{t("common.close")}</button>
  </div>
{/if}

<style>
  /* ── the language transition ──────────────────────────────────────────────
     The splash's own look (App.svelte), on purpose: this screen is replaced by
     the splash a moment later and a different treatment would read as two
     unrelated waits. A system serif for the same reason the splash uses one —
     EB Garamond is not needed to say "one moment". */
  .switching {
    position: fixed;
    inset: 0;
    z-index: 100; /* above the dialog (40/41) and the failure bar (70) */
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 0 24px;
    text-align: center;
    font-family: Georgia, "Times New Roman", serif;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
  }
  .sw-mark {
    font-size: 28px;
    color: var(--gold, #7d632c);
  }
  .sw-what {
    font-size: calc(19px * var(--uiScale, 1));
  }
  .sw-bar {
    width: min(340px, 70vw);
    height: 5px;
    margin-top: 10px;
    border-radius: 3px;
    background: var(--rule, #d8cba8);
    overflow: hidden;
  }
  .sw-fill {
    height: 100%;
    background: var(--gold, #7d632c);
    border-radius: 3px;
    transition: width 0.15s ease;
  }
  .sw-detail {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #6c665d);
  }
  /* Reassurance, not progress — quieter than the line above it, like the
     splash's own "3 MB download". */
  .sw-note {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #6c665d);
    opacity: 0.85;
    max-width: 30em;
  }

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
    font-size: calc(17px * var(--uiScale, 1));
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
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.5;
    color: var(--faded, #8a8276);
  }
  .rnd-status {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    padding: 0 0 6px 2px;
  }
  .rnd-now {
    font-size: calc(12.5px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--gold, #9e7d38);
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 5px;
    padding: 1px 9px;
  }
  .diag summary {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    cursor: pointer;
  }
  .field {
    width: 100%;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 9px;
    font-size: calc(14px * var(--uiScale, 1));
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
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    line-height: 1.4;
  }
  .off-go {
    align-self: flex-start;
    font-size: calc(13px * var(--uiScale, 1));
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
    font-size: calc(12.5px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--faded, #8a8276);
  }
  .diag table {
    width: 100%;
    margin-top: 6px;
    font-size: calc(12.5px * var(--uiScale, 1));
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
    font-size: calc(11px * var(--uiScale, 1));
    line-height: 1.35;
    color: var(--faded, #8a8276);
    user-select: text;
  }
  .diag-note {
    margin-top: 4px;
    font-size: calc(11.5px * var(--uiScale, 1));
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
    font-size: calc(15px * var(--uiScale, 1));
  }
  .toggle .desc {
    font-size: calc(12px * var(--uiScale, 1));
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
    font-size: calc(12px * var(--uiScale, 1));
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
    font-size: calc(14.5px * var(--uiScale, 1));
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
    font-size: calc(14px * var(--uiScale, 1));
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
    font-size: calc(11.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .desc-note.err {
    color: var(--disputed, #9b3b2f);
  }
</style>
