// Fetch the read-only data pack (built by scripts/build-web-pack.mjs):
// manifest first, then the gzipped files, decompressed with the browser's
// DecompressionStream. HTTP-level caching is left to the service worker /
// Cache API layer; this module just loads bytes and reports progress.

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
  const res = await fetch(new URL("pack/manifest.json", assetBase).href);
  if (!res.ok) throw new Error(`data pack manifest: HTTP ${res.status}`);
  return res.json();
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
  let doneGz = 0;
  const out = new Map<string, Uint8Array>();
  // Fetch a few files concurrently; decompression overlaps the network.
  const queue = [...files];
  const workers = Array.from({ length: 4 }, async () => {
    for (let f = queue.shift(); f; f = queue.shift()) {
      onProgress?.({ fraction: doneGz / totalGz, currentFile: f.path });
      const res = await fetch(`${packUrl(f.path)}.gz?v=${version}`);
      if (!res.ok) throw new Error(`data pack file ${f.path}: HTTP ${res.status}`);
      out.set(f.path, await gunzip(await res.arrayBuffer()));
      doneGz += f.gzBytes;
      onProgress?.({ fraction: doneGz / totalGz, currentFile: f.path });
    }
  });
  await Promise.all(workers);
  return out;
}

/** Stage 1 — the FASTEST path to text on screen (TODO #28): the corpus plus
 *  the tiny stock study set. `withIdxcache` adds the pack-shipped parsed-
 *  corpus cache — only wanted on a first visit, when IndexedDB has none;
 *  it turns the 19 MB cold parse into the cache fast path. */
export function fetchPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
  opts: { withIdxcache?: boolean } = {},
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(
    manifest.version,
    manifest.files.filter(
      (f) => f.path === "data/kjv.jsonl" || f.stock || (opts.withIdxcache === true && f.cache),
    ),
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
