// The depot: the ONLY module that touches the Cache API. Everything the app can
// re-download lives here (data pack, wasm engine, shell assets); the reader's own
// authored files live in IndexedDB (home.ts).
//
// Invariant: nothing the engine worker needs may depend on being service-worker
// controlled. On a first visit the SW is not controlling the page while the shell
// loads and claims clients mid-boot, so a bare `fetch()` for a pack or wasm file
// works on a desktop and fails offline on a phone. Every such read goes through
// `depotBytes` / `depotResponse`.
//
// Two rules baked in here so no call site can forget them:
//  1. `ignoreVary: true` on EVERY lookup. Our responses come back `Vary: Origin`
//     and Vite's `<script crossorigin>` requests carry an Origin a plain fetch
//     does not, so honouring Vary hides an entry from the request it was stored
//     for — and the app fails to boot offline with every byte already on disk.
//  2. Store a Response we CONSTRUCT, never the network's: it carries no Vary at
//     all, and lets us set the content-type `WebAssembly.compileStreaming` wants.
//
// Best-effort throughout: storage can be blocked (private mode, plain http) or
// full, so failures degrade to "works, but is not offline yet" rather than
// throwing. Reclamation is not here — `pruneToPin` in engine.worker.ts owns it,
// because the pin is the keep-set and the pin lives in the worker.

/** The single Cache bucket. Must match the name in public/sw.js — a plain script
 *  served from /, which cannot import this module. One bucket shared with the
 *  shell, because sw.js's `activate` deletes every bucket it does not recognise. */
export const DEPOT = "plumbline-v1";

const MATCH: CacheQueryOptions = { ignoreVary: true };

/** Whether the Cache API exists at all (absent in private mode / plain http). */
export function depotAvailable(): boolean {
  return typeof caches !== "undefined";
}

async function bucket(): Promise<Cache | null> {
  if (!depotAvailable()) return null;
  try {
    return await caches.open(DEPOT);
  } catch {
    return null; // storage blocked
  }
}

/** A stored response, or undefined. Never throws. */
export async function depotGet(url: string): Promise<Response | undefined> {
  const c = await bucket();
  if (!c) return undefined;
  try {
    return await c.match(url, MATCH);
  } catch {
    return undefined;
  }
}

/** Is this URL on the device? A metadata lookup — no body is read. */
export async function depotHas(url: string): Promise<boolean> {
  return (await depotGet(url)) !== undefined;
}

/** Store bytes under `url`. Constructed Response, so no Vary and an explicit
 *  content type. Best-effort: quota and blocked storage are swallowed. */
export async function depotPut(url: string, bytes: Uint8Array, contentType: string): Promise<boolean> {
  const c = await bucket();
  if (!c) return false;
  try {
    // A fresh buffer: `bytes` may be a subarray of a larger one, and Response
    // would otherwise store the whole thing.
    const body = bytes.slice().buffer as ArrayBuffer;
    await c.put(
      url,
      new Response(body, {
        headers: { "content-type": contentType, "content-length": String(bytes.length) },
      }),
    );
    return true;
  } catch {
    return false; // private mode / quota: the app works, it just isn't offline
  }
}

export async function depotDelete(url: string): Promise<boolean> {
  const c = await bucket();
  if (!c) return false;
  try {
    return await c.delete(url, MATCH);
  } catch {
    return false;
  }
}

/** Every URL the depot holds. */
export async function depotKeys(): Promise<string[]> {
  const c = await bucket();
  if (!c) return [];
  try {
    return (await c.keys()).map((r) => r.url);
  } catch {
    return [];
  }
}

/** Read-through: the depot's copy if it has one, else the network — storing it
 *  on the way past. `onChunk` reports bytes as they arrive; a depot hit reports
 *  its whole length at once.
 *
 *  Returns the bytes EXACTLY as stored/served — still gzipped for pack files;
 *  decompression is the caller's business (see pack.ts).
 *
 *  Throws only when the bytes cannot be obtained at all — a depot miss while
 *  offline. */
export async function depotBytes(
  url: string,
  onChunk?: (bytes: number) => void,
  contentType = "application/octet-stream",
  /** Which side of the read-through answered. Diagnostics only, taken from the
   *  read that actually happened — a separate `depotHas` probe would cost a
   *  storage round trip per file on the path we are making fast, and can
   *  disagree with the read it describes. */
  source?: { fromDepot: boolean },
): Promise<Uint8Array> {
  const hit = await depotGet(url);
  if (hit) {
    if (source) source.fromDepot = true;
    const bytes = new Uint8Array(await hit.arrayBuffer());
    onChunk?.(bytes.length);
    return bytes;
  }
  if (source) source.fromDepot = false;

  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);

  const reader = res.body?.getReader();
  let bytes: Uint8Array;
  if (!reader) {
    // No streaming body (very old engines): whole-file, no incremental progress.
    bytes = new Uint8Array(await res.arrayBuffer());
    onChunk?.(bytes.length);
  } else {
    const chunks: Uint8Array[] = [];
    let size = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      size += value.length;
      onChunk?.(value.length);
    }
    bytes = new Uint8Array(size);
    let at = 0;
    for (const ch of chunks) {
      bytes.set(ch, at);
      at += ch.length;
    }
  }

  void depotPut(url, bytes, contentType);
  return bytes;
}

/** Read-through for the wasm engine, which needs a Response rather than bytes so
 *  `WebAssembly.compileStreaming` can compile while it downloads. Stored with an
 *  explicit `content-type: application/wasm`: compileStreaming rejects a response
 *  with any other type, and a host's MIME table cannot be relied on. */
export async function depotResponse(url: string): Promise<Response> {
  const hit = await depotGet(url);
  if (hit) return hit;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  // Buffer it: a body can only be read once, and we need the bytes to store a
  // constructed Response — one copy of 1.6 MB for a correct content-type offline.
  const bytes = new Uint8Array(await res.arrayBuffer());
  await depotPut(url, bytes, "application/wasm");
  return (await depotGet(url)) ?? new Response(bytes.slice().buffer as ArrayBuffer, {
    headers: { "content-type": "application/wasm" },
  });
}

/** Ask the browser not to evict us under storage pressure — the offline promise
 *  rests on ~11 MB surviving. A mitigation, not a guarantee: nothing may assume
 *  it succeeded, and the boot path still has to survive missing bytes. Returns
 *  the granted state, which Settings shows the reader. */
export async function requestPersistence(): Promise<boolean> {
  try {
    const s = navigator.storage;
    if (!s?.persist) return false;
    if (await s.persisted?.()) return true;
    return await s.persist();
  } catch {
    return false;
  }
}
