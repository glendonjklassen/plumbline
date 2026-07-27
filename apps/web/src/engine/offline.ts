// "Is all of it on this device?" — the survey and the repair behind
// Settings → Offline.
//
// Most of Plumbline is already local after a first visit: the whole KJV
// arrives as one parsed-corpus cache, and Strong's, the cross-references, the
// margin notes and the bridge data follow in the background. Two things can
// leave a device short of complete, and neither announces itself:
//
//  - the machine-tier pack is deferred on phones until the reader asks for it;
//  - any download can fail, and browsers evict storage under pressure.
//
// So this doesn't just fetch the deferred pack — it checks every file the
// manifest lists against the offline cache and re-fetches what is missing.

import { CACHE, stash } from "./cache";
import { fetchManifest, packUrl } from "./pack";

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
}

/** Everything the manifest promises, as cache keys. */
async function packEntries(): Promise<{ url: string; gzBytes: number }[]> {
  const manifest = await fetchManifest();
  return manifest.files.map((f) => ({
    url: `${packUrl(f.path)}.gz?v=${manifest.version}`,
    gzBytes: f.gzBytes,
  }));
}

export async function surveyOffline(): Promise<OfflineSurvey> {
  const entries = await packEntries();
  const missing: OfflineSurvey["missing"] = [];
  if (typeof caches !== "undefined") {
    const cache = await caches.open(CACHE);
    for (const e of entries) {
      // ignoreVary for the same reason the service worker uses it: these
      // responses come back `Vary: Origin`, and the request that stored one
      // may not have carried the header this lookup does.
      if (!(await cache.match(e.url, { ignoreVary: true }))) missing.push(e);
    }
  } else {
    missing.push(...entries); // no Cache API: nothing is durably offline
  }
  const est = await navigator.storage?.estimate?.().catch(() => undefined);
  return {
    missing,
    missingBytes: missing.reduce((n, m) => n + m.gzBytes, 0),
    totalFiles: entries.length,
    bytesOnDevice: est?.usage,
  };
}

/** Fetch every missing pack file into the offline cache, reporting 0..1.
 *  Sequential on purpose: this is background completeness, not a boot path,
 *  and it should not fight the reader for bandwidth. */
export async function completeOffline(onProgress: (fraction: number) => void): Promise<OfflineSurvey> {
  const { missing, missingBytes } = await surveyOffline();
  const total = missingBytes || 1;
  let done = 0;
  for (const m of missing) {
    try {
      const res = await fetch(m.url);
      if (res.ok) await stash(m.url, res);
    } catch {
      /* offline mid-repair: the next run picks up where this left off */
    }
    done += m.gzBytes;
    onProgress(Math.min(1, done / total));
  }
  return surveyOffline();
}
