// Fetch the read-only data pack (built by scripts/build-web-pack.mjs):
// manifest first, then the gzipped files, decompressed with the browser's
// DecompressionStream.
//
// Every read goes through the DEPOT (engine/depot.ts), which serves the
// device's copy when it has one and stores what it downloads. That is not an
// optimisation — it is what makes the pack independent of whether the service
// worker happens to be controlling this worker, which on a first visit is a
// race (see the depot's header).

import { depotBytes, depotGet, depotPut } from "./depot";

export interface PackManifest {
  version: string;
  files: {
    path: string;
    bytes: number;
    gzBytes: number;
    stock?: boolean;
    rnd?: boolean;
    /** The pack-shipped corpus idxcache — fetched only when IndexedDB
     *  doesn't already hold a persisted one (see boot.ts). */
    cache?: boolean;
  }[];
}

export interface PackProgress {
  /** 0..1 across the whole pack download, weighted by gzipped size. */
  fraction: number;
  currentFile: string;
}

// The app's asset base as an ABSOLUTE url. Vite's BASE_URL is "./" (host-
// agnostic), which resolves against the *current script* — wrong inside the
// engine worker (it lives under assets/). The main thread passes the resolved
// page base into the worker, which overrides it here before booting.
let assetBase = typeof document !== "undefined" ? new URL(import.meta.env.BASE_URL, location.href).href : "";
export function setAssetBase(url: string): void {
  assetBase = url;
}
export function packUrl(path: string): string {
  return new URL(`pack/${path}`, assetBase).href;
}
export function assetUrl(path: string): string {
  return new URL(path, assetBase).href;
}

export async function fetchManifest(): Promise<PackManifest> {
  const url = new URL("pack/manifest.json", assetBase).href;
  // Network first, depot as the fallback: the manifest is the one pack file
  // with no version in its URL, so a cached copy can be stale and we want the
  // live one when there is a network. Offline, the stored copy is what lets the
  // rest of the pack be found at all.
  try {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const bytes = new Uint8Array(await res.arrayBuffer());
    void depotPut(url, bytes, "application/json");
    return JSON.parse(new TextDecoder().decode(bytes));
  } catch (e) {
    const hit = await depotGet(url);
    if (!hit) throw new Error(`data pack manifest: ${e instanceof Error ? e.message : String(e)}`);
    return hit.json();
  }
}

async function gunzip(body: ArrayBuffer): Promise<Uint8Array> {
  const bytes = new Uint8Array(body);
  // Some servers see the .gz extension and serve it with Content-Encoding:
  // gzip — the browser then hands us already-decompressed bytes. Sniff the
  // gzip magic instead of trusting headers, so any host behaviour works.
  if (bytes.length < 2 || bytes[0] !== 0x1f || bytes[1] !== 0x8b) return bytes;
  const stream = new Blob([body]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function fetchFiles(
  version: string,
  files: PackManifest["files"],
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  const totalGz = files.reduce((s, f) => s + f.gzBytes, 0);
  // Bytes as they arrive, not files as they finish: the analysis pack is four
  // files and one of them is 2.3 MB, so per-file reporting sat at 0% for the
  // whole download on a phone (2026-07-27).
  let received = 0;
  const out = new Map<string, Uint8Array>();
  // A few files concurrently; decompression overlaps the network.
  const queue = [...files];
  const workers = Array.from({ length: 4 }, async () => {
    for (let f = queue.shift(); f; f = queue.shift()) {
      const url = packFileUrl(f, version);
      const body = await depotBytes(url, (n) => {
        received += n;
        onProgress?.({ fraction: Math.min(1, received / totalGz), currentFile: f.path });
      }).catch((e) => {
        throw new Error(`data pack file ${f.path}: ${e instanceof Error ? e.message : String(e)}`);
      });
      out.set(f.path, await gunzip(body.buffer as ArrayBuffer));
    }
  });
  await Promise.all(workers);
  return out;
}

/** The depot key for a pack file. One function, so the loader, the offline
 *  survey and the cache sweep cannot disagree about what a file is called. */
export function packFileUrl(f: PackManifest["files"][number], version: string): string {
  return `${packUrl(f.path)}.gz?v=${version}`;
}

/** Stage 1 — the FASTEST path to text on screen: the text, plus the tiny stock
 *  study set.
 *
 *  "The text" means the pack's parsed-corpus cache, NOT `kjv.jsonl`: the cache
 *  supersedes it (core opens straight from it when no source file is present),
 *  so shipping both would be 2.5 MB of download nothing ever reads. When this
 *  device already has a usable cache in IndexedDB, neither is fetched. The
 *  raw JSONL is the fallback only for a pack that predates the cache. */
export function fetchPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
  opts: { needText?: boolean } = {},
): Promise<Map<string, Uint8Array>> {
  const packCache = manifest.files.find((f) => f.cache);
  const text = (f: PackManifest["files"][number]) =>
    packCache ? f.cache === true : f.path === "data/kjv.jsonl";
  return fetchFiles(
    manifest.version,
    manifest.files.filter((f) => f.stock || (opts.needText !== false && text(f))),
    onProgress,
  );
}

/** Stage 2 — the rest of the core pack (Strong's, cross-references, margin
 *  notes, bridge witnesses), fetched right after the reader hands over. */
export function fetchStage2Pack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(
    manifest.version,
    manifest.files.filter((f) => !f.rnd && !f.stock && !f.cache && f.path !== "data/kjv.jsonl"),
    onProgress,
  );
}

/** Load the deferred machine-tier (`rnd`) files — fetched in the background
 *  after first paint, never on the boot path (TODO #28). */
export function fetchRndPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(manifest.version, manifest.files.filter((f) => f.rnd), onProgress);
}
