// The depot: the ONE place this app touches the Cache API.
//
// Everything the app can re-download lives here — the data pack, the wasm
// engine, the shell assets. The reader's own authored files live in IndexedDB
// (see home.ts) and never here; the rule is "bytes that can be re-derived go
// in the depot, bytes that cannot go in IndexedDB".
//
// WHY APP CODE STORES ITS OWN DOWNLOADS RATHER THAN LEAVING IT TO THE SERVICE
// WORKER. On a first visit the SW is not controlling the page while the shell
// loads, and it claims the clients somewhere in the middle of boot — so whether
// the ~12 MB pack passed through its fetch handler came down to a race with
// clients.claim() (measured 2026-07-26: the wasm landed in the cache, the pack
// did not, and the app could not boot offline afterwards). A dedicated worker
// inherits its creator's controller at creation, so the engine worker spawned
// during an uncontrolled first load is itself uncontrolled. Downloading code
// already holds the bytes; storing them here is deterministic.
//
// INVARIANT, and the reason this module exists as a chokepoint: nothing the
// engine worker needs may depend on being SW-controlled. Every pack and wasm
// read goes through `depotBytes` / `depotResponse`. A bare `fetch()` for one of
// those is a bug — it will work on your machine and fail offline on a phone.
//
// TWO TRAPS THIS MODULE EXISTS TO KEEP CLOSED:
//
//  1. `ignoreVary: true` on EVERY lookup. Our responses come back
//     `Vary: Origin`, and Vite's `<script crossorigin>` requests carry an
//     Origin header that a plain fetch does not — honouring Vary made a cached
//     entry invisible to the very request it was stored for, and the app failed
//     to boot offline with every byte already on disk (2026-07-26). Baked in
//     here so no call site can forget it.
//  2. We store a Response we CONSTRUCT, never the network's. A constructed
//     Response carries no Vary header at all, so trap 1 cannot even arise for
//     anything we wrote; and it lets us set `content-type` ourselves, which
//     `WebAssembly.compileStreaming` is picky about.
//
// Best-effort throughout: storage can be blocked (private mode, plain http) or
// full. A reader who cannot cache should still be able to read, so every
// failure here degrades to "works, but is not offline yet" rather than throwing.
//
// RECLAMATION IS NOT HERE. This module hands out primitives (`depotKeys`,
// `depotDelete`); the sweep itself is `pruneToPin` in engine.worker.ts, because
// the pin is the authority on what to keep and the pin lives in the worker.
// There used to be a second sweeper here — `pruneStale`, a DENYLIST keyed on
// `?v=` — and it went unreferenced the day the pin landed (2026-07-28) while
// still reading as live code. One sweeper, in the module that holds the
// keep-set: a second one either disagrees with the first or is dead.

/** The single Cache bucket. Must match the name in public/sw.js — a plain
 *  script served from /, which cannot import this module. Change both together.
 *
 *  Deliberately ONE bucket shared with the shell: sw.js's `activate` deletes
 *  every bucket it does not recognise, so a second name is a bucket an older
 *  service worker can wipe. */
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
    // A fresh ArrayBuffer view: `bytes` may be a subarray of a larger buffer,
    // and Response would otherwise store the whole thing.
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
 *  on the way past. `onChunk` reports bytes as they arrive so a caller can drive
 *  a progress bar; a depot hit reports its whole length at once, because a local
 *  read is not a download and pretending otherwise makes the bar crawl for no
 *  reason.
 *
 *  Returns the bytes EXACTLY as stored/served — still gzipped for pack files.
 *  Decompression is the caller's business (see pack.ts, which has to sniff for
 *  hosts that transparently decode `.gz`).
 *
 *  Throws only when the bytes cannot be obtained at all — a depot miss while
 *  offline. That is a real failure the caller must handle. */
export async function depotBytes(
  url: string,
  onChunk?: (bytes: number) => void,
  contentType = "application/octet-stream",
  /** Filled in with which side of the read-through answered. Diagnostics only.
   *
   *  Reported from the read that ACTUALLY HAPPENED rather than from a separate
   *  `depotHas` probe beforehand: a probe costs a storage round trip per file on
   *  the very path we are trying to make fast (44 of them on a phone), and it can
   *  disagree with the read it is supposed to describe. */
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

/** Read-through for the wasm engine, which needs a Response rather than bytes
 *  so `WebAssembly.compileStreaming` can compile while it downloads.
 *
 *  Stored with `content-type: application/wasm` explicitly: compileStreaming
 *  REJECTS a response with any other type, and left to the host that depends on
 *  a correct MIME table on whatever is serving the file. */
export async function depotResponse(url: string): Promise<Response> {
  const hit = await depotGet(url);
  if (hit) return hit;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  // Buffer it: we need the bytes to store a constructed Response, and a
  // Response body can only be read once. The wasm is 1.6 MB — buffering it
  // costs a copy, and it buys a correct content-type offline.
  const bytes = new Uint8Array(await res.arrayBuffer());
  await depotPut(url, bytes, "application/wasm");
  return (await depotGet(url)) ?? new Response(bytes.slice().buffer as ArrayBuffer, {
    headers: { "content-type": "application/wasm" },
  });
}

/** Ask the browser not to evict us under storage pressure.
 *
 *  Worth asking because the whole offline promise rests on ~14 MB surviving,
 *  and eviction is the one failure the app cannot detect until a reader is
 *  already offline and short. Chrome auto-grants on engagement or install;
 *  Safari grants only for home-screen apps; others prompt or decline.
 *
 *  NOTHING may assume this succeeded — it is a mitigation, not a guarantee, and
 *  the boot path still has to survive missing bytes. Returns the granted state
 *  so Settings can tell the reader the truth about their device. */
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
