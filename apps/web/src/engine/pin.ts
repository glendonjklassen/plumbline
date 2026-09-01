// The pack pin: what data pack this device is KNOWN to hold.
//
// Without it, boot must fetch `pack/manifest.json` before anything else — a
// network request on the critical path, on the one pack file with no version in
// its URL, so it cannot be cache-first. The pin removes that request, which is
// what makes a warm boot cost ZERO network requests before text: it IS a
// manifest, stored on the device, written only after every file it names was
// verified present.
//
// It lives in the depot beside the bytes it describes, not in IndexedDB (which
// holds what cannot be re-derived — the reader's files, and flags recording their
// decisions). Co-located, an eviction takes description and bytes together and the
// device cold-starts cleanly, leaving one case to handle:
//
//   THE PIN IS A CLAIM, NOT A PROOF. Browsers evict, so every read of a pinned
//   file is "try the depot, else fall back to the cold path" — and the cold path
//   IS the repair: it re-downloads only what is actually missing.

import { depotGet, depotPut } from "./depot";
import { assetUrl, type PackFile, type PackManifest } from "./pack";

/** This build. Read once so a pin and a staleness check cannot disagree. */
const BUILD_ID = typeof __BUILD_ID__ === "string" ? __BUILD_ID__ : "dev";

/** Whether this pin was written by an older build than the one running — the pin's
 *  file list may then be missing something the code now expects, and it is the only
 *  condition under which a warm boot re-asks for the manifest. Unknown (a pin from
 *  before the field) counts as stale, once. */
export function pinIsFromAnOlderBuild(pin: Pin | null): boolean {
  return pin !== null && pin.buildId !== BUILD_ID;
}

const PIN_URL = "__depot/pack-pin.json";
const PREV_URL = "__depot/pack-pin.prev.json";
const FORMAT = "pack-pin-v1";

export interface Pin {
  format: string;
  /** The app base this pin was written against, ABSOLUTE. The Cache API is
   *  origin-partitioned, so a pin from a different origin or subpath describes
   *  storage we cannot see — a base mismatch reads as "no pin". */
  base: string;
  packVersion: string;
  /** The build that wrote this pin (`__BUILD_ID__`, one per `vite build`) — a
   *  build id rather than the app version, because a data-only deploy still
   *  rebuilds. Absent on pins written before the field, which reads as stale. */
  buildId?: string;
  /** Every file the PACK OFFERS, each carrying the explicit URL its bytes are
   *  stored under — explicit rather than computed, so two pack generations can
   *  coexist in the depot and an unchanged file keeps its URL across a version bump.
   *
   *  A file this device does not have (only `optional` files) is listed WITHOUT a
   *  url: the entry stays, because a warm boot rebuilds the manifest from this list
   *  and Settings could not otherwise offer the download; the url goes, because the
   *  pin's promise is that every url it names is present and prune keeps exactly
   *  those. Both readers skip an entry with no url. */
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
 *  hash, so a release that changes one weave invalidates one URL instead of all of
 *  them. The hash goes in the QUERY, not the filename, so the layout under
 *  `public/pack/` is untouched and the server needs no old-generation retention.
 *
 *  Module-private: the pin is the only thing that mints these, and every reader of
 *  a pack file goes through `packFileUrl` in pack.ts, which prefers the recorded
 *  URL — a second way to derive it is a second way to disagree. */
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

// No "does the depot have this stage?" probe here, on purpose: boot asks
// `fetchStageLocal` for the stage's BYTES and treats a miss as the cold path. A
// probe costs a storage round trip per file on the path we are making fast, and —
// the pin being a claim, not a proof — can disagree with the read it describes.

/** Commit a pin, keeping the one it replaces.
 *
 *  Order matters: `prev` is written FIRST, so a torn `pin.json` degrades to the
 *  previous generation, whose bytes are all still present because prune keeps two
 *  generations and runs at the START of a session. No lock needed: a `Cache.put`
 *  either lands whole or does not land. */
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

/** A manifest equivalent to what the network would have returned, from the pin, so
 *  every stage filter in pack.ts works off it unchanged — the pin gets no loading
 *  path of its own, which is what keeps the two from drifting. */
export function manifestFromPin(pin: Pin): PackManifest {
  return { formatVersion: 2, version: pin.packVersion, files: pin.files };
}
