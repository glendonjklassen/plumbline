<script lang="ts">
  // Backup and restore: their own surface, raised from the ≡ menu. Three ways
  // out — the encrypted backup (an age file under this device's five-word
  // phrase), the backup without a phrase (the plain zip), and restore, which
  // takes either — plus the phrase itself. Moved out of Settings so a reader
  // looking for it finds it where History and the guide are.
  //
  // Mounted for the whole session, like the dialog it came from: the notice
  // for a restore that failed after its reload is read on mount.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { zipRead, zipWrite } from "../engine/zip";
  import { isEncryptedBackup } from "../engine/backupCrypto";
  import { idbApply } from "../engine/idb";
  import { nowStamp } from "../engine/StudyEngine";
  import { plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  // Backup / restore: the authored home dirs as a zip. Must stay in step with
  // engine/home.ts's USER_DIRS — a dir missing from this restore filter exports
  // into the zip and is then silently dropped on the way back in.
  const BACKUP_DIRS = ["tags/", "threads/", "weaves/", "notes/", "memory/", "reading/", "plans/", "devotionals/", "deleted/", ".config/"];

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
  // notice lands whether or not Backup was open when the page came back.
  try {
    const carried = sessionStorage.getItem(RESTORE_FAILED);
    if (carried) {
      sessionStorage.removeItem(RESTORE_FAILED);
      restoreFailed = carried;
    }
  } catch {
    /* no session storage — then nothing was carried across either */
  }

  // ── backup ─────────────────────────────────────────────────────────────────
  // The backup is the home's authored dirs as a zip, sealed under a five-word
  // phrase (an age file — see crates/core/src/backup.rs), and handed to the
  // share sheet where there is one: a phone's Proton Drive, kDrive, Files, mail.
  // Where there is none it downloads. The phrase is made by the engine the
  // first time, shown once, and kept in localStorage so this device never asks
  // for it again; another device asks on restore and then keeps it too.
  //
  // localStorage, not the home: the home is what the backup CONTAINS, and a
  // phrase inside the file it unlocks helps nobody — least of all the plain
  // zip, which would carry it in the clear.
  const PHRASE_KEY = "plumbline:backupPhrase";

  function storedPhrase(): string | null {
    try {
      return localStorage.getItem(PHRASE_KEY);
    } catch {
      return null;
    }
  }
  /** The engine's own spelling of a phrase — trimmed, lowercase, single
   *  spaces — so what a reader typed with stray capitals is kept, and later
   *  shown, the way it was made. Mirrors `normalize_phrase` in the core. */
  const canonical = (phrase: string): string => phrase.trim().split(/\s+/).map((w) => w.toLowerCase()).join(" ");

  function rememberPhrase(phrase: string): void {
    try {
      localStorage.setItem(PHRASE_KEY, canonical(phrase));
    } catch {
      /* no storage: the next backup makes and shows a phrase again */
    }
  }

  /** This device's phrase — made and shown on first use. Null when the reader
   *  backed out of the dialog, in which case nothing is made or remembered. */
  async function ensurePhrase(): Promise<string | null> {
    const have = storedPhrase();
    if (have) return have;
    const phrase = await s.rpc.backupPhrase();
    if (!(await s.askPhrase(phrase))) return null;
    rememberPhrase(phrase);
    return phrase;
  }

  /** Show the phrase again, making one if this device has none yet. */
  async function showPhrase(): Promise<void> {
    const phrase = storedPhrase() ?? (await s.rpc.backupPhrase());
    if (await s.askPhrase(phrase)) rememberPhrase(phrase);
  }

  async function exportZip(): Promise<{ zip: Uint8Array; n: number }> {
    const files = new Map<string, Uint8Array>(await s.rpc.exportUserData());
    files.set(
      "plumbline-backup.json",
      new TextEncoder().encode(JSON.stringify({ format: 1, app: "web", exported: nowStamp() })),
    );
    return { zip: zipWrite(files), n: files.size - 1 };
  }

  function download(bytes: Uint8Array, name: string, type: string): void {
    const blob = new Blob([bytes as unknown as BlobPart], { type });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = name;
    a.click();
    URL.revokeObjectURL(a.href);
  }

  /** Hand the file over: the share sheet where the platform offers one for
   *  files, else a download. "declined" is the reader closing the sheet — not a
   *  failure, and not a backup either, so it gets no toast. */
  async function handOver(
    bytes: Uint8Array,
    name: string,
    type = "application/octet-stream",
  ): Promise<"shared" | "saved" | "declined"> {
    const file = new File([bytes as unknown as BlobPart], name, { type });
    if (navigator.canShare?.({ files: [file] })) {
      try {
        await navigator.share({ files: [file], title: name });
        return "shared";
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") return "declined";
        // Anything else — the gesture expired behind the dialog, the platform
        // refused the type — and the download still works.
      }
    }
    download(bytes, name, type);
    return "saved";
  }

  async function backup(): Promise<void> {
    try {
      const phrase = await ensurePhrase();
      if (!phrase) return;
      const { zip, n } = await exportZip();
      const sealed = await s.rpc.backupSeal(zip, phrase);
      if (sealed.error) throw new Error(sealed.error.message);
      const name = `plumbline-backup-${nowStamp().slice(0, 10)}.age`;
      const how = await handOver(sealed.bytes, name);
      // There is no completion signal for a download or a share, so the toast
      // names the file and says what we did, not that it landed anywhere.
      if (how !== "declined") s.showToast(plural("settings.backedUp.one", "settings.backedUp.other", n, { name }));
    } catch (err) {
      // A toast, not the blocking notice a failed restore gets: nothing was
      // written and the reader is looking at the button that is also the repair.
      // It must not stay silent, though — unguarded, a rejection made the button
      // look like a browser that saved the file somewhere unnoticed.
      const why = err instanceof Error ? err.message : String(err);
      s.showToast(t("settings.backupFailed", { why }));
    }
  }

  /** The backup without a phrase: the plain zip, handed over the same way. For
   *  a reader who would rather not keep a phrase and does not mind who reads
   *  the file. Never asks for, makes or remembers a phrase. */
  async function backupPlain(): Promise<void> {
    // Asked every time: this is the one way out that leaves the notes readable
    // to whoever ends up holding the file.
    const ok = await s.askConfirm(
      t("settings.backupPlainAsk"),
      t("settings.backupPlainBody"),
      t("settings.backupPlainVerb"),
    );
    if (!ok) return;
    const name = `plumbline-backup-${nowStamp().slice(0, 10)}.zip`;
    try {
      const { zip, n } = await exportZip();
      const how = await handOver(zip, name, "application/zip");
      if (how !== "declined") s.showToast(plural("settings.backedUp.one", "settings.backedUp.other", n, { name }));
    } catch (err) {
      const why = err instanceof Error ? err.message : String(err);
      s.showToast(t("settings.backupFailed", { why }));
    }
  }

  /** Open an encrypted backup: this device's phrase first, then ask — and ask
   *  again on a wrong one, with what was typed left in the field — until it
   *  opens or the reader cancels (null). Damage is thrown, like an unreadable
   *  zip. A phrase typed here becomes this device's own when it had none, so a
   *  restore onto a new device leaves it able to back up without asking. */
  async function unseal(file: Uint8Array): Promise<Uint8Array | null> {
    let phrase = storedPhrase();
    let typed = "";
    for (;;) {
      if (phrase === null) {
        const answer = await s.askText(t("settings.phraseAsk"), typed);
        if (answer === null) return null;
        phrase = typed = answer;
      }
      const opened = await s.rpc.backupOpen(file, phrase);
      if (opened.bytes) {
        if (!storedPhrase()) rememberPhrase(phrase);
        return opened.bytes;
      }
      if (opened.error.kind !== "wrongPhrase") throw new Error(opened.error.message);
      s.showToast(t("settings.restoreWrongPhrase"));
      phrase = null;
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
      let bytes: Uint8Array = new Uint8Array(await file.arrayBuffer());
      if (isEncryptedBackup(bytes)) {
        const opened = await unseal(bytes);
        if (!opened) return; // the reader gave up on the phrase: nothing changed
        bytes = opened;
      }
      const entries = await zipRead(bytes);
      // Only home-relative authored paths — no traversal, nothing else.
      const safe = new Map<string, Uint8Array>();
      for (const [path, bytes] of entries)
        if (BACKUP_DIRS.some((d) => path.startsWith(d)) && !path.includes(".."))
          safe.set(currentConfigPath(path), bytes);
      if (safe.size === 0) {
        s.showToast(t("settings.restoreNothing"));
        return;
      }
      // Merged INTO this device, not written over it (core `merge`): the same
      // note edited on both keeps the newer edit, different notes on a verse are
      // all kept, tags and plans take the union, and what either device deleted
      // stays deleted. Computed against this session's home before anything is
      // muted, so a failure here leaves the session whole.
      const merged = await s.rpc.backupMerge([...safe]);
      // The merged files are now the truth — nothing (incl. the pagehide
      // flush) may persist the current session over them; just reload.
      muted = true;
      s.restoring = true;
      await s.rpc.freeze(); // the debounced authoring persist must not fire either
      await idbApply("user", new Map(merged.write), merged.delete);
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
</script>

{#if s.showBackup}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => (s.showBackup = false)}></div>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t("shell.backup")}
    data-surface="backup"
    use:modal={{ close: () => (s.showBackup = false) }}
  >
    <h2>{t("shell.backup")}</h2>
    <p class="label">{t("settings.data")}</p>
    <!-- One column of full-width actions, each with its own line underneath, so
         every sentence sits with the button it explains and every button lines
         up with the next, whatever the length of its label in this language. -->
    <div class="actions">
      <button class="action" onclick={backup}>{t("settings.backup")}</button>
      <p class="desc-note">{t("settings.backupDesc")}</p>
      <button class="action" onclick={() => void backupPlain()}>{t("settings.backupPlain")}</button>
      <p class="desc-note">{t("settings.backupPlainDesc")}</p>
    </div>
    <hr />
    <div class="actions">
      <label class="action">
        {t("settings.restore")}
        <input type="file" accept=".zip,.age,application/zip,application/octet-stream" onchange={restore} hidden />
      </label>
      <p class="desc-note">{t("settings.restoreDesc")}</p>
      <button class="action" onclick={() => void showPhrase()}>{t("settings.phraseShow")}</button>
    </div>
    <button class="done" onclick={() => (s.showBackup = false)}>{t("settings.done")}</button>
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
  /* The Settings dialog's frame and controls, so the two read as one family. */
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
    max-height: calc(84vh - var(--bottomNavH, 0px));
    overflow-y: auto;
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
  .label {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .action {
    /* Full width, so the column lines up; centred, so a two-line label in a
       longer language still reads as one button. Restore is a `<label>` around
       a file input, which app.css's 44px tap floor does not reach, so the floor
       is stated here for both. */
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-height: 44px;
    padding: 6px 12px;
    text-align: center;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    cursor: pointer;
    font-size: calc(14px * var(--uiScale, 1));
  }
  /* A description belongs to the button above it: tight to it, clear of the next. */
  .action + .desc-note {
    margin: -2px 2px 6px;
  }
  hr {
    border: none;
    border-top: 1px solid var(--rule, #d8cba8);
    margin: 2px 0;
  }
  .action:hover {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
  }
  .desc-note {
    font-size: calc(11.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .done {
    align-self: flex-end;
    padding: 6px 18px;
    border: 1px solid var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
    border-radius: 7px;
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
  .err-dialog h2 {
    font-size: calc(17px * var(--uiScale, 1));
  }
  .err-body {
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.5;
    color: var(--faded, #8a8276);
  }
</style>
