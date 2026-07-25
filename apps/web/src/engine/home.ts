// The virtual home directory the engine sees through WASI: the read-only data
// pack (data/, bridge/) plus the user's authored files (tags/, threads/,
// weaves/, notes/, memory/, .config/) restored from IndexedDB. After every
// authoring write the user subtree is diffed back to IndexedDB — the browser
// twin of the desktop shells' "engine reloads from disk after any write".

import { Directory, File } from "@bjorn3/browser_wasi_shim";
import { idbApply, idbEntries, idbGet } from "./idb";

/** Home-relative directories that hold user-authored state. */
const USER_DIRS = ["tags", "threads", "weaves", "notes", "memory", ".config"];
/** The engine-built corpus cache — rebuildable, persisted to skip re-parse. */
const IDXCACHE = "data/kjv.jsonl.idxcache";

export interface VirtualHome {
  /** Root contents map handed to PreopenDirectory("/home", …). */
  root: Map<string, Directory | File>;
  /** Diff the user subtree against IndexedDB (call after authoring writes). */
  persistUserData(): Promise<void>;
  /** Persist the engine-built idxcache once, after a successful open. */
  persistIdxcache(): Promise<void>;
}

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

export async function buildHome(pack: Map<string, Uint8Array>): Promise<VirtualHome> {
  const root = new Map<string, Directory | File>();
  for (const [path, bytes] of pack) insertFile(root, path, bytes);

  // Authoring dirs must exist even when empty (the engine lists them), and
  // weaves/suggested is part of the expected shape.
  for (const d of USER_DIRS) ensureDir(root, d);
  ensureDir(root, "weaves/suggested");

  // Restore the user's files and the corpus cache from previous sessions.
  const [userFiles, idxcache] = await Promise.all([idbEntries("user"), idbGet("cache", IDXCACHE)]);
  for (const [path, bytes] of userFiles) insertFile(root, path, bytes);
  if (idxcache) insertFile(root, IDXCACHE, idxcache);

  // Snapshot of what IndexedDB currently holds, for cheap diffs on persist.
  let synced = new Set(userFiles.keys());

  return {
    root,
    async persistUserData() {
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
      if (cache instanceof File) await idbApply("cache", new Map([[IDXCACHE, (cache as File).data]]));
    },
  };
}
