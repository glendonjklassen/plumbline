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

import { depotGet, depotPut } from "./depot";
import { assetUrl, type PackFile, type PackManifest } from "./pack";

/** This build. Read once so a pin and a staleness check cannot disagree. */
const BUILD_ID = typeof __BUILD_ID__ === "string" ? __BUILD_ID__ : "dev";

/** Whether this pin was written by an OLDER build than the one running — the
 *  condition under which the pin's file list may be missing something the code
 *  now expects, and the only condition under which a warm boot re-asks for the
 *  manifest. Unknown (a pin from before the field) counts as stale, once. */
export function pinIsFromAnOlderBuild(pin: Pin | null): boolean {
  return pin !== null && pin.buildId !== BUILD_ID;
}

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
  /** The BUILD that wrote this pin (`__BUILD_ID__`, one per `vite build`).
   *
   *  What it answers is "is my code newer than my pack description?" — the pin
   *  IS the manifest on a warm boot, so a pin from an older build can be missing
   *  a file the running code expects. v0.39.0 shipped that: every upgrading
   *  reader's hymn tab was empty, because their pin predated hymnal.json.
   *
   *  A build id rather than the app version, because a deploy that changes only
   *  data still rebuilds, and the app version would not move. Absent on a pin
   *  written before this field existed, which reads as "unknown, assume stale"
   *  — one manifest fetch on the first boot after upgrading, then never again. */
  buildId?: string;
  /** Every file the PACK OFFERS, each carrying the explicit URL its bytes are
   *  stored under. Explicit rather than computed, so two pack generations can
   *  coexist in the depot and an unchanged file keeps its URL across a version
   *  bump.
   *
   *  A file this device does not have is listed WITHOUT a url. Only `optional`
   *  files are ever in that state, and the distinction carries the pin's two
   *  jobs at once: the entry has to stay, because a warm boot rebuilds the
   *  manifest from this list and Settings could not offer a download it cannot
   *  see; and the url has to go, because the pin's promise is that every file
   *  it names is PRESENT, and prune keeps exactly the urls it names. Both
   *  readers already skip an entry with no url. */
  files: PackFile[];
}

function pinFrom(manifest: PackManifest, base: string, here: PackFile[]): Pin {
  const have = new Set(here.map((f) => f.path));
  return {
    format: FORMAT,
    base,
    packVersion: manifest.version,
    buildId: BUILD_ID,
    files: manifest.files.map((f) =>
      have.has(f.path) ? { ...f, url: fileUrl(f, manifest.version) } : { ...f, url: undefined },
    ),
  };
}

/** The URL a pack file's bytes live under: content-addressed on the file's own
 *  hash, so a release that changes one weave invalidates one URL instead of all
 *  forty-four. The hash goes in the QUERY, not the filename, so the layout under
 *  `public/pack/` is untouched and the server needs no old-generation retention.
 *
 *  Module-private: the pin is the ONLY thing that mints these, and every reader
 *  of a pack file goes through `packFileUrl` in pack.ts, which prefers the URL
 *  the pin already recorded. Exporting this offered a second way to derive what
 *  is supposed to be recorded once. */
function fileUrl(f: PackFile, packVersion: string): string {
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

// THERE IS NO "does the depot have this stage?" PROBE HERE, on purpose. One
// shipped with the pin and was never called: boot instead asks `fetchStageLocal`
// for the stage's BYTES and treats a miss as the cold path. A probe would cost a
// storage round trip per file on the one path we are making fast (44 files on a
// phone), and — because the pin is a claim, not a proof — it can disagree with
// the read it is supposed to describe. The read that actually happens is the
// only honest answer. (Same reasoning as `depotBytes`'s `source` out-param.)

/** Commit a pin, keeping the one it replaces.
 *
 *  Order matters: `prev` is written FIRST, so a torn or half-written `pin.json`
 *  degrades to the previous generation — whose bytes are all still present,
 *  because prune keeps two generations and runs at the START of a session rather
 *  than the end. That is the whole atomicity story, and it needs no lock: a
 *  `Cache.put` either lands whole or does not land. */
export async function writePin(manifest: PackManifest, base: string, here: PackFile[]): Promise<void> {
  const next = pinFrom(manifest, base, here);
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
