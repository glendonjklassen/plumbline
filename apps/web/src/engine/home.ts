// The virtual home directory the engine sees through WASI: the read-only data
// pack (data/, bridge/) plus the user's authored files (tags/, threads/,
// weaves/, notes/, memory/, reading/, plans/, devotionals/, .config/) restored
// from IndexedDB. After every authoring write the user subtree is diffed back.

import { Directory, File } from "@bjorn3/browser_wasi_shim";
import { idbApply, idbEntries, idbGet } from "./idb";

/** Home-relative directories that hold user-authored state. */
const USER_DIRS = ["tags", "threads", "weaves", "notes", "memory", "reading", "plans", "devotionals", ".config"];
/** The corpus idxcache — rebuildable, persisted to skip the 19 MB re-parse.
 *  Sources, in preference order: this device's persisted copy (IndexedDB),
 *  else the pack-shipped web-stamped one (fetched on first visit). */
const IDXCACHE = "data/kjv.jsonl.idxcache";

/** Which pack version the persisted idxcache was built from. Without it a cache
 *  outlives the data update that invalidates it: its verses would be the old text,
 *  and the tokenization stamp (unchanged across data updates) wouldn't notice. */
const IDXCACHE_VERSION = "meta:idxcacheVersion";

/** Delete the legacy IndexedDB copy of the corpus cache — the depot has held the
 *  same file since the first visit, so it avoids no download and costs a 37 MB
 *  rewrite per launch. BY KEY, never by clearing the store: `meta:stockSeeded` and
 *  `meta:bundled` live there too, and losing stockSeeded re-seeds the stock set,
 *  resurrecting every stock weave the reader deliberately threw away. */
export async function dropLegacyIdxcache(): Promise<number> {
  const existing = await idbGet("cache", IDXCACHE);
  if (!existing) return 0;
  await idbApply("cache", new Map(), [IDXCACHE, IDXCACHE_VERSION]);
  return existing.byteLength;
}

export interface VirtualHome {
  /** Root contents map handed to PreopenDirectory("/home", …). */
  root: Map<string, Directory | File>;
  /** Whether the cache for the corpus this home will open was restored into it —
   *  when true the engine open takes the fast path (no 19 MB re-parse). Either
   *  corpus counts: stage 1 inflates exactly one (see `corpusRoleFor`), so at most
   *  one is ever here and either one means the fast path fired. */
  hadIdxcache: boolean;
  /** Insert read-only pack files into the live home (the WASI shim resolves
   *  paths on open, so the engine sees them immediately) — the late R&D pack. */
  addFiles(files: Map<string, Uint8Array>): void;
  /** Drop pack files the engine has finished reading, freeing their bytes.
   *  Returns how many bytes went. See `evict` below for the rules. */
  evict(paths: string[]): number;
  /** Diff the user subtree against IndexedDB (call after authoring writes). Writes
   *  only files THIS session changed and deletes only files it removed, so
   *  concurrent tabs can't clobber each other — see the contract inline. REJECTS
   *  when the write did not land, which the caller owes the reader: the bytes then
   *  exist only in memory. Every dirty file stays dirty, so the next persist
   *  retries the backlog. */
  persistUserData(): Promise<void>;
  /** Persist ONE user directory, additively — no whole-subtree diff, no deletions.
   *  For writes on a TIMER rather than a human action: the reading map reports
   *  dwell every 30 s, and a full rewrite of every note and memory card on that
   *  timer would spend a phone's IndexedDB budget, and the one thread that answers
   *  taps, on work nothing asked for. Rejects on a failed write. */
  persistUserDir(dir: string): Promise<void>;
  /** Snapshot of the authored files (for the backup zip). */
  exportUserData(): Map<string, Uint8Array>;
  /** Stop ALL persistence (a restore is pending reload — nothing may write). */
  freeze(): void;
  /** Whether the bundled stock study set is enabled. */
  bundledOn: boolean;
  /** Flip the bundled set (removes/reseeds the stock files); reload after. */
  setBundled(on: boolean): Promise<void>;
  /** Whether the optional suggested-weave set is already installed here. */
  suggestedInstalled: boolean;
  /** The language codes whose optional corpus is already installed here. */
  langsInstalled: Set<string>;
  /** Put a downloaded corpus cache in `data/` and persist the marker.
   *  Rejects if the write did not land, like `installSuggestedWeaves`. */
  installLangCorpus(code: string, cachePath: string, cache: Uint8Array): Promise<void>;
  /** Unpack the downloaded suggested-weave bundle into `weaves/suggested/` and
   *  persist it. Returns how many files were written (0 if the reader already has
   *  them all). Rejects if the write did not land, like `persistUserData`. */
  installSuggestedWeaves(bundle: Uint8Array): Promise<number>;
}

const STOCK_SEEDED = "meta:stockSeeded";
const BUNDLED = "meta:bundled";
/** Set once the suggested-weave bundle has been unpacked here. Separate from
 *  STOCK_SEEDED so that turning the bundled set off and on again does not silently
 *  re-download 422 KB. */
const SUGGESTED_INSTALLED = "meta:suggestedInstalled";
/** The language codes whose corpus this home has taken, comma-separated. A marker
 *  rather than "is the file in the home", because the home evicts pack files under
 *  `data/` once read. One key for all languages, since its reader (stage 1, before
 *  there is an engine) has no language list to ask about. */
const LANGS_INSTALLED = "meta:langsInstalled";
/** German's original marker, read forever and written no more: dropping it would
 *  unname a 28 MB file those devices already have, prune would reclaim it, and the
 *  next boot would fetch it again. */
const GERMAN_INSTALLED_LEGACY = "meta:germanInstalled";
const enc = new TextEncoder();
const dec = new TextDecoder();

function ensureDir(root: Map<string, Directory | File>, path: string): Directory {
  let contents = root;
  let dir: Directory | undefined;
  for (const part of path.split("/")) {
    let next = contents.get(part);
    if (!(next instanceof Directory)) {
      next = new Directory(new Map());
      contents.set(part, next);
    }
    dir = next;
    contents = next.contents as Map<string, Directory | File>;
  }
  return dir!;
}

function insertFile(root: Map<string, Directory | File>, path: string, bytes: Uint8Array): void {
  const slash = path.lastIndexOf("/");
  const contents =
    slash < 0 ? root : (ensureDir(root, path.slice(0, slash)).contents as Map<string, Directory | File>);
  contents.set(path.slice(slash + 1), new File(bytes));
}

/** Find the directory a home-relative path lives in, WITHOUT creating anything —
 *  both callers below are asking what is there, and `ensureDir` would answer by
 *  minting the directories it was looking for. */
function locate(
  root: Map<string, Directory | File>,
  path: string,
): { contents: Map<string, Directory | File>; name: string } | undefined {
  const parts = path.split("/");
  const name = parts.pop();
  if (!name) return undefined;
  let contents = root;
  for (const part of parts) {
    const next = contents.get(part);
    if (!(next instanceof Directory)) return undefined;
    contents = next.contents as Map<string, Directory | File>;
  }
  return { contents, name };
}

/** The live bytes at a home-relative path, or nothing if no FILE is there. */
function readFile(root: Map<string, Directory | File>, path: string): Uint8Array | undefined {
  const at = locate(root, path);
  const node = at?.contents.get(at.name);
  return node instanceof File ? (node as File).data : undefined;
}

/** Drop a file from the live home. Directories are never touched. */
function removeFile(root: Map<string, Directory | File>, path: string): void {
  const at = locate(root, path);
  if (at && at.contents.get(at.name) instanceof File) at.contents.delete(at.name);
}

/** Byte-for-byte equality — NOT a fingerprint. `fingerprint` may collide because
 *  a collision there costs one skipped write; the caller here is deciding whether
 *  to DELETE the reader's file, where a collision would destroy their work. */
function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** A leftover of an interrupted atomic write (the core renames a hidden sibling
 *  over the target), or one that rode in on a restored backup. Not the reader's
 *  data: no loader reads one, and left alone it ships in every backup after.
 *
 *  Mirrors `store::is_temp_name` in crates/core, and all three parts must match:
 *  "starts with a dot" alone takes `.config`, where the settings live, and "ends
 *  with .tmp" alone takes a `notes.tmp` from someone else's archive.
 *  `config.json.bad`, the rescue copy of damaged settings, matches none. */
const TEMP_NAME = /^\.[^/]+\.[0-9]+\.tmp$/;

/** Walk a user directory, collecting home-relative path → bytes.
 *
 *  The one gate everything downstream passes through — both persists and the backup
 *  zip's `exportUserData` — so the temp rule is applied here and nowhere else. Only
 *  FILE names are tested: nothing mints a temp directory, and testing directory
 *  names is how a loose rule swallows `.config`. */
function collectFiles(prefix: string, dir: Directory, out: Map<string, Uint8Array>): void {
  for (const [name, node] of dir.contents as Map<string, Directory | File>) {
    const path = `${prefix}/${name}`;
    if (node instanceof Directory) collectFiles(path, node, out);
    else if (node instanceof File && !TEMP_NAME.test(name)) out.set(path, (node as File).data);
  }
}

/** Content fingerprint for the dirty-file check (FNV-1a folded with length).
 *  Computed fresh from the live bytes on every use — never a reference to the
 *  shim's `File.data`, which the engine mutates in place. */
function fingerprint(bytes: Uint8Array): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    h ^= bytes[i];
    h = Math.imul(h, 0x01000193);
  }
  return `${bytes.length}:${h >>> 0}`;
}

/** The subset of `current` whose bytes differ from what `synced` recorded —
 *  the files THIS session changed (or created), with their new fingerprints. */
function changedFiles(
  current: Map<string, Uint8Array>,
  synced: Map<string, string>,
): { puts: Map<string, Uint8Array>; prints: Map<string, string> } {
  const puts = new Map<string, Uint8Array>();
  const prints = new Map<string, string>();
  for (const [path, bytes] of current) {
    const print = fingerprint(bytes);
    if (synced.get(path) !== print) {
      puts.set(path, bytes);
      prints.set(path, print);
    }
  }
  return { puts, prints };
}

/** Which optional pack files this device has already taken, read straight from
 *  IndexedDB. Its own function because it must answer BEFORE the home exists:
 *  stage 1 has to know which corpus to carry, and the home it would ask is built
 *  from stage 1's own result. */
export async function installedOptional(): Promise<{ suggestedInstalled: boolean; langsInstalled: Set<string> }> {
  const [suggested, langs, german] = await Promise.all([
    idbGet("cache", SUGGESTED_INSTALLED),
    idbGet("cache", LANGS_INSTALLED),
    idbGet("cache", GERMAN_INSTALLED_LEGACY),
  ]);
  return { suggestedInstalled: suggested !== undefined, langsInstalled: decodeLangs(langs, german) };
}

/** The installed set, from the current key and the legacy German one. */
function decodeLangs(langs: Uint8Array | undefined, german: Uint8Array | undefined): Set<string> {
  const out = new Set<string>(
    langs
      ? dec
          .decode(langs)
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean)
      : [],
  );
  if (german !== undefined) out.add("de");
  return out;
}

export async function buildHome(
  pack: Map<string, Uint8Array>,
  stockPaths: Set<string> = new Set(),
): Promise<VirtualHome> {
  const root = new Map<string, Directory | File>();
  const [userFiles, seededFlag, bundledFlag, suggestedFlag, langsFlag, germanFlag] = await Promise.all([
    idbEntries("user"),
    idbGet("cache", STOCK_SEEDED),
    idbGet("cache", BUNDLED),
    idbGet("cache", SUGGESTED_INSTALLED),
    idbGet("cache", LANGS_INSTALLED),
    idbGet("cache", GERMAN_INSTALLED_LEGACY),
  ]);
  const bundledOn = bundledFlag ? dec.decode(bundledFlag) !== "off" : true;
  // Both are mutable and read through getters below: an install has to change the
  // answer inside the session that made it, or Settings keeps offering a download
  // the reader has already completed.
  let suggestedOn = suggestedFlag !== undefined;
  const langsOn = decodeLangs(langsFlag, germanFlag);
  // The stock set seeds ONCE: after that the user's own copies rule, so edits and
  // deletions stick across pack updates.
  const seedStock = bundledOn && !seededFlag;

  // The PRISTINE bytes of every bundled stock file, kept for the session so the OFF
  // toggle can tell an untouched example from one the reader made their own (see
  // `setBundled`). ~64 KB, held here rather than re-fetched because the toggle must
  // work offline. Only paths the pack delivered: a stock path whose pristine bytes
  // are unknown is one the toggle may never delete, having nothing to compare to.
  const pristineStock = new Map<string, Uint8Array>();
  for (const [path, bytes] of pack) if (stockPaths.has(path)) pristineStock.set(path, bytes);

  for (const [path, bytes] of pack) {
    if (stockPaths.has(path) && !seedStock) continue;
    insertFile(root, path, bytes);
  }

  // Authoring dirs must exist even when empty (the engine lists them), and
  // weaves/suggested is part of the expected shape.
  for (const d of USER_DIRS) ensureDir(root, d);
  ensureDir(root, "weaves/suggested");

  // Restore the user's files from previous sessions (their copies overwrite
  // freshly-seeded stock — theirs is newer). Stranded temps come back too, on
  // purpose: they must be in `synced` for the next persist to sweep them (see
  // persistUserData), and in the tree they are inert — no loader opens one.
  for (const [path, bytes] of userFiles) insertFile(root, path, bytes);

  // What this session last saw in IndexedDB: path → content fingerprint. These are
  // the multi-tab contract — persists compare against them so a file another tab
  // wrote is never overwritten by our stale copy.
  const synced = new Map<string, string>();
  for (const [path, bytes] of userFiles) synced.set(path, fingerprint(bytes));
  let frozen = false;

  // ONE persist at a time. Each diffs the live tree against `synced` and rewrites
  // `synced` only after its transaction commits, so two overlapping calls would
  // diff the same snapshot and write the same files twice — and the flush that
  // closes the debounce window (engine.worker.ts) is exactly that overlap.
  //
  // They QUEUE rather than join: a flush that merely awaited the attempt in flight
  // would return before the write the reader made after that attempt took its diff,
  // which is the write the flush exists to save.
  let chain: Promise<unknown> = Promise.resolve();
  function serial<T>(f: () => Promise<T>): Promise<T> {
    // `.then(f, f)`: the next persist runs however the previous one ended, so a
    // quota failure does not strand the whole queue behind it.
    const run = chain.then(f, f);
    chain = run.catch(() => {});
    return run;
  }

  if (seedStock) {
    // Persist the seeded stock as the user's own files + set the marker.
    const seeded = new Map<string, Uint8Array>();
    for (const d of USER_DIRS) {
      const dir = root.get(d);
      if (dir instanceof Directory) collectFiles(d, dir, seeded);
    }
    await idbApply("user", seeded);
    await idbApply("cache", new Map([[STOCK_SEEDED, enc.encode("1")]]));
    synced.clear();
    for (const [path, bytes] of seeded) synced.set(path, fingerprint(bytes));
  }

  return {
    root,
    // ANY corpus cache, by extension rather than by name: which one arrived depends
    // on the reader's language, and a filename list is one more place a new
    // language would have to be remembered.
    hadIdxcache: [...pack.keys()].some((p) => p.endsWith(".idxcache")),
    addFiles(files: Map<string, Uint8Array>) {
      for (const [path, bytes] of files) insertFile(root, path, bytes);
    },
    /** Drop a read pack file's bytes out of the in-memory home. The engine holds
     *  its own parsed copy in wasm memory and the WASI shim's `File` constructor
     *  COPIES what it is given, so the node here is a genuine second copy — 37 MB
     *  for the corpus cache alone.
     *
     *  TWO HARD RULES, both protecting the reader's data:
     *
     *  1. `data/` ONLY. `persistUserData` derives deletions by diffing this tree
     *     against IndexedDB, so evicting under tags/ threads/ weaves/ notes/
     *     memory/ .config would make the reader's very next authoring write DELETE
     *     it permanently, and the backup zip (built from the same tree) would ship
     *     truncated. Enforced below, not merely documented.
     *  2. Only paths whose single reader has provably finished; callers pass those
     *     explicitly, nothing is inferred. `data/kjv-notes.jsonl` never qualifies —
     *     `load_study` re-reads it on every authoring write, so evicting it empties
     *     the 1769 margin notes the moment a note is saved. Nor do
     *     `data/cross-references.tsv` and `bridge/*`, which load through lazy cells
     *     that can fire on any later tap.
     *
     *  Frees the steady state, not the peak: at boot the corpus cache exists as the
     *  gunzip output, this node's copy, the shim's per-read slice and the engine's
     *  parsed copy. */
    evict(paths: string[]): number {
      const dataDir = root.get("data");
      if (!(dataDir instanceof Directory)) return 0;
      const contents = dataDir.contents as Map<string, Directory | File>;
      let freed = 0;
      for (const path of paths) {
        const [dir, ...rest] = path.split("/");
        // Rule 1, enforced rather than documented.
        if (dir !== "data" || rest.length !== 1) continue;
        const node = contents.get(rest[0]);
        if (!(node instanceof File)) continue;
        freed += (node as File).data.byteLength;
        contents.delete(rest[0]);
      }
      return freed;
    },
    exportUserData() {
      const out = new Map<string, Uint8Array>();
      for (const d of USER_DIRS) {
        const dir = root.get(d);
        if (dir instanceof Directory) collectFiles(d, dir, out);
      }
      return out;
    },
    bundledOn,
    get suggestedInstalled() {
      return suggestedOn;
    },
    get langsInstalled() {
      return langsOn;
    },
    async installLangCorpus(code: string, cachePath: string, cache: Uint8Array) {
      if (frozen) return;
      await serial(async () => {
        if (frozen) return;
        // Into the live home so this session can read it, and then ONLY THE MARKER
        // into IndexedDB — not the 28 MB. The bytes are already in the depot (where
        // `fetchLangCorpus` put them) and the depot is what boot reads; writing them
        // to IndexedDB too is the mistake `dropLegacyIdxcache` undoes for English.
        insertFile(root, cachePath, cache);
        langsOn.add(code);
        await idbApply("cache", new Map([[LANGS_INSTALLED, enc.encode([...langsOn].join(","))]]));
      });
    },
    async installSuggestedWeaves(bundle: Uint8Array) {
      if (frozen) return 0;
      // `{ "<name>.json": "<file text>" }` — the text verbatim, never
      // re-serialized, so these stay the maintainer's own bytes.
      const parsed: unknown = JSON.parse(dec.decode(bundle));
      if (typeof parsed !== "object" || parsed === null) throw new Error("suggested-weave bundle is not an object");

      const fresh = new Map<string, Uint8Array>();
      for (const [name, text] of Object.entries(parsed as Record<string, unknown>)) {
        // A bundle is data off the network, so it may not reach outside the one
        // directory it is allowed to fill.
        if (typeof text !== "string" || name.includes("/") || name.startsWith(".")) continue;
        const path = `weaves/suggested/${name}`;
        // The reader's copy wins, as with seeded stock: a file already here was
        // either had before or written, and an install must overwrite neither.
        if (readFile(root, path)) continue;
        fresh.set(path, enc.encode(text));
      }

      return serial(async () => {
        if (frozen) return 0;
        for (const [path, bytes] of fresh) {
          insertFile(root, path, bytes);
          // Registered as pristine so the bundled-set OFF toggle can remove an
          // untouched suggestion later; without this they are indistinguishable
          // from the reader's own work and never removable.
          pristineStock.set(path, bytes);
        }
        // The marker goes in the SAME transaction as the files: two writes could
        // leave a home marked installed with nothing in it, and the Settings row
        // would then offer no way to try again.
        await idbApply("user", fresh);
        await idbApply("cache", new Map([[SUGGESTED_INSTALLED, enc.encode("1")]]));
        for (const [path, bytes] of fresh) synced.set(path, fingerprint(bytes));
        // Only now, with bytes and marker both down: this session's answer to "do
        // you have them?" changes when the store's does.
        suggestedOn = true;
        return fresh.size;
      });
    },
    async setBundled(on: boolean) {
      // A restore is pending reload — nothing may write (see `freeze`). The
      // restored user store is about to become the truth, and deleting stock paths
      // out of it would take files the archive just put back.
      if (frozen) return;
      await idbApply("cache", new Map([[BUNDLED, enc.encode(on ? "on" : "off")]]));
      if (on) {
        // Re-seed on next boot: missing stock files come back, kept edits win.
        await idbApply("cache", new Map(), [STOCK_SEEDED]);
        return;
      }
      // OFF removes the examples, not the reader's work: a stock thread or weave
      // they renamed, re-noted or added verses to must survive a toggle that reads
      // as "hide the examples". Their copy wins on the way in, and this is the same
      // invariant on the way out.
      //
      // The test is RAW BYTES against the bundled file — never a normalised form,
      // never a fingerprint. The seed writes the pack's bytes verbatim, so an
      // untouched stock file is byte-identical by construction, while any authoring
      // write re-serializes through the core and differs in whitespace and key order
      // even before the edit. Comparing a normalised form would call a re-saved file
      // pristine when the reader's change lives in a field the normaliser drops;
      // byte equality can only err toward keeping.
      //
      // BOTH COPIES MUST AGREE: the live tree can hold an unpersisted edit and
      // IndexedDB can hold one another tab made after our boot, so a path is
      // removable only when neither copy differs from the bundled bytes.
      await serial(async () => {
        if (frozen) return;
        const stored = await idbEntries("user");
        const removable: string[] = [];
        for (const [path, pristine] of pristineStock) {
          const live = readFile(root, path);
          if (live && !sameBytes(live, pristine)) continue;
          const saved = stored.get(path);
          if (saved && !sameBytes(saved, pristine)) continue;
          removable.push(path);
        }
        if (removable.length === 0) return;
        await idbApply("user", new Map(), removable);
        // Gone from the tree AND from `synced`, so the next persist neither
        // resurrects them nor deletes them a second time.
        for (const path of removable) {
          synced.delete(path);
          removeFile(root, path);
        }
      });
    },
    freeze() {
      frozen = true;
    },
    persistUserData() {
      return serial(async () => {
        if (frozen) return;
        const current = new Map<string, Uint8Array>();
        for (const d of USER_DIRS) {
          const dir = root.get(d);
          if (dir instanceof Directory) collectFiles(d, dir, current);
        }
        // THE MULTI-TAB CONTRACT. Two tabs share one IndexedDB but each holds its
        // own in-memory home, snapshotted at ITS boot, so writing the whole subtree
        // would let the slower tab's stale copies overwrite the faster tab's edits
        // and resurrect files deleted over there. So:
        //
        //  * write ONLY files whose bytes differ from what we last synced — a file
        //    we didn't change is one we have no opinion about;
        //  * delete ONLY files WE removed (in `synced`, gone from our tree) —
        //    absence elsewhere is someone else's decision.
        //
        // Both tabs editing the same file stays last-writer-wins per file; no
        // cross-tab lock is needed, since IndexedDB serialises readwrite
        // transactions per store. A flush changes none of this: it runs the same
        // diff a moment earlier, and the fingerprints decide, not the caller.
        //
        // One deletion this session makes without having touched the file: a
        // stranded temp. `collectFiles` no longer collects them, so one an older
        // build persisted — or one that rode in on a restored backup — is in
        // `synced`, absent from `current`, and swept here. The sweep reaches nothing
        // else: `deletes` is exactly `synced` minus `current`, and the only names
        // the filter newly withholds are those matching TEMP_NAME.
        const { puts, prints } = changedFiles(current, synced);
        const deletes = [...synced.keys()].filter((k) => !current.has(k));
        if (puts.size === 0 && deletes.length === 0) return;
        await idbApply("user", puts, deletes);
        // Only after the transaction commits: a failed persist must leave every
        // dirty file dirty, so the next write retries the whole backlog.
        for (const k of deletes) synced.delete(k);
        for (const [k, p] of prints) synced.set(k, p);
      });
    },
    persistUserDir(d: string) {
      return serial(async () => {
        if (frozen) return;
        // An unknown directory here would write paths `persistUserData` never
        // collects, and its diff would then delete them on the next authoring write.
        if (!USER_DIRS.includes(d)) return;
        const current = new Map<string, Uint8Array>();
        const dir = root.get(d);
        if (dir instanceof Directory) collectFiles(d, dir, current);
        // Additive only: deletions belong to the diffing path, since a timer-driven
        // write has no business deciding something is gone. Same dirty-only rule as
        // persistUserData — on the 30 s dwell tick, one book's file.
        const { puts, prints } = changedFiles(current, synced);
        if (puts.size === 0) return;
        await idbApply("user", puts);
        for (const [k, p] of prints) synced.set(k, p);
      });
    },
  };
}
