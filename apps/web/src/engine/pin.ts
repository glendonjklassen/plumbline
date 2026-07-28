// The pack pin: what data pack this device is KNOWN to hold.
//
// Boot used to fetch `pack/manifest.json` before it could do anything else — a
// network request on the critical path, on the one file with no version in its
// URL, so it could not be cache-first. On a stalled radio that cost up to the
// service worker's 3.5 s timebox before a device holding every byte of scripture
// would open. The pin removes the request: it IS a manifest, stored on the
// device, written only after every file it names was verified present.
//
// WHERE IT LIVES, and the rule behind it. The pin sits in the depot (the Cache
// API), beside the bytes it describes — not in IndexedDB.
//
//   IndexedDB holds what CANNOT be re-derived: the reader's authored files, and
//   the small flags that record a reader's DECISION (stockSeeded, bundled).
//   The depot holds what can be re-downloaded. A description of a store shares
//   that store's fate.
//
// The argument is failure-correlation rather than tidiness. Split across two
// independently-evictable stores, the divergences double — pin-without-bytes AND
// bytes-without-pin, each needing its own recovery. Co-located, an eviction takes
// both and the device cold-starts cleanly, leaving exactly one case to handle:
// some bytes evicted under a surviving pin. Which is handled, because:
//
//   THE PIN IS A CLAIM, NOT A PROOF. Browsers evict under storage pressure. Every
//   read of a pinned file is written as "try the depot, else fall back", never as
//   "the pin said so, therefore it is there". Boot degrades to the cold path,
//   which re-downloads only what is actually missing.

import { depotGet, depotHas, depotPut } from "./depot";
import { assetUrl, type PackFile, type PackManifest } from "./pack";

const PIN_URL = "__depot/pack-pin.json";
const PREV_URL = "__depot/pack-pin.prev.json";
const FORMAT = "pack-pin-v1";

export interface Pin {
  format: string;
  /** The app base this pin was written against, ABSOLUTE. The Cache API is
   *  origin-partitioned and our URLs are built from a resolved base, so a pin
   *  from a different origin or subpath describes storage we cannot see. The
   *  domain has already moved once (plumblinebible.org, 2026-07-25), and treating
   *  a base mismatch as "no pin" is correct rather than defensive: it IS a
   *  different origin's storage. */
  base: string;
  packVersion: string;
  /** Every file, each carrying the explicit URL its bytes are stored under.
   *  Explicit rather than computed, so two pack generations can coexist in the
   *  depot and an unchanged file keeps its URL across a version bump. */
  files: PackFile[];
}

function pinFrom(manifest: PackManifest, base: string): Pin {
  return {
    format: FORMAT,
    base,
    packVersion: manifest.version,
    files: manifest.files.map((f) => ({ ...f, url: fileUrl(f, manifest.version) })),
  };
}

/** The URL a pack file's bytes live under: content-addressed on the file's own
 *  hash, so a release that changes one weave invalidates one URL instead of all
 *  forty-four. The hash goes in the QUERY, not the filename, so the layout under
 *  `public/pack/` is untouched and the server needs no old-generation retention. */
export function fileUrl(f: PackFile, packVersion: string): string {
  return f.hash ? `pack/${f.path}.gz?h=${f.hash}` : `pack/${f.path}.gz?v=${packVersion}`;
}

/** The pin, or null. Never throws — an unreadable pin is simply no pin. */
export async function readPin(base: string): Promise<Pin | null> {
  for (const url of [PIN_URL, PREV_URL]) {
    try {
      const hit = await depotGet(assetUrl(url));
      if (!hit) continue;
      const pin = (await hit.json()) as Pin;
      if (pin?.format !== FORMAT || !Array.isArray(pin.files) || !pin.files.length) continue;
      if (pin.base !== base) continue; // a different origin's storage
      return pin;
    } catch {
      /* torn or unparseable — try the previous generation */
    }
  }
  return null;
}

/** Whether every file this stage needs is actually in the depot. The pin claims
 *  it; this checks it. Cheap — `match` is a metadata lookup, no body is read. */
export async function pinHasStage(pin: Pin, stage: PackFile["stage"]): Promise<boolean> {
  for (const f of pin.files) {
    if (f.stage !== stage) continue;
    if (!(await depotHas(assetUrl(f.url!)))) return false;
  }
  return true;
}

/** Commit a pin, keeping the one it replaces.
 *
 *  Order matters: `prev` is written FIRST, so a torn or half-written `pin.json`
 *  degrades to the previous generation — whose bytes are all still present,
 *  because prune keeps two generations and runs at the START of a session rather
 *  than the end. That is the whole atomicity story, and it needs no lock: a
 *  `Cache.put` either lands whole or does not land. */
export async function writePin(manifest: PackManifest, base: string): Promise<void> {
  const next = pinFrom(manifest, base);
  const current = await depotGet(assetUrl(PIN_URL));
  if (current) {
    const bytes = new Uint8Array(await current.arrayBuffer());
    await depotPut(assetUrl(PREV_URL), bytes, "application/json");
  }
  const body = new TextEncoder().encode(JSON.stringify(next));
  await depotPut(assetUrl(PIN_URL), body, "application/json");
}

/** Every URL the current and previous generations reference, plus the pin slots
 *  themselves. The keep-set for prune. */
export async function pinnedUrls(base: string): Promise<Set<string>> {
  const keep = new Set<string>([assetUrl(PIN_URL), assetUrl(PREV_URL)]);
  for (const url of [PIN_URL, PREV_URL]) {
    try {
      const hit = await depotGet(assetUrl(url));
      if (!hit) continue;
      const pin = (await hit.json()) as Pin;
      if (pin?.base !== base) continue;
      for (const f of pin.files) if (f.url) keep.add(assetUrl(f.url));
    } catch {
      /* unreadable: contributes nothing, and prune is skipped if BOTH are */
    }
  }
  return keep;
}

/** A manifest equivalent to what the network would have returned, from the pin.
 *  Every stage filter in pack.ts works off this unchanged — the pin does not get
 *  its own loading path, which is what keeps the two from drifting. */
export function manifestFromPin(pin: Pin): PackManifest {
  return { formatVersion: 2, version: pin.packVersion, files: pin.files };
}
