// Fetch the read-only data pack (built by scripts/build-web-pack.mjs):
// manifest first, then the gzipped files, decompressed with the browser's
// DecompressionStream.
//
// Every read goes through the DEPOT (engine/depot.ts), which serves the
// device's copy when it has one and stores what it downloads. That is not an
// optimisation — it is what makes the pack independent of whether the service
// worker happens to be controlling this worker, which on a first visit is a
// race (see the depot's header).

import { depotBytes, depotDelete, depotGet, depotPut } from "./depot";
import { PERF } from "./perf";

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
  /** Where these bytes live, relative to the app base. Present on files that came
   *  from a PIN, absent on a manifest straight off the network (where it is
   *  derived). Storing it lets two pack generations coexist in the depot: an
   *  unchanged file keeps its URL across a version bump, so re-pinning costs no
   *  bytes. See pin.ts. */
  url?: string;
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

/** Per-file download detail, for the diagnostics panel.
 *
 *  The trace used to carry ONE number per stage — "stage2 fetch+gunzip" — which
 *  conflates three unrelated things: bytes off the network, bytes off this
 *  device, and time the thread simply wasn't available to receive either. On a
 *  phone that read 33,993 ms where a desktop over localhost read 907 ms, and
 *  dividing the byte count by it produced a "connection speed" that was pure
 *  invention (2026-07-28). Never again from one number: record what each file
 *  cost and WHERE IT CAME FROM. */
export interface PackFileTrace {
  path: string;
  gzBytes: number;
  ms: number;
  /** A depot hit costs no network at all; a miss is a real download. */
  from: "depot" | "network";
  /** What the BROWSER'S OWN network stack says this request took, and how many
   *  bytes crossed the wire — independent of when our thread got around to
   *  reading them.
   *
   *  This is the whole argument-settler. Our `ms` above is wall clock around an
   *  `await`, so a frozen thread inflates it without limit: a phone reported
   *  34,448 ms for a 787 KB file sitting beside a 3,304 KB file that took 475 ms
   *  (2026-07-28). If `netMs` is small while `ms` is huge, the bytes arrived on
   *  time and we were late collecting them — which is a scheduling bug, not a
   *  bandwidth one, and no amount of shrinking the pack would touch it. */
  netMs?: number;
  transferBytes?: number;
}

let packTrace: PackFileTrace[] = [];
export function takePackTrace(): PackFileTrace[] {
  return [...packTrace];
}

/** What the browser's network stack says a request cost, as opposed to what our
 *  wall clock around the `await` says. The gap between the two is time the bytes
 *  existed and this thread had not collected them yet. */
function netTiming(url: string): { netMs?: number; transferBytes?: number } {
  try {
    const e = performance.getEntriesByName(url).at(-1) as PerformanceResourceTiming | undefined;
    if (!e) return {};
    return { netMs: Math.round(e.duration), transferBytes: e.transferSize };
  } catch {
    return {}; // no resource timing here: the rest of the row still stands
  }
}

async function fetchFiles(
  version: string,
  files: PackFile[],
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  const totalGz = files.reduce((s, f) => s + f.gzBytes, 0);
  // Bytes as they arrive, not files as they finish: the analysis pack is a
  // handful of files and one of them dwarfs the rest, so per-file reporting sat
  // at 0% for the whole download on a phone (2026-07-27).
  let received = 0;
  const out = new Map<string, Uint8Array>();
  // A few files concurrently; decompression overlaps the network.
  const queue = [...files];
  const workers = Array.from({ length: 4 }, async () => {
    for (let f = queue.shift(); f; f = queue.shift()) {
      const url = packFileUrl(f, version);
      // Which side of the read-through answered, taken from the read itself so
      // it adds no work to the load path.
      const src = { fromDepot: false };
      const t0 = performance.now();
      const body = await depotBytes(
        url,
        (n) => {
          received += n;
          onProgress?.({ fraction: Math.min(1, received / totalGz), currentFile: f.path });
        },
        undefined,
        PERF ? src : undefined,
      ).catch((e) => {
        throw new Error(`data pack file ${f.path}: ${e instanceof Error ? e.message : String(e)}`);
      });
      out.set(f.path, await gunzip(body.buffer as ArrayBuffer));
      if (PERF) {
        packTrace.push({
          path: f.path,
          gzBytes: f.gzBytes,
          ms: Math.round(performance.now() - t0),
          from: src.fromDepot ? "depot" : "network",
          ...netTiming(url),
        });
      }
    }
  });
  await Promise.all(workers);
  return out;
}

/** The depot key for a pack file. One function, so the loader, the offline
 *  survey and the cache sweep cannot disagree about what a file is called.
 *
 *  A pinned file carries its own URL — content-addressed on its hash, so one
 *  changed weave invalidates one URL. A manifest fresh off the network has none
 *  yet, and gets the same scheme derived from the same hash. */
export function packFileUrl(f: PackFile, version: string): string {
  if (f.url) return assetUrl(f.url);
  return f.hash ? `${packUrl(f.path)}.gz?h=${f.hash}` : `${packUrl(f.path)}.gz?v=${version}`;
}

/** sha256 of raw bytes, first 16 hex chars — the same form the manifest carries.
 *  Returns null where `crypto.subtle` is unavailable (it needs a secure context,
 *  so a plain-http origin has none). Callers treat null as "unverified" rather
 *  than "bad": refusing to load on a host that cannot hash would be worse than
 *  loading unverified, which is exactly what every previous build did. */
export async function sha16(raw: Uint8Array): Promise<string | null> {
  try {
    const buf = raw.slice().buffer as ArrayBuffer;
    const d = await crypto.subtle.digest("SHA-256", buf);
    return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("").slice(0, 16);
  } catch {
    return null;
  }
}

/** Check a freshly-stored pack file against the hash the manifest claims, and
 *  delete it if it does not match.
 *
 *  Nothing verified downloaded content before this beyond `res.ok`, so a CDN error
 *  page served with a 200, or a truncated body, was stored as a permanently valid
 *  file — and the engine then failed to parse it on every launch with no recovery
 *  path. This is what makes the per-file hash load-bearing rather than decorative. */
export async function verifyStored(f: PackFile, version: string): Promise<boolean> {
  if (!f.hash) return true;
  const url = packFileUrl(f, version);
  const hit = await depotGet(url);
  if (!hit) return false;
  const raw = await gunzip(await hit.arrayBuffer());
  const got = await sha16(raw);
  if (got === null) return true; // no crypto here: unverified, not rejected
  if (got === f.hash) return true;
  await depotDelete(url);
  return false;
}

/** Read a stage entirely from the DEPOT, or give up. No network, at all — this is
 *  what makes a warm boot cost zero requests, and returning null rather than
 *  falling back to the network is deliberate: the caller decides whether to take
 *  the cold path, so a partially-evicted device gets one coherent decision
 *  instead of a per-file mix of local reads and downloads. */
export async function fetchStageLocal(
  manifest: PackManifest,
  stage: PackFile["stage"],
): Promise<Map<string, Uint8Array> | null> {
  const out = new Map<string, Uint8Array>();
  for (const f of manifest.files) {
    if (f.stage !== stage) continue;
    const hit = await depotGet(packFileUrl(f, manifest.version));
    if (!hit) return null;
    out.set(f.path, await gunzip(await hit.arrayBuffer()));
  }
  return out;
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
