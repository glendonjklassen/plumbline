// Fetch the read-only data pack (built by scripts/build-web-pack.mjs): manifest
// first, then the gzipped files, decompressed with DecompressionStream.
//
// Every read goes through the DEPOT (engine/depot.ts), which is what makes the
// pack independent of whether the service worker happens to control this worker —
// on a first visit that is a race (see the depot's header).

import { depotBytes, depotDelete, depotGet, depotPut } from "./depot";
import { PERF } from "./perf";
import { shippedBase } from "../lib/locale";

/** When the loader fetches a file. The MANIFEST IS THE LOAD SPEC: this is carried
 *  per entry, never re-derived from filenames here, or several places end up able
 *  to disagree about a file's tier. `scripts/build-web-pack.mjs` owns the
 *  assignment; `scripts/check-web-pack.mjs` guards the shape. */
export type PackStage =
  /** Needed before the reader sees a word: the corpus cache + the stock set. */
  | "text"
  /** Strong's, margin notes, cross-references, the overlay, bridge witnesses. */
  | "study"
  /** The machine tier — background, and deferred behind an action on phones. */
  | "analysis"
  /** Every language's Bible except the one this boot opens: downloaded in the
   *  background once there is text on screen, so picking a language is a switch
   *  rather than a download. Only the opened one is inflated into the home
   *  ([isOtherCorpus]); the rest sit in the depot as compressed bytes. */
  | "corpus"
  /** Never fetched unless the reader asks: the suggested-weave bundle and the
   *  machine-translated dictionaries. */
  | "optional";

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
  /** The parsed-corpus cache (the one file the fast open depends on) or the
   *  suggested-weave bundle. Found by role rather than filename, so a rename
   *  cannot quietly unhook them. */
  role?: "corpusCache" | "suggestedWeaves" | `corpus:${string}` | `lexicon:${string}`;
  /** Where these bytes live, relative to the app base. Present on files from a
   *  PIN, absent on a manifest straight off the network (where it is derived).
   *  Storing it lets two pack generations coexist in the depot: an unchanged file
   *  keeps its URL across a version bump. See pin.ts. */
  url?: string;
}

export interface PackManifest {
  /** Bumped on any non-additive change to the entry shape. */
  formatVersion: number;
  version: string;
  files: PackFile[];
}

/** The entry shape this build understands. A pack from the future is refused
 *  rather than mis-tiered: an unknown `stage` would make files silently
 *  unreachable, surfacing as a boot that hangs with no explanation. */
const SUPPORTED_FORMAT = 2;

export interface PackProgress {
  /** 0..1 across the whole pack download, weighted by gzipped size. */
  fraction: number;
  currentFile: string;
}

// The app's asset base as an ABSOLUTE url. Vite's BASE_URL is "./", which resolves
// against the *current script* — wrong inside the engine worker, which lives under
// assets/. The main thread passes the resolved page base in before booting.
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
  // Network first, depot as the fallback: the manifest is the one pack file with
  // no version in its URL, so a stored copy can be stale. Offline, that stored copy
  // is what lets the rest of the pack be found at all.
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

/** Per-file download detail, for the diagnostics panel. Per file and with its
 *  source, because one number per stage conflates bytes off the network, bytes off
 *  this device, and time the thread was not available to receive either. */
export interface PackFileTrace {
  path: string;
  gzBytes: number;
  ms: number;
  /** A depot hit costs no network at all; a miss is a real download. */
  from: "depot" | "network";
  /** What the browser's own network stack says the request took, and how many
   *  bytes crossed the wire. `ms` above is wall clock around an `await`, which a
   *  frozen thread inflates without limit — a small `netMs` beside a huge `ms`
   *  means a scheduling problem, not a bandwidth one. */
  netMs?: number;
  transferBytes?: number;
}

let packTrace: PackFileTrace[] = [];
export function takePackTrace(): PackFileTrace[] {
  return [...packTrace];
}

/** What the browser's network stack says a request cost. The gap against our own
 *  wall clock is time the bytes existed uncollected by this thread. */
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
  // Bytes as they arrive, not files as they finish: one analysis file dwarfs the
  // rest, so per-file reporting would sit at 0% for the whole download.
  let received = 0;
  const out = new Map<string, Uint8Array>();
  // A few files concurrently; decompression overlaps the network.
  const queue = [...files];
  const workers = Array.from({ length: 4 }, async () => {
    for (let f = queue.shift(); f; f = queue.shift()) {
      const url = packFileUrl(f, version);
      // Which side of the read-through answered, taken from the read itself so it
      // adds no work to the load path.
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

/** The depot key for a pack file. One function, so the loader, the offline survey
 *  and the cache sweep cannot disagree about what a file is called. A pinned file
 *  carries its own URL (content-addressed on its hash, so one changed weave
 *  invalidates one URL); a fresh manifest gets the same scheme derived. */
export function packFileUrl(f: PackFile, version: string): string {
  if (f.url) return assetUrl(f.url);
  return f.hash ? `${packUrl(f.path)}.gz?h=${f.hash}` : `${packUrl(f.path)}.gz?v=${version}`;
}

/** sha256 of raw bytes, first 16 hex chars — the form the manifest carries. Null
 *  where `crypto.subtle` is unavailable (it needs a secure context, so a plain-http
 *  origin has none); callers treat null as "unverified", not "bad". */
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
 *  delete it if it does not match. Without this, a CDN error page served with a
 *  200, or a truncated body, is stored as a permanently valid file the engine then
 *  fails to parse on every launch with no recovery path. */
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

/** Read a stage entirely from the DEPOT, or give up. No network at all — this is
 *  what makes a warm boot cost zero requests before text. Returning null rather
 *  than falling back leaves the cold-path decision to the caller, so a
 *  partially-evicted device makes one coherent choice rather than a per-file mix. */
export async function fetchStageLocal(
  manifest: PackManifest,
  stage: PackFile["stage"],
  also: (f: PackFile) => boolean = () => false,
  skip: (f: PackFile) => boolean = () => false,
): Promise<Map<string, Uint8Array> | null> {
  const out = new Map<string, Uint8Array>();
  for (const f of manifest.files) {
    if (f.stage !== stage && !also(f)) continue;
    if (skip(f)) continue;
    const hit = await depotGet(packFileUrl(f, manifest.version));
    if (!hit) return null;
    out.set(f.path, await gunzip(await hit.arrayBuffer()));
  }
  return out;
}

/** Stage 1 — the fastest path to text on screen: the parsed corpus cache (which
 *  supersedes `kjv.jsonl`; the raw JSONL is not shipped) plus the tiny stock study
 *  set, which must be present AT OPEN so it can seed. `needText: false` skips the
 *  cache when this device already holds a usable one. */
export function fetchPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
  opts: { needText?: boolean; also?: (f: PackFile) => boolean; skip?: (f: PackFile) => boolean } = {},
): Promise<Map<string, Uint8Array>> {
  const also = opts.also ?? (() => false);
  const skip = opts.skip ?? (() => false);
  return fetchFiles(
    manifest.version,
    manifest.files.filter(
      (f) =>
        !skip(f) && ((f.stage === "text" && (opts.needText !== false || f.role !== "corpusCache")) || also(f)),
    ),
    onProgress,
  );
}

/** Stage 2 — Strong's, cross-references, margin notes, the overlay and the bridge
 *  witnesses, fetched right after the reader hands over. A device with a language's
 *  corpus installed also gets that language's lexicon here, so the engine's
 *  `strongs_for` finds it in the home before `loadCoreData`. */
export function fetchStage2Pack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
  langsInstalled: Set<string> = new Set(),
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(
    manifest.version,
    manifest.files.filter((f) => {
      if (f.stage === "study") return true;
      const code = langOfRole(f.role);
      return f.role?.startsWith("lexicon:") === true && code !== null && langsInstalled.has(code);
    }),
    onProgress,
  );
}

/** A NAMED set of entries, whatever stage they came from. For its one caller,
 *  `backgroundLoad`, which diffs a refreshed manifest against the files the pin
 *  actually delivered — what comes out is a list, not a tier. */
export function fetchPackEntries(
  version: string,
  files: PackFile[],
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(version, files, onProgress);
}

/** The machine tier — background after first paint, never on the boot path. */
export function fetchRndPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  return fetchFiles(manifest.version, manifest.files.filter((f) => f.stage === "analysis"), onProgress);
}

/** Whether an optional pack file is one THIS home has asked for.
 *
 *  Keyed on `role`, so an unnamed optional file defaults to "not ours": it is then
 *  never pinned as present and never swept onto a device that declined it. The
 *  authority is the home's install marker, not whether the bytes happen to be in
 *  the depot, which prune is free to reclaim.
 *
 *  The home is passed structurally rather than as `VirtualHome`, so `pack.ts` stays
 *  free of a `home.ts` import — they already point the other way. */
export function hasOptional(home: { suggestedInstalled: boolean; langsInstalled: Set<string> }, f: PackFile): boolean {
  if (f.role === "suggestedWeaves") return home.suggestedInstalled;
  // NOT the corpora: a Bible is `stage: "corpus"` and ships to every device, so it
  // is not something a reader opts into. `devicePackFiles` takes it on its stage,
  // stage 1 takes the opened one by role. Answering false rather than dropping the
  // branch, because `langsInstalled` still carries codes written by the old install
  // flow on upgrading devices, and a stale marker must not start meaning something
  // new.
  if (isCorpusRole(f.role)) return false;
  // The dictionary is still an ask, and still keyed by language code.
  const code = langOfRole(f.role);
  return code !== null && home.langsInstalled.has(code);
}

/** The language code a `corpus:xx` / `lexicon:xx` role names, else null. One
 *  parser, so the loader, the pin and Settings cannot disagree about what a role
 *  means, and adding a language touches no literal string elsewhere. */
export function langOfRole(role: string | undefined): string | null {
  if (!role) return null;
  const at = role.indexOf(":");
  if (at < 0) return null;
  const kind = role.slice(0, at);
  return kind === "corpus" || kind === "lexicon" ? role.slice(at + 1) : null;
}

/** Whether this entry is any corpus cache — English's distinguished role, or a
 *  language's own. */
export function isCorpusRole(role: string | undefined): boolean {
  return role === "corpusCache" || (!!role && role.startsWith("corpus:"));
}

/**
 * The corpus this boot should INFLATE, by role — and therefore the ones it skips.
 * Several corpus caches ship, and inflating them all would cost tens of MB of work
 * and memory before any text appeared when only one is ever opened.
 *
 * Decided by `plumbline.lang` and then the device locale, because stage 1 runs
 * before there is an engine to read a config with, and on a genuinely cold start
 * that key is still empty (it is written at the END of a boot). Those two arguments
 * in that order ARE `i18n::resolve(chosen, device)` in crates/core: the same rule
 * decides the interface and the text, or an app ends up with its chrome and its
 * scripture in different languages.
 *
 * Skipping a load is not dropping a file: every corpus stays in the pin and the
 * depot, so switching back to English works with no network.
 *
 * `has` asks the MANIFEST — whether this build carries that language's text at all
 * — not the reader's install history: every Bible ships now, so asking about an
 * install would answer "no" for a corpus sitting in the depot. Every way of being
 * wrong lands on the KJV.
 */
export function corpusRoleFor(lang: string | null, locale: string | null, has: (role: string) => boolean): string {
  // Stored answer first, hardware second: a stored "en" must win over an Arabic
  // device, or a reader who chose English is overruled by their phone every launch.
  // `shippedBase` rather than a bare base-tag strip, because a Chinese device says
  // `zh-TW` while the corpus roles say `zht`/`zhs`, and a strip to `zh` misses every
  // role and boots the KJV under a Chinese interface.
  const wants = shippedBase(lang) || shippedBase(locale);
  // By convention, not by a table: `crates/core/src/i18n.rs` files every non-English
  // corpus under `corpus:<code>` and this composes the same string. A language→role
  // map would be one more place a new language has to be registered, and forgetting
  // it falls back silently to the KJV.
  const role = `corpus:${wants}`;
  return wants && wants !== "en" && has(role) ? role : "corpusCache";
}

/** Whether this pack file is a corpus cache the reader is NOT reading. */
export function isOtherCorpus(f: PackFile, want: string): boolean {
  return isCorpusRole(f.role) && f.role !== want;
}

/** The Bibles this device should hold but is not reading — what the background pass
 *  downloads into the depot after there is text on screen.
 *
 *  DEPOT ONLY: bytes on disk, never files in the in-memory home (three corpora in
 *  the home is ~91 MB — see [isOtherCorpus]). Ordered smallest first, so a reader
 *  who closes the tab mid-run has whole files and the most Bibles, not the fewest. */
export function otherCorpora(manifest: PackManifest, want: string): PackFile[] {
  return manifest.files
    .filter((f) => f.stage === "corpus" && isOtherCorpus(f, want))
    .sort((a, b) => a.gzBytes - b.gzBytes);
}

/** The files THIS device's pack consists of: every stage but `optional`, plus the
 *  optional entries `has` says the reader asked for. Asked per file, because a
 *  reader can have one and not another. Both readers must agree on this set — the
 *  update sweep fetches exactly it, so a deploy never pushes a declined download;
 *  and the pin names exactly it, its promise being that every file it names is
 *  present. */
export function devicePackFiles(manifest: PackManifest, has: (f: PackFile) => boolean): PackFile[] {
  return manifest.files.filter((f) => f.stage !== "optional" || has(f));
}

/** The suggested-weave bundle, found by role rather than filename so a rename
 *  cannot unhook it. Null when this pack ships none, which the Settings row reads
 *  as "nothing to offer" rather than an error. */
export function suggestedWeavesEntry(manifest: PackManifest): PackFile | null {
  return manifest.files.find((f) => f.role === "suggestedWeaves") ?? null;
}

/** A language's corpus cache, by role. Null when this pack ships none: the
 *  interface is still translated over the English text, because `corpus_for` in
 *  crates/ffi falls back rather than failing. */
export function langCorpusEntry(manifest: PackManifest, code: string): PackFile | null {
  return manifest.files.find((f) => f.role === `corpus:${code}`) ?? null;
}

/** A language's own Strong's dictionary, when this pack ships one — installed
 *  with the corpus (one ask covers both), read by the engine's `strongs_for`. */
export function langLexiconEntry(manifest: PackManifest, code: string): PackFile | null {
  return manifest.files.find((f) => f.role === `lexicon:${code}`) ?? null;
}

/** Fetch a language's lexicon into the depot (read-through: boot finds it
 *  there). Null when the pack ships none — that language's study then serves
 *  the English dictionary, which the engine falls back to by itself. */
export async function fetchLangLexicon(manifest: PackManifest, code: string): Promise<Uint8Array | null> {
  const entry = langLexiconEntry(manifest, code);
  if (!entry) return null;
  const got = await fetchFiles(manifest.version, [entry]);
  return got.get(entry.path) ?? null;
}

export async function fetchLangCorpus(
  manifest: PackManifest,
  code: string,
  onProgress?: (p: PackProgress) => void,
): Promise<Uint8Array | null> {
  const entry = langCorpusEntry(manifest, code);
  if (!entry) return null;
  const got = await fetchFiles(manifest.version, [entry], onProgress);
  return got.get(entry.path) ?? null;
}

export async function fetchSuggestedWeaves(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Uint8Array | null> {
  const entry = suggestedWeavesEntry(manifest);
  if (!entry) return null;
  const got = await fetchFiles(manifest.version, [entry], onProgress);
  return got.get(entry.path) ?? null;
}
