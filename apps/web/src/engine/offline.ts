// "Is all of it on this device?" — the survey and the repair behind
// Settings → Offline. Most of the app is local after a first visit, but the
// machine tier is deferred on phones until asked for, downloads fail, and browsers
// evict under pressure — none of which announces itself. So this checks every file
// the manifest lists against the depot and re-fetches what is missing.

import { depotBytes, depotHas, requestPersistence } from "./depot";
import { fetchManifest, packFileUrl } from "./pack";

export interface OfflineSurvey {
  /** Pack files not in the offline cache, with their download size. */
  missing: { url: string; gzBytes: number }[];
  /** Bytes still to fetch for a complete offline copy. */
  missingBytes: number;
  /** Files the manifest lists. */
  totalFiles: number;
  /** What the app currently occupies on the device (caches + IndexedDB),
   *  when the browser will tell us. */
  bytesOnDevice?: number;
  /** Whether the browser has promised not to evict us under pressure — shown
   *  because "it's all downloaded" and "it will still be there" are different
   *  claims. */
  persisted?: boolean;
}

/** Everything the manifest promises, as depot keys. A straight walk: the manifest
 *  IS the load spec, and `scripts/check-web-pack.mjs` refuses a pack carrying
 *  anything unreachable, so every entry is a file some stage fetches. */
async function packEntries(): Promise<{ url: string; gzBytes: number }[]> {
  const manifest = await fetchManifest();
  return manifest.files.map((f) => ({ url: packFileUrl(f, manifest.version), gzBytes: f.gzBytes }));
}

export async function surveyOffline(): Promise<OfflineSurvey> {
  const entries = await packEntries();
  const missing: OfflineSurvey["missing"] = [];
  for (const e of entries) if (!(await depotHas(e.url))) missing.push(e);
  const est = await navigator.storage?.estimate?.().catch(() => undefined);
  return {
    missing,
    missingBytes: missing.reduce((n, m) => n + m.gzBytes, 0),
    totalFiles: entries.length,
    bytesOnDevice: est?.usage,
    persisted: await navigator.storage?.persisted?.().catch(() => undefined),
  };
}

/** Fetch every missing pack file into the offline cache, reporting 0..1.
 *  Sequential on purpose: this is background completeness, not a boot path,
 *  and it should not fight the reader for bandwidth. */
export async function completeOffline(onProgress: (fraction: number) => void): Promise<OfflineSurvey> {
  const { missing, missingBytes } = await surveyOffline();
  const total = missingBytes || 1;
  let done = 0;
  // The reader has just said they want this to work with no signal, which is the
  // engagement signal browsers grant persistence on.
  void requestPersistence();
  for (const m of missing) {
    try {
      await depotBytes(m.url); // read-through stores it
    } catch {
      /* offline mid-repair: the next run picks up where this left off */
    }
    done += m.gzBytes;
    onProgress(Math.min(1, done / total));
  }
  return surveyOffline();
}
