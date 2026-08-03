// The virtual home directory the engine sees through WASI: the read-only data
// pack (data/, bridge/) plus the user's authored files (tags/, threads/,
// weaves/, notes/, memory/, reading/, .config/) restored from IndexedDB. After every
// authoring write the user subtree is diffed back to IndexedDB — the browser
// twin of the desktop shells' "engine reloads from disk after any write".

import { Directory, File } from "@bjorn3/browser_wasi_shim";
import { idbApply, idbEntries, idbGet } from "./idb";

/** Home-relative directories that hold user-authored state. */
const USER_DIRS = ["tags", "threads", "weaves", "notes", "memory", "reading", ".config"];
/** The corpus idxcache — rebuildable, persisted to skip the 19 MB re-parse.
 *  Sources, in preference order: this device's persisted copy (IndexedDB),
 *  else the pack-shipped web-stamped one (fetched on first visit). */
const IDXCACHE = "data/kjv.jsonl.idxcache";

/** Which pack version the persisted idxcache was built from. A cache outlives
 *  the data update that invalidates it otherwise: its verses would be the old
 *  text, and the tokenization stamp (unchanged across data updates) wouldn't
 *  notice. */
const IDXCACHE_VERSION = "meta:idxcacheVersion";

/** Delete the LEGACY IndexedDB copy of the corpus cache.
 *
 *  It used to be the fast path: boot probed IndexedDB before fetching, to avoid
 *  re-downloading 3.3 MB. It was never actually buying that — the depot has held
 *  the same file since the first visit, so the "download" it avoided was already
 *  a local read. What it did cost was real: `persistIdxcache` wrote 37 MB back
 *  into IndexedDB on EVERY launch, including launches that had just read those
 *  same bytes out of it, and it kept a second full copy of the corpus on disk.
 *
 *  Deleted BY KEY, never by clearing the store: `meta:stockSeeded` and
 *  `meta:bundled` live in there too, and they are decisions rather than data —
 *  losing stockSeeded re-seeds the stock set on the next boot, which resurrects
 *  every stock weave the reader deliberately threw away. */
export async function dropLegacyIdxcache(): Promise<number> {
  const existing = await idbGet("cache", IDXCACHE);
  if (!existing) return 0;
  await idbApply("cache", new Map(), [IDXCACHE, IDXCACHE_VERSION]);
  return existing.byteLength;
}

export interface VirtualHome {
  /** Root contents map handed to PreopenDirectory("/home", …). */
  root: Map<string, Directory | File>;
  /** Whether a persisted corpus idxcache was restored into this home — when
   *  true the engine open should take the fast path (no 19 MB re-parse). */
  hadIdxcache: boolean;
  /** Insert read-only pack files into the live home (the WASI shim resolves
   *  paths on open, so the engine sees them immediately) — the late R&D pack. */
  addFiles(files: Map<string, Uint8Array>): void;
  /** Drop pack files the engine has finished reading, freeing their bytes.
   *  Returns how many bytes went. See `evict` below for the rules. */
  evict(paths: string[]): number;
  /** Diff the user subtree against IndexedDB (call after authoring writes).
   *  Writes only files THIS session changed and deletes only files it removed,
   *  so concurrent tabs can't clobber each other — see the contract inline.
   *
   *  REJECTS when the write did not land (quota, blocked storage, a browser that
   *  dropped the database). The caller owes the reader that news: the bytes exist
   *  only in this in-memory home and die with the tab. Every dirty file stays
   *  dirty, so the next persist retries the whole backlog. */
  persistUserData(): Promise<void>;
  /** Persist ONE user directory, additively — no whole-subtree diff and no
   *  deletions. For writes that fire on a TIMER rather than on a human action:
   *  the reading map reports dwell every 30 s while someone reads, and putting
   *  a full rewrite of every note and memory card on that timer would spend a
   *  phone's IndexedDB budget (and the one worker thread that answers taps) on
   *  work nothing asked for. See `persistUserData` for the diffing version.
   *  Rejects on a failed write, like its sibling. */
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
  /** Whether the optional German corpus is already installed here. */
  germanInstalled: boolean;
  /** Put the downloaded German corpus cache in `data/` and persist the marker.
   *  Rejects if the write did not land, like `installSuggestedWeaves`. */
  installGermanCorpus(cache: Uint8Array): Promise<void>;
  /** Unpack the downloaded suggested-weave bundle into `weaves/suggested/` and
   *  persist it. Returns how many files were written (0 if the reader already
   *  has them all). Rejects if the write did not land — the caller owes the
   *  reader that news, exactly as with `persistUserData`. */
  installSuggestedWeaves(bundle: Uint8Array): Promise<number>;
}

const STOCK_SEEDED = "meta:stockSeeded";
const BUNDLED = "meta:bundled";
/** Set once the suggested-weave bundle has been unpacked into this home. It is
 *  a separate marker from STOCK_SEEDED on purpose: the two sets arrive by
 *  different routes (one at open, one when asked for) and a reader who turns
 *  the bundled set off and on again must not silently re-acquire 422 KB they
 *  never asked for a second time. */
const SUGGESTED_INSTALLED = "meta:suggestedInstalled";
/** Set once the German corpus cache has been stored in this home.
 *
 *  A MARKER rather than "is the file in the home", because the in-memory home
 *  evicts pack files under `data/` once the engine has read them — so the file's
 *  absence proves nothing, and the pin would flip between claiming and
 *  disclaiming the download depending on when it was asked. */
const GERMAN_INSTALLED = "meta:germanInstalled";
/** Where the German corpus's start-up cache lives in the home. */
export const GERMAN_CACHE = "data/luther1912.jsonl.idxcache";
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

/** Find the directory a home-relative path lives in, WITHOUT creating anything.
 *  Nothing comes back when a parent is missing — both callers below are asking a
 *  question about what is there, and `ensureDir` would answer it by minting the
 *  directories it was looking for. */
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

/** Byte-for-byte equality — NOT a fingerprint.
 *
 *  `fingerprint` below is allowed to collide because the cost of a collision
 *  there is one skipped write of a file that gets rewritten on the next edit.
 *  The one caller here is deciding whether to DELETE the reader's file, and a
 *  collision would delete their work. Nothing but the bytes will do. */
function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** A leftover of an interrupted atomic write. The core writes to a hidden
 *  sibling and renames it over the target, so a session killed in between — or
 *  an Android one, whose backups we restore — strands a `.<name>.<digits>.tmp`
 *  in an authored dir. It is not the reader's data: the write that made it
 *  either landed under its real name or came back as an error, no loader can
 *  read one (they take `*.json`), and one that reaches a backup zip is restored
 *  onto the next device as a permanent fixture.
 *
 *  The rule is `store::is_temp_name` in crates/core, restated here because a
 *  filename check has no business crossing into wasm. All three parts must
 *  match, and nothing looser will do: "starts with a dot" takes `.config`,
 *  where the reader's settings live; "ends with .tmp" takes a `notes.tmp` that
 *  arrived in someone else's archive. `config.json.bad` — the rescue copy of
 *  damaged settings, which must keep riding along in backups — matches none of
 *  the three. */
const TEMP_NAME = /^\.[^/]+\.[0-9]+\.tmp$/;

/** Walk a user directory, collecting home-relative path → bytes.
 *
 *  The ONE gate everything downstream passes through — both persists and the
 *  backup zip's `exportUserData` — so the temp rule is applied here and nowhere
 *  else. Only FILE names are tested: nothing mints a temp directory, and testing
 *  directory names is exactly how a loose rule swallows `.config`. */
function collectFiles(prefix: string, dir: Directory, out: Map<string, Uint8Array>): void {
  for (const [name, node] of dir.contents as Map<string, Directory | File>) {
    const path = `${prefix}/${name}`;
    if (node instanceof Directory) collectFiles(path, node, out);
    else if (node instanceof File && !TEMP_NAME.test(name)) out.set(path, (node as File).data);
  }
}

/** Content fingerprint for the dirty-file check (FNV-1a folded with length).
 *  Computed fresh from the live bytes on every use — never a reference to the
 *  shim's `File.data`, which the engine mutates in place. A collision would
 *  skip one write of one changed file, and needs the same length AND the same
 *  32-bit hash across an edit of the same small JSON file (~2⁻³² per edit). */
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

/**
 * Which optional pack files this device has already taken, read straight from
 * IndexedDB.
 *
 * BEFORE the home exists, which is why it is its own function: stage 1 has to
 * know whether to carry the German corpus, and the home it would ask is built
 * from stage 1's own result. One `idbGet` against a marker key, on the boot path
 * — cheaper than the alternative, which was a second boot to notice.
 */
export async function installedOptional(): Promise<{ suggestedInstalled: boolean; germanInstalled: boolean }> {
  const [suggested, german] = await Promise.all([
    idbGet("cache", SUGGESTED_INSTALLED),
    idbGet("cache", GERMAN_INSTALLED),
  ]);
  return { suggestedInstalled: suggested !== undefined, germanInstalled: german !== undefined };
}

export async function buildHome(
  pack: Map<string, Uint8Array>,
  stockPaths: Set<string> = new Set(),
): Promise<VirtualHome> {
  const root = new Map<string, Directory | File>();
  const [userFiles, seededFlag, bundledFlag, suggestedFlag, germanFlag] = await Promise.all([
    idbEntries("user"),
    idbGet("cache", STOCK_SEEDED),
    idbGet("cache", BUNDLED),
    idbGet("cache", SUGGESTED_INSTALLED),
    idbGet("cache", GERMAN_INSTALLED),
  ]);
  const bundledOn = bundledFlag ? dec.decode(bundledFlag) !== "off" : true;
  // Whether the suggested-weave bundle is in this home. MUTABLE, and read
  // through a getter below: an install has to change the answer within the
  // session that made it, or Settings keeps offering a download the reader has
  // already completed. A plain boolean captured here reads correctly at boot
  // and then lies until the next reload.
  let suggestedOn = suggestedFlag !== undefined;
  // Same shape, same reason: an install has to change the answer inside the
  // session that made it, or Settings keeps offering a completed download.
  let germanOn = germanFlag !== undefined;
  // The stock set seeds ONCE (Android parity): after that the user's own
  // copies rule, so edits and deletions stick across pack updates.
  const seedStock = bundledOn && !seededFlag;

  // The PRISTINE bytes of every bundled stock file, kept for the session so the
  // OFF toggle can tell an untouched example from one the reader made their own
  // (see `setBundled`). ~64 KB for 30 small JSON files, and it is here rather
  // than re-fetched at toggle time because the toggle must work offline.
  //
  // Held only for paths the pack actually delivered: `build-web-pack.mjs` forces
  // every `seedOnce` entry to stage "text", so on both boot paths they arrive in
  // this map. A stock path whose pristine bytes are NOT known is one the toggle
  // may never delete — no reference to compare against is no proof it is
  // untouched.
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

  // What this session last saw in IndexedDB: path → content fingerprint.
  // The fingerprints are the multi-tab contract — persists compare against
  // them so a file another tab wrote is never overwritten by our stale copy.
  const synced = new Map<string, string>();
  for (const [path, bytes] of userFiles) synced.set(path, fingerprint(bytes));
  let frozen = false;

  // ONE persist at a time. Every persist below diffs the live tree against
  // `synced` and rewrites `synced` only after its transaction commits, so two
  // overlapping calls diff against the same snapshot and write the same files
  // twice — and the flush that closes the debounce window (engine.worker.ts) is
  // exactly that overlap.
  //
  // They QUEUE rather than join. A flush that merely awaited the attempt already
  // in flight would return before the write the reader made AFTER that attempt
  // took its diff — which is the write the flush exists to save.
  let chain: Promise<unknown> = Promise.resolve();
  function serial<T>(f: () => Promise<T>): Promise<T> {
    // `.then(f, f)`: the next persist runs however the previous one ended. A
    // quota failure must not strand the whole queue behind it.
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
    hadIdxcache: pack.has(IDXCACHE),
    addFiles(files: Map<string, Uint8Array>) {
      for (const [path, bytes] of files) insertFile(root, path, bytes);
    },
    /** Drop a read pack file's bytes out of the in-memory home.
     *
     *  The engine holds its own parsed copy in wasm memory, and the WASI shim's
     *  `File` constructor COPIES what it is given (`new Uint8Array(data)`), so
     *  the node here is a genuine second copy of the bytes — 37 MB of it for the
     *  corpus cache alone. Nothing re-opens these paths after the stage that
     *  reads them, so the node is pure duplication once that stage is done.
     *
     *  TWO HARD RULES, both of which protect the reader's data:
     *
     *  1. `data/` ONLY. `persistUserData` works out deletions by diffing this
     *     tree against IndexedDB, so evicting anything under tags/ threads/
     *     weaves/ notes/ memory/ .config would make the reader's very next
     *     authoring write DELETE it from IndexedDB — permanently. The backup zip
     *     is built from the same tree, so it would quietly ship truncated. The
     *     guard is a rule rather than care: user dirs are unreachable from here.
     *  2. Only paths whose single reader has provably finished. Callers pass
     *     those explicitly; nothing is inferred. `data/kjv-notes.jsonl` is NOT
     *     among them and never can be — `load_study` re-reads it on every
     *     authoring write, so evicting it empties the 1769 margin notes the
     *     moment the reader saves a note. Nor is `data/cross-references.tsv`
     *     or `bridge/*`: those load through lazy cells that can fire on an
     *     arbitrary later tap, and there is no way to ask the engine whether they
     *     already have.
     *
     *  Frees the steady state, NOT the peak. At boot the corpus cache exists as
     *  the gunzip output, as this node's copy, as the shim's per-read `slice`,
     *  and as the engine's parsed copy — roughly four times over, and the peak is
     *  what runs a phone out of memory. Reducing that means not making the copies
     *  in the first place, which is a separate piece of work. */
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
    get germanInstalled() {
      return germanOn;
    },
    async installGermanCorpus(cache: Uint8Array) {
      if (frozen) return;
      await serial(async () => {
        if (frozen) return;
        // Into the live home so this session could read it, and then ONLY THE
        // MARKER into IndexedDB.
        //
        // NOT the 28 MB itself, and that is the point. The bytes are already in
        // the DEPOT — that is where `fetchGermanCorpus` put them — and the depot
        // is what boot reads. Writing them to IndexedDB as well is precisely the
        // mistake `dropLegacyIdxcache` exists to undo for the English corpus: a
        // second full copy of a Bible on the device, rewritten on every launch.
        insertFile(root, GERMAN_CACHE, cache);
        await idbApply("cache", new Map([[GERMAN_INSTALLED, enc.encode("1")]]));
        germanOn = true;
      });
    },
    async installSuggestedWeaves(bundle: Uint8Array) {
      if (frozen) return 0;
      // `{ "<name>.json": "<file text>" }` — the text verbatim, never
      // re-serialized, so these are the maintainer's bytes (build-web-pack.mjs
      // stores them the same way and for the same reason).
      const parsed: unknown = JSON.parse(dec.decode(bundle));
      if (typeof parsed !== "object" || parsed === null) throw new Error("suggested-weave bundle is not an object");

      const fresh = new Map<string, Uint8Array>();
      for (const [name, text] of Object.entries(parsed as Record<string, unknown>)) {
        // A bundle is data off the network, so it may not reach outside the one
        // directory it is allowed to fill. Nothing here should ever trip, which
        // is exactly why it is checked rather than assumed.
        if (typeof text !== "string" || name.includes("/") || name.startsWith(".")) continue;
        const path = `weaves/suggested/${name}`;
        // THE READER'S COPY WINS, the same rule the seeded stock follows: if a
        // file is already here they have either had it before or written it,
        // and an install must not overwrite either.
        if (readFile(root, path)) continue;
        fresh.set(path, enc.encode(text));
      }

      return serial(async () => {
        if (frozen) return 0;
        for (const [path, bytes] of fresh) {
          insertFile(root, path, bytes);
          // Registered as pristine so the bundled-set OFF toggle can remove an
          // untouched suggestion later — without this they would be
          // indistinguishable from the reader's own work and never removable.
          pristineStock.set(path, bytes);
        }
        // The marker goes in the SAME transaction as the files. Two writes could
        // leave a home marked installed with nothing in it, and the Settings row
        // would then offer no way to try again.
        await idbApply("user", fresh);
        await idbApply("cache", new Map([[SUGGESTED_INSTALLED, enc.encode("1")]]));
        for (const [path, bytes] of fresh) synced.set(path, fingerprint(bytes));
        // Only now, with the bytes and the marker both down: this session's
        // answer to "do you have them?" changes when the store's does.
        suggestedOn = true;
        return fresh.size;
      });
    },
    async setBundled(on: boolean) {
      // A restore is pending reload — nothing may write (see `freeze`). The
      // restored user store is about to become the truth, and deleting stock
      // paths out of it would take files the archive just put back.
      if (frozen) return;
      await idbApply("cache", new Map([[BUNDLED, enc.encode(on ? "on" : "off")]]));
      if (on) {
        // Re-seed on next boot: missing stock files come back, kept edits win.
        await idbApply("cache", new Map(), [STOCK_SEEDED]);
        return;
      }
      // OFF REMOVES THE EXAMPLES, NOT THE READER'S WORK.
      //
      // This used to delete every stock PATH outright, so a stock thread or
      // weave the reader had renamed, re-noted or added verses to was destroyed
      // by a settings toggle that reads as "hide the examples". Their copy wins
      // on the way in (buildHome lays their saved files over freshly-seeded
      // stock, and skips the stock paths entirely once seeded); this is the same
      // invariant on the way out.
      //
      // THE TEST IS RAW BYTES, exactly, against the bundled file — never a
      // normalised or parsed form, and never a fingerprint:
      //
      //  * The seed writes the pack's bytes verbatim, so a stock file nobody has
      //    touched is byte-identical by construction. Merely OPENING one changes
      //    nothing: the core writes a thread/tag/weave only from an authoring
      //    call (`write_weave` and friends have no read-path callers), so
      //    "the reader looked at it" still classifies as pristine — correctly,
      //    since none of their work is in it.
      //  * Any authoring write re-serializes through the core, which differs from
      //    the shipped bytes in whitespace and key order even before the edit
      //    itself, so an edited file lands on the KEEP side twice over.
      //  * Comparing a NORMALISED form is the dangerous direction, and it is why
      //    this does not do it: it would call a re-saved file pristine even when
      //    the reader's change lives in a field the normaliser drops. Byte
      //    equality can only err toward keeping.
      //
      // BOTH COPIES MUST AGREE. The live tree can hold an edit that has not been
      // persisted yet, and IndexedDB can hold one ANOTHER TAB made after our
      // boot — either alone would call the other tab's rename pristine. So a
      // path is removable only when neither copy differs from the bundled bytes.
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
        // Keep this session's bookkeeping honest: gone from the tree AND from
        // `synced`, so the next persist neither resurrects them (a file whose
        // fingerprint still matched `synced` would not be rewritten, but one
        // still in `synced` and absent from the tree would be deleted a second
        // time) nor claims anything about them.
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
        // own in-memory home, snapshotted at ITS boot. This used to write the
        // whole subtree, which made every persist an assertion about files this
        // tab had never touched — the slower tab's stale copies overwrote the
        // faster tab's edits, and a file deleted over there was resurrected by
        // any write over here. So:
        //
        //  * write ONLY files whose bytes differ from what we last synced —
        //    a file we didn't change is a file we have no opinion about;
        //  * delete ONLY files WE removed (in `synced`, gone from our tree) —
        //    absence elsewhere is someone else's decision, not ours.
        //
        // Both tabs editing the SAME file stays last-writer-wins per file: the
        // engine can't reload another tab's state mid-session, and IndexedDB
        // already serialises readwrite transactions per store, so no cross-tab
        // lock is needed — the transaction below is atomic either way.
        //
        // A FLUSH CHANGES NONE OF THIS. It runs this same diff, one moment
        // earlier, so it can neither widen what this session claims to own nor
        // resurrect a deletion — the fingerprints decide that, not the caller.
        //
        // ONE DELETION THIS SESSION MAKES WITHOUT HAVING TOUCHED THE FILE: a
        // stranded temp. `collectFiles` no longer collects them, so one an older
        // build persisted — or one that rode in on a restored Android backup — is
        // in `synced`, absent from `current`, and swept here. That is the point:
        // it is a fragment nothing can read, and left alone it ships in every
        // backup from now on. The sweep reaches nothing else, because `deletes` is
        // exactly `synced` minus `current` and the only names the filter newly
        // withholds from `current` are the ones matching TEMP_NAME.
        const { puts, prints } = changedFiles(current, synced);
        const deletes = [...synced.keys()].filter((k) => !current.has(k));
        if (puts.size === 0 && deletes.length === 0) return;
        await idbApply("user", puts, deletes);
        // Only after the transaction commits — a failed persist must leave every
        // dirty file dirty, so the next write retries the whole backlog.
        for (const k of deletes) synced.delete(k);
        for (const [k, p] of prints) synced.set(k, p);
      });
    },
    persistUserDir(d: string) {
      return serial(async () => {
        if (frozen) return;
        // A guard, not politeness: an unknown directory here would write paths
        // `persistUserData` never collects, and its diff would then delete them
        // on the reader's next authoring write.
        if (!USER_DIRS.includes(d)) return;
        const current = new Map<string, Uint8Array>();
        const dir = root.get(d);
        if (dir instanceof Directory) collectFiles(d, dir, current);
        // Additive only. Deletions belong to the diffing path — a timer-driven
        // write has no business deciding something else is gone. Same dirty-only
        // rule as persistUserData: on the 30 s dwell tick that means one book's
        // file, not the whole reading dir.
        const { puts, prints } = changedFiles(current, synced);
        if (puts.size === 0) return;
        await idbApply("user", puts);
        for (const [k, p] of prints) synced.set(k, p);
      });
    },
  };
}
