// Minimal promise wrapper over IndexedDB — one database, two object stores:
// "user"  — the authored home files (tags/threads/weaves/notes/memory/config),
//           keyed by home-relative path, valued as bytes.
// "cache" — rebuildable artifacts (the corpus idxcache), same keying.
// No dependency: the app's storage needs are a flat key→bytes map.

const DB_NAME = "plumbline";
const DB_VERSION = 1;
export type StoreName = "user" | "cache";

let dbPromise: Promise<IDBDatabase> | null = null;

function db(): Promise<IDBDatabase> {
  dbPromise ??= new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      for (const name of ["user", "cache"] as const)
        if (!req.result.objectStoreNames.contains(name)) req.result.createObjectStore(name);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbPromise;
}

function done<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function idbGet(store: StoreName, key: string): Promise<Uint8Array | undefined> {
  const d = await db();
  const v = await done(d.transaction(store).objectStore(store).get(key));
  return v as Uint8Array | undefined;
}

export async function idbEntries(store: StoreName): Promise<Map<string, Uint8Array>> {
  const d = await db();
  const os = d.transaction(store).objectStore(store);
  const [keys, values] = await Promise.all([done(os.getAllKeys()), done(os.getAll())]);
  const out = new Map<string, Uint8Array>();
  keys.forEach((k, i) => out.set(String(k), values[i] as Uint8Array));
  return out;
}

/// Apply a batch of writes + deletes in one transaction.
export async function idbApply(
  store: StoreName,
  puts: Map<string, Uint8Array>,
  deletes: Iterable<string> = [],
): Promise<void> {
  const d = await db();
  const tx = d.transaction(store, "readwrite");
  const os = tx.objectStore(store);
  for (const [k, v] of puts) os.put(v, k);
  for (const k of deletes) os.delete(k);
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}
