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

/** When the loader fetches a file. The manifest carries this per entry — it is
 *  NOT re-derived from filenames here, which is how four places ended up able to
 *  disagree about which tier a file belonged to. `scripts/build-web-pack.mjs`
 *  owns the assignment; `scripts/check-web-pack.mjs` guards the shape. */
export type PackStage =
  /** Needed before the reader sees a word: the corpus cache + the stock set. */
  | "text"
  /** Strong's, margin notes, cross-references, the overlay, bridge witnesses. */
  | "study"
  /** The machine tier — background, and deferred behind an action on phones. */
  | "analysis";

export interface PackFile {
  path: string;
  bytes: number;
  gzBytes: number;
  /** sha256 of the RAW (decompressed) bytes, 16 hex chars. Raw because a host
   *  may serve `.gz` with `Content-Encoding: gzip`, in which case the app never
   *  sees the compressed form at all (see `gunzip` below). */
  hash: string;
  stage: PackStage;
  /** The bundled stock study set: seeded into the reader's own files once, and
   *  their copies rule afterwards. */
  seedOnce?: true;
  /** The parsed-corpus cache — the one file the fast open depends on, fetched
   *  only when IndexedDB doesn't already hold a usable copy (see boot.ts). */
  role?: "corpusCache";
}

export interface PackManifest {
  /** Bumped on any non-additive change to the entry shape. */
  formatVersion: number;
  version: string;
  files: PackFile[];
}

/** The entry shape this build understands. A pack from the future is refused
 *  rather than mis-tiered: an unknown `stage` would silently make files
 *  unreachable, which surfaces as a boot that hangs with no explanation. */
const SUPPORTED_FORMAT = 2;

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
    return checked(JSON.parse(new TextDecoder().decode(bytes)));
  } catch (e) {
    const hit = await depotGet(url);
    if (!hit) throw new Error(`data pack manifest: ${e instanceof Error ? e.message : String(e)}`);
    return checked(await hit.json());
  }
}

function checked(m: PackManifest): PackManifest {
  if (m.formatVersion !== SUPPORTED_FORMAT) {
    throw new Error(
      `data pack format ${m.formatVersion} — this build understands ${SUPPORTED_FORMAT}. ` +
        `Rebuild the pack (npm run pack:data) or update the app.`,
    );
  }
  return m;
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
  files: PackFile[],
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
export function packFileUrl(f: PackFile, version: string): string {
  return `${packUrl(f.path)}.gz?v=${version}`;
}

/** Stage 1 — the FASTEST path to text on screen: the parsed corpus cache plus
 *  the tiny stock study set (which has to be present AT OPEN so it can seed).
 *
 *  "The text" is the cache, not `kjv.jsonl`: the cache supersedes it, core opens
 *  straight from it, and the raw JSONL is no longer shipped at all. When this
 *  device already holds a usable cache in IndexedDB, `needText: false` skips it
 *  and only the stock set is fetched. */
export function fetchPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
  opts: { needText?: boolean } = {},
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(
    manifest.version,
    manifest.files.filter(
      (f) => f.stage === "text" && (opts.needText !== false || f.role !== "corpusCache"),
    ),
    onProgress,
  );
}

/** Stage 2 — Strong's, cross-references, margin notes, the overlay and the
 *  bridge witnesses, fetched right after the reader hands over. */
export function fetchStage2Pack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(manifest.version, manifest.files.filter((f) => f.stage === "study"), onProgress);
}

/** The machine tier — background after first paint, never on the boot path. */
export function fetchRndPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(manifest.version, manifest.files.filter((f) => f.stage === "analysis"), onProgress);
}
