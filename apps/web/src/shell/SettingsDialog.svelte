<script lang="ts">
  // One Settings dialog: analysis switches, theme, text size / margin /
  // line-spacing sliders, copy format, bundled stock set.
  import { getSession } from "../state/session.svelte";
  import { shippedBase } from "../lib/locale";
  import { DEFAULT_FONT, FONT_CSS_FAMILY, FONT_SCRIPT } from "../engine/fonts.generated";
  import { fontStackFor } from "../reader/measure";
  import { modal } from "../lib/modal";
  import { completeOffline, surveyOffline, type OfflineSurvey } from "../engine/offline";
  import { PERF } from "../engine/perf";
  import type { WorkerDiagnostics } from "../engine/worker-client";
  import { zipRead, zipWrite } from "../engine/zip";
  import { idbApply } from "../engine/idb";
  import { nowStamp } from "../engine/StudyEngine";
  import {
    deviceLocale,
    fill,
    hasOwnBible,
    hasOwnLexicon,
    languageLabel,
    languages,
    plural,
    readerFace,
    script,
    t,
  } from "../lib/i18n.svelte";

  const s = getSession();

  // Backup / restore: the authored home dirs as a zip. Must stay in step with
  // engine/home.ts's USER_DIRS — a dir missing from this restore filter exports
  // into the zip and is then silently dropped on the way back in.
  const BACKUP_DIRS = ["tags/", "threads/", "weaves/", "notes/", "memory/", "reading/", "plans/", "devotionals/", ".config/"];

  // Pre-rename archives carry the config under ".config/pure-study/"; the live
  // home reads ".config/plumbline/". Remapped on restore only — nothing writes
  // the old name back. Without this an older backup drops the user's settings.
  const LEGACY_CONFIG = ".config/pure-study/";
  const currentConfigPath = (p: string): string =>
    p.startsWith(LEGACY_CONFIG) ? ".config/plumbline/" + p.slice(LEGACY_CONFIG.length) : p;

  // Once a restore has muted this session's writes (`s.restoring` + `freeze()`,
  // neither of which has an undo) the session can no longer save anything, so a
  // failure past that point must reload regardless — and the message rides
  // across the reload in sessionStorage.
  const RESTORE_FAILED = "plumbline:restoreFailed";
  /** A failed restore to tell the reader about, blocking. */
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

  /** The configured Sunday service start as an `<input type="time">` value —
   *  "HH:MM", or "" when never set. */
  function serviceTimeValue(): string {
    const m = s.config.sundayService;
    if (typeof m !== "number") return "";
    return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
  }

  function setServiceTime(e: Event): void {
    const v = (e.currentTarget as HTMLInputElement).value;
    if (!v) {
      // Cleared: back to the before-noon rule. undefined drops the key from the
      // snapshot, so the file loses it rather than storing a null.
      s.config.sundayService = undefined;
    } else {
      const [h, m] = v.split(":").map(Number);
      s.config.sundayService = h * 60 + m;
    }
    s.saveConfig();
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
      // There is no completion signal for a download, so the toast names the
      // file and says what we did, not that it landed on the device.
      const n = files.size - 1;
      s.showToast(plural("settings.backedUp.one", "settings.backedUp.other", n, { name }));
    } catch (err) {
      // A toast, not the blocking notice a failed restore gets: nothing was
      // written and the reader is looking at the button that is also the repair.
      // It must not stay silent, though — unguarded, a rejection made the button
      // look like a browser that saved the file somewhere unnoticed.
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
      // The close-safe theme cache mirrors THIS session's config; the boot
      // reconcile would trust it over the restored home's own theme.
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
    // `!== true`, not `=== false`: the tiers are opt-in, so an absent value means
    // off, and `undefined === false` left the first click doing nothing.
    s.config[key] = s.config[key] !== true;
    s.saveConfig();
    // Machine tier on: pull the deferred R&D pack in (no-op if idle already did).
    if (key === "machineAnalysis" && s.config[key] === true) void s.ensureRnd();
  }
  /** The switch in progress: which language, and how far its Bible has come
   *  (null fraction = nothing to download). Drives the full-screen overlay at
   *  the bottom of this file. */
  let switching = $state<{
    endonym: string;
    fraction: number | null;
    /** The TARGET language's catalogue, falling back to the live one. */
    say: (id: string, args?: Record<string, string | number>) => string;
  } | null>(null);

  /** Choose a language, or `""` to follow the device. Reloads: book names come
   *  from the TOC, which the engine hands over once on boot and session.svelte.ts
   *  pins, so a live swap would leave "Genesis" in the passage navigator.
   *
   *  Flush, then await, then reload — `flushConfig` only posts the save, and a
   *  reload in the same tick tears the page down before the worker writes it.
   *  The overlay goes up before any await (this can take seconds), and speaks the
   *  TARGET language, so the sentence is fetched from its catalogue up front. */
  async function setLanguage(code: string): Promise<void> {
    if ((s.config.language ?? "") === code) return;
    s.config.language = code;
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

    // The splash after the reload seeds itself from this key. It is otherwise
    // only written when a catalogue arrives — after the boot the splash belongs
    // to — so without this the reader gets an English splash mid-switch.
    try {
      localStorage.setItem("plumbline.lang", code || shippedBase(navigator.languages?.[0] ?? "en") || "en");
    } catch {
      // A private window costs one frame of the old language. Not worth failing.
    }

    s.flushConfig();
    await s.rpc.flush();
    // The text too, not just the interface. Failure must not block the switch —
    // `corpus_for` falls back to the KJV. The gate is `hasOwnBible` (the
    // catalogue's corpus role), never the DOWNLOAD list, which empties for a
    // language with no dictionary and let the reload race the very download it
    // needed. The question is "is that Bible on this device right now", so ask
    // the depot and install only if it is absent.
    if (hasOwnBible(code)) {
      const state = await s.rpc.langPackState(code).catch(() => null);
      if (state?.available && !state.installed) {
        switching = { ...switching!, fraction: 0 };
        s.rpc.onLangPackProgress = (f) => (switching = { ...switching!, fraction: f });
        await s.rpc.installLangPack(code).catch(() => false);
      }
    }
    location.reload();
  }

  /** English definitions over this language's own dictionary: flush, await,
   *  reload — setLanguage's discipline, because it is picked at open. */
  async function setBaseLexicon(off: boolean): Promise<void> {
    s.config.localizedLexiconOff = off;
    s.flushConfig();
    await s.rpc.flush();
    location.reload();
  }

  /** No `applyTheme()` here: the choice IS the input, and App.svelte's writer
   *  effects paint whatever it resolves to. A second caller would be a second
   *  owner of the same pixels. */
  function setTheme(theme: string): void {
    s.config.theme = theme;
    s.saveConfig();
  }
  function setNum(key: "bodySize" | "sideMargin" | "lineSpacing", v: number): void {
    s.config[key] = v;
    s.saveConfig();
  }
  /** The style knobs back to core::config's shipped defaults: size, spacing,
   *  margins, both faces, theme. Nothing else — reading aids and the reader's
   *  data are not "style". */
  async function defaultStyle(): Promise<void> {
    setTheme("system");
    setNum("bodySize", 20);
    setNum("sideMargin", 28);
    setNum("lineSpacing", 1.35);
    if ((s.config.chromeFont ?? DEFAULT_FONT) !== DEFAULT_FONT) s.setChromeFont(DEFAULT_FONT);
    // Last and guarded: the text face relayouts the chapter.
    if ((s.config.textFont ?? DEFAULT_FONT) !== DEFAULT_FONT) await setTextFont(DEFAULT_FONT);
  }
  async function toggleBundled(): Promise<void> {
    await s.rpc.setBundled(!s.bundledOn);
    s.flushConfig();
    location.reload(); // the engine re-opens with/without the stock set
  }

  // The suggested weaves, on request: 194 machine-proposed links, a download
  // rather than boot payload for a reader who may never open the weave library.
  let suggested = $state<{ available: boolean; installed: boolean; gzBytes: number } | null>(null);
  let installing = $state(false);
  let suggestedError = $state("");
  $effect(() => {
    void s.rpc.suggestedState().then(
      (st) => (suggested = st),
      // A pack with no bundle answers `available: false` rather than throwing, so
      // reaching here means the worker is unreachable and the row does not appear.
      () => (suggested = null),
    );
  });
  async function installSuggested(): Promise<void> {
    installing = true;
    suggestedError = "";
    try {
      await s.rpc.installSuggested();
      suggested = await s.rpc.suggestedState();
    } catch (e) {
      suggestedError =
        e instanceof Error && e.message ? t("settings.downloadFailedWhy", { why: e.message }) : t("settings.downloadFailed");
    } finally {
      installing = false;
    }
  }

  // Offline completeness: the answer to "will this work with no signal?", and
  // the repair when it wouldn't have.
  let offline = $state<OfflineSurvey | null>(null);
  let offlineBusy = $state(false);
  let offlineProgress = $state(0);
  const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;
  // A claim about bytes only, never about whether this session has finished
  // PREPARING them — otherwise a phone holding every byte is told "still to
  // download" for as long as the engine is parsing. See `preparingNote`.
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
      // The machine tier first: the piece left out on phones, and loading it
      // also puts its files in the cache.
      if (s.rndState !== "ready") await s.ensureRnd();
      offline = await completeOffline((f) => (offlineProgress = f));
    } catch {
      s.showToast(t("settings.offlineFailed"));
    } finally {
      offlineBusy = false;
      offlineProgress = 0;
    }
  }

  // Fresh timings whenever the dialog opens: background stages keep appending to
  // the boot trace. Read with PERF OFF too — `diagnostics` is not gated, so the
  // report header is identical in a release build; the numbers come back zeroed
  // there and `reportMeasurements` refuses to print them.
  let diag = $state<WorkerDiagnostics | null>(null);
  let copied = $state(false);
  $effect(() => {
    if (!s.showSettings) return;
    // One round trip, so trace, stall total and per-file costs share an instant;
    // separate reads drift while the background load is still running.
    void s.rpc
      .diagnostics()
      .then((d) => {
        diag = d;
        s.bootTrace = d.trace;
        s.turnTrace = d.turn;
      })
      // A dead worker rejects every call and this runs in every session. Nothing
      // in the header needs it, so a failure must not become an unhandled
      // rejection while the reader is reporting the crash that caused it.
      .catch(() => {});
  });

  /** The trace mixes kilobytes, counts and milliseconds under one label column;
   *  without this every row is suffixed " ms". */
  function unitOf(label: string): string {
    if (/\(KB\)$/.test(label)) return " KB";
    if (/^(worker font faces|items|wasm→JS)/.test(label)) return "";
    return " ms";
  }

  /** The header of a bug report: which build, which data, what kind of device.
   *  Not PERF-gated, and no line in here may become so — a release build with
   *  measurement off must still paste something answerable. */
  function reportHeader(): string[] {
    const nav = navigator as any;
    const c = nav.connection ?? {};
    const L: string[] = [];
    L.push(`Plumbline ${__APP_VERSION__} · build ${__BUILD_ID__} · engine ${s.engineVersion}`);
    // The pack version off the SESSION (it arrives in BootInfo), not out of the
    // diagnostics round trip: same `manifest.version`, but still there when the
    // worker is gone — which is when a report matters most.
    L.push(`data pack ${s.packVersion || "?"}${diag?.fromPin ? " (warm: stage 1 off the device)" : ""}`);
    L.push("");
    L.push("DEVICE");
    L.push(`  ua              ${navigator.userAgent}`);
    // The browser's OWN estimate — a byte count over a wall clock measures this
    // thread being busy, not the connection.
    L.push(`  connection      ${c.effectiveType ?? "?"} · downlink ${c.downlink ?? "?"} Mbps · rtt ${c.rtt ?? "?"} ms · saveData ${c.saveData ?? false}`);
    L.push(`  cpu threads     ${navigator.hardwareConcurrency ?? "?"}`);
    L.push(`  device memory   ${nav.deviceMemory ?? "?"} GB`);
    L.push(`  screen          ${screen.width}x${screen.height} @${devicePixelRatio}`);
    L.push(`  storage used    ${offline?.bytesOnDevice ? mb(offline.bytesOnDevice) : "?"} · persisted ${offline?.persisted ?? "?"}`);
    L.push(`  pack files      ${offline?.totalFiles ?? "?"} · missing ${offline?.missing.length ?? "?"}`);
    return L;
  }

  /** The measured half of a report. Missing whole rather than zeroed: with PERF
   *  off nothing is timed, and `total 0 ms across 0 stalls` reads as a device that
   *  never stalled rather than one nobody watched. */
  function reportMeasurements(): string[] {
    if (!PERF) return ["", "(this build is not measuring itself — no boot trace, no stall meter)"];
    const L: string[] = [];
    if (diag) {
      L.push("");
      L.push("ENGINE THREAD UNAVAILABLE");
      L.push(`  total           ${Math.round(diag.stall.totalMs)} ms across ${diag.stall.count} stalls`);
      L.push(`  worst single    ${Math.round(diag.stall.worstMs)} ms`);
      // Reported, and excluded from the numbers above: a hidden tab has its
      // timers and downloads frozen, which counts as engine work otherwise.
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
  /** Shown as well as copied — the clipboard can be refused, and copying by hand
   *  needs text on screen. Derived, so what is read is what is copied. */
  const reportText = $derived(report());

  async function copyReport(): Promise<void> {
    const text = reportText;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // Clipboard needs a secure context and a user gesture, and some phone
      // browsers refuse anyway; fall back to the share sheet, then to the toast.
      try {
        await navigator.share?.({ text });
      } catch {
        s.showToast(t("settings.copyBlocked"));
      }
    }
  }

  // Tokens only: the label is looked up at render, so a language change repaints
  // the picker. Keep in step with core::theme::ThemeChoice and the worker's
  // THEME_TOKENS.
  const themes = [
    "system",
    "light",
    "dark",
    "night",
    "solarized-light",
    "solarized-dark",
    "gruvbox",
    "nord",
    "one-dark",
    "sepia",
    "catppuccin-mocha",
    "catppuccin-latte",
    "tokyo-night",
    "rose-pine",
    "synthwave",
    "scriptorium",
    "blueprint",
    "phosphor",
    "high-contrast",
  ] as const;
  const themeLabel: Record<(typeof themes)[number], string> = {
    system: "themeSystem",
    light: "themeLight",
    dark: "themeDark",
    night: "themeNight",
    "solarized-light": "themeSolarizedLight",
    "solarized-dark": "themeSolarizedDark",
    gruvbox: "themeGruvbox",
    nord: "themeNord",
    "one-dark": "themeOneDark",
    sepia: "themeSepia",
    "catppuccin-mocha": "themeCatppuccinMocha",
    "catppuccin-latte": "themeCatppuccinLatte",
    "tokyo-night": "themeTokyoNight",
    "rose-pine": "themeRosePine",
    synthwave: "themeSynthwave",
    scriptorium: "themeScriptorium",
    blueprint: "themeBlueprint",
    phosphor: "themePhosphor",
    "high-contrast": "themeHighContrast",
  };
  // The faces this language can be read in, straight off the generated registry
  // so a family added to scripts/subset-fonts.mjs appears here with no edit. The
  // label is the typeface's own name — a proper noun, so not in the catalogue.
  //
  // Filtered by SCRIPT (`core::font::Font::offered_for` is the same rule in
  // Rust): per-glyph fallback would paint Arabic in Amiri whatever was picked,
  // and since `FONT_SCALE` reads off the SELECTED token, offering Inter to an
  // Arabic reader would render Amiri at Inter's 0.87 — a mislabelled size slider.
  //
  // When exactly one face qualifies the pickers are not rendered at all;
  // `readerFace` in lib/i18n.svelte.ts resolves applied tokens to the script face.
  const fonts = $derived(
    Object.keys(FONT_CSS_FAMILY).filter((tok) => FONT_SCRIPT[tok] === script()),
  );
  const fontName = (token: string): string => FONT_CSS_FAMILY[token] ?? token;

  let fontBusy = $state(false);
  async function setTextFont(token: string): Promise<void> {
    // Awaited with the control disabled: the face must be in both the worker
    // (which measures) and this thread (which paints) before the relayout, and
    // on a slow connection that is a real download.
    fontBusy = true;
    try {
      await s.setTextFont(token);
    } finally {
      fontBusy = false;
    }
  }

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
      <!-- First: it decides what the rest of this dialog is written in, and a
           reader who cannot read the labels must not have to hunt for it. -->
      <p class="label">{t("settings.language")}</p>
      <p class="desc-note">{t("settings.languageDesc")}</p>
      <!-- Options are ENDONYMS, always — someone looking for German is looking
           for "Deutsch", in a dialog they may not be able to read. -->
      <select
        class="dropdown"
        aria-label={t("settings.language")}
        value={s.config.language ?? ""}
        onchange={(e) => void setLanguage((e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">{t("settings.languageDevice")}</option>
        {#each languages() as l (l.code)}
          <option value={l.code}>{languageLabel(l)}</option>
        {/each}
      </select>
      {#if hasOwnLexicon()}
        <!-- Escape hatch from a machine-translated dictionary back to the English
             original. Shown for any language the registry says has one of its
             own. Reloads: the dictionary is picked when the engine opens. -->
        <label class="toggle">
          <span class="body">
            <span class="name">{t("settings.baseLexicon")}</span>
            <span class="desc">{t("settings.baseLexiconDesc")}</span>
          </span>
          <input
            type="checkbox"
            checked={s.config.localizedLexiconOff === true}
            onchange={(e) => void setBaseLexicon(e.currentTarget.checked)}
          />
        </label>
      {/if}
      {#if s.akjvAvailable}
        <!-- A choice, not a switch, so each option can say what it costs. Still a
             reading aid over the SAME text, not a version picker: the words stay
             the KJV's for memorize, Present, copy and share. -->
        <hr />
        <p class="label">{t("settings.wording")}</p>
        <label class="radio rich">
          <input
            type="radio"
            name="wording"
            checked={s.config.akjvOverlay !== true}
            onchange={() => void s.setAkjvOverlay(false)}
          />
          <span class="body">
            <span class="name">{t("settings.wordingClassic")}</span>
            <span class="desc">{t("settings.wordingClassicDesc")}</span>
          </span>
        </label>
        <label class="radio rich">
          <input
            type="radio"
            name="wording"
            checked={s.config.akjvOverlay === true}
            onchange={() => void s.setAkjvOverlay(true)}
          />
          <span class="body">
            <span class="name">{t("settings.wordingModern")}</span>
            <span class="desc">{t("settings.wordingModernDesc")}</span>
          </span>
        </label>
      {/if}
      <hr />
      <p class="label">{t("settings.theme")}</p>
      <select
        class="dropdown"
        aria-label={t("settings.theme")}
        value={s.config.theme ?? "system"}
        onchange={(e) => setTheme((e.currentTarget as HTMLSelectElement).value)}
      >
        {#each themes as token (token)}
          <option value={token}>{t(`settings.${themeLabel[token]}`)}</option>
        {/each}
      </select>
      {#if fonts.length > 1}
        <hr />
        <p class="label">{t("settings.textFont")}</p>
        <select
          class="dropdown"
          aria-label={t("settings.textFont")}
          disabled={fontBusy}
          value={readerFace(s.config.textFont ?? DEFAULT_FONT)}
          onchange={(e) => setTextFont((e.currentTarget as HTMLSelectElement).value)}
        >
          {#each fonts as token (token)}
            <option value={token} style:font-family={fontStackFor(token)}>{fontName(token)}</option>
          {/each}
        </select>
        <p class="label">{t("settings.chromeFont")}</p>
        <select
          class="dropdown"
          aria-label={t("settings.chromeFont")}
          value={readerFace(s.config.chromeFont ?? DEFAULT_FONT)}
          onchange={(e) => s.setChromeFont((e.currentTarget as HTMLSelectElement).value)}
        >
          {#each fonts as token (token)}
            <option value={token} style:font-family={fontStackFor(token)}>{fontName(token)}</option>
          {/each}
        </select>
      {/if}
      <hr />
      <p class="label">{t("settings.textSize")}</p>
      <p class="aa" style:font-size="{Number(s.config.bodySize ?? 20)}px">Aa</p>
      <input
        type="range"
        min="14"
        max="30"
        value={Number(s.config.bodySize ?? 20)}
        oninput={(e) => setNum("bodySize", Number((e.target as HTMLInputElement).value))}
      />
      <p class="label">{t("settings.margin")}</p>
      <input
        type="range"
        min="16"
        max="56"
        value={Number(s.config.sideMargin ?? 28)}
        oninput={(e) => setNum("sideMargin", Number((e.target as HTMLInputElement).value))}
      />
      <p class="label">{t("settings.lineSpacing")}</p>
      <input
        type="range"
        min="1.2"
        max="2"
        step="0.05"
        value={Number(s.config.lineSpacing ?? 1.35)}
        oninput={(e) => setNum("lineSpacing", Number((e.target as HTMLInputElement).value))}
      />
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
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.pageTurn")}</span>
          <span class="desc">{t("settings.pageTurnDesc")}</span>
        </span>
        <input
          type="checkbox"
          checked={!!s.config.pageTurn}
          onchange={() => {
            s.config.pageTurn = !s.config.pageTurn;
            s.saveConfig();
          }}
        />
      </label>
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.sundayService")}</span>
          <span class="desc">{t("settings.sundayServiceDesc")}</span>
        </span>
        <input type="time" class="time" value={serviceTimeValue()} onchange={setServiceTime} />
      </label>
      <div class="row">
        <button class="action" disabled={fontBusy} onclick={() => void defaultStyle()}>{t("settings.defaultStyle")}</button>
      </div>
      <p class="desc-note">{t("settings.defaultStyleDesc")}</p>
      <hr />
      <!-- Backup lives with the everyday settings, not behind Advanced. -->
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
      <details class="advanced">
        <summary>{t("settings.advanced")}</summary>
        <p class="desc-note">{t("settings.advancedDesc")}</p>
      <!-- Both typography switches default ON, so `!== false` is the read: an
           absent key is a config from before the setting existed, not an opt-out.
           Numbers are a layout input (the worker re-lays out); italics repaint. -->
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.verseNumbers")}</span>
          <span class="desc">{t("settings.verseNumbersDesc")}</span>
        </span>
        <input
          type="checkbox"
          checked={s.config.verseNumbers !== false}
          onchange={() => {
            s.config.verseNumbers = s.config.verseNumbers === false;
            s.saveConfig();
          }}
        />
      </label>
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.addedItalics")}</span>
          <span class="desc">{t("settings.addedItalicsDesc")}</span>
        </span>
        <input
          type="checkbox"
          checked={s.config.addedItalics !== false}
          onchange={() => {
            s.config.addedItalics = s.config.addedItalics === false;
            s.saveConfig();
          }}
        />
      </label>
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
      <!-- Which thread the gospel button opens. The stored value is the thread's
           NAME, and the empty option means the default rather than "none" —
           storing "Romans Road" would freeze a name the reader can change.

           The displayed value repeats the resolver's existence check: a deleted
           thread behaves as the default, so the control must show the default
           rather than a blank select or a stale name. -->
      <p class="label">{t("settings.gospelThread")}</p>
      <p class="desc-note">{t("settings.gospelThreadDesc")}</p>
      <select
        class="dropdown"
        aria-label={t("settings.gospelThread")}
        value={(s.q("threads")?.threads ?? []).some((th: any) => th.name === s.config.gospelThread)
          ? s.config.gospelThread
          : ""}
        onchange={(e) => {
          s.config.gospelThread = (e.currentTarget as HTMLSelectElement).value;
          s.saveConfig();
        }}
      >
        <option value="">{t("settings.gospelThreadDefault")}</option>
        {#each (s.q("threads")?.threads ?? []) as th (th.name)}
          <option value={th.name}>{th.name}</option>
        {/each}
      </select>
      <hr />
      <label class="toggle">
        <span class="body">
          <span class="name">{t("settings.bundled")}</span>
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
          <!-- "All downloaded" and "it will still be there" are different claims:
               browsers evict storage under pressure. Say which one is true. -->
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
      <p class="label">{t("settings.report")}</p>
      <!-- Always here, measuring build or not: the app ships with the perf switch
           off and a bug report still needs something to paste. -->
      <p class="desc-note">{t("settings.reportDesc")}</p>
      <div class="row">
        <button class="action" onclick={copyReport}>{copied ? t("settings.reportCopied") : t("settings.reportCopy")}</button>
      </div>
      <details class="diag">
        <summary>{t("settings.reportShow")}</summary>
        <pre class="report">{reportText}</pre>
      </details>
      </details>
      <!-- i18n-ignore-start: PERF-only — renders only in a measuring build, and
           its contents are stage names out of the engine's own trace. -->
      {#if PERF && s.bootTrace.length}
        <hr />
        <details class="diag">
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
  <!-- The language transition: full-screen and above everything, because the
       change takes the whole app with it (config write, maybe a 2 MB download,
       reload). Styled like the splash, which it hands straight over to. -->
  <div class="switching" role="status" aria-live="polite">
    <div class="sw-mark" aria-hidden="true">✦</div>
    <p class="sw-what">{switching.say("settings.switchingTo", { language: switching.endonym })}</p>
    {#if switching.fraction !== null}
      <div class="sw-bar">
        <div class="sw-fill" style:width="{Math.round(switching.fraction * 100)}%"></div>
      </div>
      <p class="sw-detail">{switching.say("settings.gettingTheBible", { language: switching.endonym, percent: Math.round(switching.fraction * 100) })}</p>
      <p class="sw-note">{switching.say("settings.gettingTheBibleNote")}</p>
    {/if}
  </div>
{/if}

{#if restoreFailed}
  <!-- Not a toast: a restore can fail with the phone already back in a pocket,
       and a reader who missed it believes their backup went in. No backdrop
       dismiss either — a stray tap must not take the message away. -->
  <div class="err-backdrop"></div>
  <!-- `use:modal` with NO close: focus comes here and Tab is held here, but
       Escape does not dismiss it (it is still swallowed, so it cannot close
       something behind this instead). Focus goes to Close, the acknowledgement. -->
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
  /* The language transition, in the splash's own look (App.svelte): it is
     replaced by the splash a moment later. System serif, like the splash. */
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
  /* Above every other surface, the confirmation included. */
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
  .advanced {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    padding: 8px 12px;
  }
  .advanced > summary {
    cursor: pointer;
    font-weight: 600;
    color: var(--ink, #211f1a);
    padding: 4px 0;
  }
  .diag summary {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    cursor: pointer;
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
    text-align: end;
    font-variant-numeric: tabular-nums;
    color: var(--ink, #211f1a);
  }
  /* The fallback when the clipboard is refused, so it has to be selectable. */
  .report {
    margin-top: 6px;
    max-height: 38vh;
    overflow: auto;
    white-space: pre-wrap;
    /* The user-agent string is one ~130-character unbroken token; without this it
       widens the dialog past the edge of a phone. */
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
  /* The time input is text-shaped, not a 17px checkbox square. `--popupPaper`,
     not `--popup`: the latter is not a palette variable, so it silently takes
     the light-cream fallback and vanishes on every dark theme. */
  .toggle input.time {
    width: auto;
    height: auto;
    font: inherit;
    font-size: calc(14px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 4px 6px;
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
  /* A radio with a description under its name. */
  .rich {
    align-items: flex-start;
  }
  .rich input {
    margin-top: 3px;
  }
  .rich .body {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .rich .name {
    font-size: calc(15px * var(--uiScale, 1));
  }
  .rich .desc {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .dropdown {
    font: inherit;
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    padding: 6px 10px;
    max-width: 100%;
    cursor: pointer;
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
    /* Restore is a `<label>` wrapping a file input, so app.css's 44px tap floor
       does not reach it — it stretches to the row height beside a button that
       the floor DID reach, and a label does not centre its own text. */
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
