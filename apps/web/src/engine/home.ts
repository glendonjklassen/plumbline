// The virtual home directory the engine sees through WASI: the read-only data
// pack (data/, bridge/) plus the user's authored files (tags/, threads/,
// weaves/, notes/, memory/, .config/) restored from IndexedDB. After every
// authoring write the user subtree is diffed back to IndexedDB — the browser
// twin of the desktop shells' "engine reloads from disk after any write".

import { Directory, File } from "@bjorn3/browser_wasi_shim";
import { idbApply, idbEntries, idbGet } from "./idb";

/** Home-relative directories that hold user-authored state. */
const USER_DIRS = ["tags", "threads", "weaves", "notes", "memory", ".config"];
/** The corpus idxcache — rebuildable, persisted to skip the 19 MB re-parse.
 *  Sources, in preference order: this device's persisted copy (IndexedDB),
 *  else the pack-shipped web-stamped one (fetched on first visit). */
const IDXCACHE = "data/kjv.jsonl.idxcache";

/** Which pack version the persisted idxcache was built from. A cache outlives
 *  the data update that invalidates it otherwise: its verses would be the old
 *  text, and the tokenization stamp (unchanged across data updates) wouldn't
 *  notice. */
const IDXCACHE_VERSION = "meta:idxcacheVersion";

/** This device's persisted idxcache and the pack version it came from — boot
 *  checks it BEFORE the stage-1 fetch to decide whether to download the text
 *  at all. */
export async function loadPersistedIdxcache(): Promise<{ bytes: Uint8Array; version: string } | undefined> {
  const [bytes, version] = await Promise.all([idbGet("cache", IDXCACHE), idbGet("cache", IDXCACHE_VERSION)]);
  return bytes && version ? { bytes, version: dec.decode(version) } : undefined;
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
  /** Diff the user subtree against IndexedDB (call after authoring writes). */
  persistUserData(): Promise<void>;
  /** Persist the engine-built idxcache once, after a successful open. */
  persistIdxcache(): Promise<void>;
  /** Snapshot of the authored files (for the backup zip). */
  exportUserData(): Map<string, Uint8Array>;
  /** Stop ALL persistence (a restore is pending reload — nothing may write). */
  freeze(): void;
  /** Whether the bundled stock study set is enabled. */
  bundledOn: boolean;
  /** Flip the bundled set (removes/reseeds the stock files); reload after. */
  setBundled(on: boolean): Promise<void>;
}

const STOCK_SEEDED = "meta:stockSeeded";
const BUNDLED = "meta:bundled";
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

/** Walk a user directory, collecting home-relative path → bytes. */
function collectFiles(prefix: string, dir: Directory, out: Map<string, Uint8Array>): void {
  for (const [name, node] of dir.contents as Map<string, Directory | File>) {
    const path = `${prefix}/${name}`;
    if (node instanceof Directory) collectFiles(path, node, out);
    else if (node instanceof File) out.set(path, (node as File).data);
  }
}

export async function buildHome(
  pack: Map<string, Uint8Array>,
  stockPaths: Set<string> = new Set(),
  idxcache?: Uint8Array,
  packVersion = "",
): Promise<VirtualHome> {
  const root = new Map<string, Directory | File>();
  const [userFiles, seededFlag, bundledFlag] = await Promise.all([
    idbEntries("user"),
    idbGet("cache", STOCK_SEEDED),
    idbGet("cache", BUNDLED),
  ]);
  const bundledOn = bundledFlag ? dec.decode(bundledFlag) !== "off" : true;
  // The stock set seeds ONCE (Android parity): after that the user's own
  // copies rule, so edits and deletions stick across pack updates.
  const seedStock = bundledOn && !seededFlag;

  for (const [path, bytes] of pack) {
    if (stockPaths.has(path) && !seedStock) continue;
    insertFile(root, path, bytes);
  }

  // Authoring dirs must exist even when empty (the engine lists them), and
  // weaves/suggested is part of the expected shape.
  for (const d of USER_DIRS) ensureDir(root, d);
  ensureDir(root, "weaves/suggested");

  // Restore the user's files and the corpus cache from previous sessions
  // (user copies overwrite freshly-seeded stock — theirs is newer; the
  // persisted idxcache overwrites the pack-shipped copy the same way).
  for (const [path, bytes] of userFiles) insertFile(root, path, bytes);
  if (idxcache) insertFile(root, IDXCACHE, idxcache);

  // Snapshot of what IndexedDB currently holds, for cheap diffs on persist.
  let synced = new Set(userFiles.keys());
  let frozen = false;

  if (seedStock) {
    // Persist the seeded stock as the user's own files + set the marker.
    const seeded = new Map<string, Uint8Array>();
    for (const d of USER_DIRS) {
      const dir = root.get(d);
      if (dir instanceof Directory) collectFiles(d, dir, seeded);
    }
    await idbApply("user", seeded);
    await idbApply("cache", new Map([[STOCK_SEEDED, enc.encode("1")]]));
    synced = new Set(seeded.keys());
  }

  return {
    root,
    hadIdxcache: !!idxcache || pack.has(IDXCACHE),
    addFiles(files: Map<string, Uint8Array>) {
      for (const [path, bytes] of files) insertFile(root, path, bytes);
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
    async setBundled(on: boolean) {
      await idbApply("cache", new Map([[BUNDLED, enc.encode(on ? "on" : "off")]]));
      if (on) {
        // Re-seed on next boot: missing stock files come back, kept edits win.
        await idbApply("cache", new Map(), [STOCK_SEEDED]);
      } else {
        // Remove just the stock items by their bundled paths; anything the
        // reader authored under other names stays (Android parity).
        await idbApply("user", new Map(), [...stockPaths]);
      }
    },
    freeze() {
      frozen = true;
    },
    async persistUserData() {
      if (frozen) return;
      const current = new Map<string, Uint8Array>();
      for (const d of USER_DIRS) {
        const dir = root.get(d);
        if (dir instanceof Directory) collectFiles(d, dir, current);
      }
      const deletes = [...synced].filter((k) => !current.has(k));
      // Files are small (per-verse/per-tag JSON); rewriting the subtree on an
      // authoring event is cheaper than tracking per-file dirty bits.
      await idbApply("user", current, deletes);
      synced = new Set(current.keys());
    },
    async persistIdxcache() {
      const dataDir = root.get("data");
      if (!(dataDir instanceof Directory)) return;
      const cache = (dataDir.contents as Map<string, Directory | File>).get("kjv.jsonl.idxcache");
      if (!(cache instanceof File)) return;
      // Stamped with the pack it came from, so the next launch can tell
      // whether it still describes the shipped text.
      await idbApply(
        "cache",
        new Map([
          [IDXCACHE, (cache as File).data],
          [IDXCACHE_VERSION, enc.encode(packVersion)],
        ]),
      );
    },
  };
}
