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
  | "analysis"
  /** Never fetched unless the reader asks: today the suggested-weave bundle. */
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
  /** The parsed-corpus cache — the one file the fast open depends on, fetched
   *  only when IndexedDB doesn't already hold a usable copy (see boot.ts) — or
   *  the suggested-weave bundle, which the reader downloads from Settings.
   *  Both are found by role rather than by filename, so a rename cannot quietly
   *  unhook them. */
  role?: "corpusCache" | "suggestedWeaves" | `corpus:${string}` | `lexicon:${string}`;
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
 *  ONE number per stage — "stage2 fetch+gunzip" — conflates three unrelated
 *  things: bytes off the network, bytes off this device, and time the thread
 *  simply wasn't available to receive either. Dividing the byte count by it
 *  produces a "connection speed" that is pure invention. So record what each file
 *  cost and WHERE IT CAME FROM, not one number. */
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
   *  `await`, so a frozen thread inflates it without limit. If `netMs` is small
   *  while `ms` is huge, the bytes arrived on time and we were late collecting
   *  them — which is a scheduling bug, not a bandwidth one, and no amount of
   *  shrinking the pack would touch it. */
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
  // handful of files and one of them dwarfs the rest, so per-file reporting would
  // sit at 0% for the whole download on a phone.
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
 *  Without this, a CDN error page served with a 200, or a truncated body, is
 *  stored as a permanently valid file — and the engine then fails to parse it on
 *  every launch with no recovery path. This is what makes the per-file hash
 *  load-bearing rather than decorative. */
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

/** Stage 2 — Strong's, cross-references, margin notes, the overlay and the
 *  bridge witnesses, fetched right after the reader hands over. A device with a
 *  language's corpus installed also gets that language's lexicon here, so the
 *  engine's `strongs_for` pick finds it in the home before `loadCoreData`. */
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

/** A NAMED set of entries, whatever stage they came from.
 *
 *  Every other fetch here selects by tier, because the tier is the question.
 *  This one exists for the case where it is not: `backgroundLoad` diffs a
 *  refreshed manifest against the files the pin actually delivered, and what
 *  comes out is a list, not a tier. Its only caller. */
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

/** The suggested-weave bundle, and ONLY when the reader asked for it.
 *
 *  Found by role, not by filename or stage sweep: it is the one entry a rename
 *  could otherwise unhook silently. Null when this pack has none (an older
 *  pack, or a build with no `stock/weaves/suggested/`), which the Settings row
 *  reads as "nothing to offer" rather than an error. */
/**
 * The files THIS device's pack consists of.
 *
 * Every stage but `optional` always counts. An `optional` entry counts only
 * where the reader has actually asked for it, and the authority on that is the
 * home's install marker — NOT whether the bytes happen to be in the depot,
 * which prune is free to reclaim.
 *
 * `has` is asked PER FILE rather than once for the whole stage, because a reader
 * can easily have one optional file and not another, and one boolean for the
 * whole set would be a lie about half of it — the pin naming a file that is
 * absent, or the update sweep silently pushing a German Bible onto a device that
 * never asked for it.
 *
 * Two things read this and they must not disagree. The update sweep fetches
 * exactly this set, so a deploy never pushes an optional file onto a device
 * that declined it. And the PIN names exactly this set, because the pin's
 * promise is that every file it names is present — a pin listing a file the
 * device deliberately does not have is a false claim, and prune, which keeps
 * what the pin names, would be asked to preserve something that was never
 * there.
 */
/** Whether an optional pack file is one THIS home has asked for.
 *
 *  Keyed on `role`, so a new optional entry has to be named here to count —
 *  which is the failure mode worth designing for: an unnamed optional file
 *  defaults to "not ours", so it is never pinned as present and never swept onto
 *  a device that declined it. The alternative default would put a download the
 *  reader never approved on the update path.
 *
 *  The home is passed structurally rather than as `VirtualHome` so `pack.ts`
 *  stays free of a `home.ts` import — they already point the other way. */
export function hasOptional(home: { suggestedInstalled: boolean; langsInstalled: Set<string> }, f: PackFile): boolean {
  if (f.role === "suggestedWeaves") return home.suggestedInstalled;
  // The lexicon rides with the corpus: one install, one answer, so both roles
  // ask the same question of the same language code.
  const code = langOfRole(f.role);
  return code !== null && home.langsInstalled.has(code);
}

/** The language code a `corpus:xx` / `lexicon:xx` role names, else null.
 *
 *  ONE PARSER, so the loader, the pin and Settings cannot disagree about what a
 *  role means. These were three literal strings — `germanCorpus`,
 *  `germanLexicon`, and a `"de"` compared in a Svelte file — which is why
 *  adding a language meant finding all three. */
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
 * The corpus this boot should actually INFLATE, by role — and therefore the one
 * it should skip.
 *
 * Two corpus caches are in the pack now, and a German reader has both on the
 * device. Without this, stage 1 would gunzip and copy BOTH into the home on
 * every launch — about 63 MB of work and memory before any text appeared,
 * against 35 MB for an English reader — when only one is ever opened.
 *
 * BY `plumbline.lang`, and not by the config, because stage 1 runs before there
 * is an engine to read a config with. That key is already the splash's own seed
 * and is written on every boot and on every language switch, so it is the one
 * answer available this early.
 *
 * BOTH STAY IN THE PIN AND THE DEPOT. This decides what to inflate, never what
 * to keep: switching back to English has to work with no network, and prune
 * keeps exactly what the pin names. Skipping the load is not the same as
 * dropping the file.
 *
 * Every way of being wrong lands on the KJV, which is what a boot does today: a
 * missing key, a stale key, a language whose corpus was never downloaded.
 */
export function corpusRoleFor(lang: string | null, has: (role: string) => boolean): string {
  const wants = (lang ?? "").split(/[-_]/)[0].toLowerCase();
  // BY CONVENTION, not by a table: `crates/core/src/i18n.rs` files every
  // non-English corpus under `corpus:<code>` and this composes the same string.
  // The alternative — a map from language to role — is the thing that had to be
  // edited to add Spanish, and forgetting it is a silent fall back to the KJV.
  const role = `corpus:${wants}`;
  return wants && wants !== "en" && has(role) ? role : "corpusCache";
}

/** Whether this pack file is a corpus cache the reader is NOT reading. */
export function isOtherCorpus(f: PackFile, want: string): boolean {
  return isCorpusRole(f.role) && f.role !== want;
}

export function devicePackFiles(manifest: PackManifest, has: (f: PackFile) => boolean): PackFile[] {
  return manifest.files.filter((f) => f.stage !== "optional" || has(f));
}

export function suggestedWeavesEntry(manifest: PackManifest): PackFile | null {
  return manifest.files.find((f) => f.role === "suggestedWeaves") ?? null;
}

/** A language's corpus cache, and ONLY when the reader has picked it.
 *
 *  Found by role for the same reason as the bundle above. Null when this pack
 *  has none, which a shell reads as "that scripture is not on offer in this
 *  build" — the interface is still translated, over the English text, because
 *  `corpus_for` in crates/ffi falls back rather than failing. */
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
