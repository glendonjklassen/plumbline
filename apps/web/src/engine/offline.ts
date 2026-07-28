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

import { depotBytes, depotHas, requestPersistence } from "./depot";
import { fetchManifest, packFileUrl, type PackManifest } from "./pack";

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
  /** Whether the browser has promised not to evict us under pressure. Shown
   *  because "it's all downloaded" and "it will still be there" are different
   *  claims, and only one of them is ours to make. */
  persisted?: boolean;
}

/** Everything the manifest promises that the app will actually READ, as depot
 *  keys.
 *
 *  `data/kjv.jsonl` is excluded deliberately. The pack ships it (2.4 MB gz) but
 *  no stage fetches it — the parsed-corpus idxcache supersedes it, and core
 *  opens straight from that. Counting it made Settings report the device
 *  permanently "incomplete" and made "Download everything" pull 2.4 MB that
 *  nothing ever opens. */
async function packEntries(): Promise<{ url: string; gzBytes: number }[]> {
  const manifest = await fetchManifest();
  const consumed = manifest.files.filter((f) => !isUnreadFallback(f, manifest));
  return consumed.map((f) => ({ url: packFileUrl(f, manifest.version), gzBytes: f.gzBytes }));
}

/** The raw JSONL is a fallback for a pack that predates the corpus cache: with
 *  a cache present it is never fetched, so it is not part of "complete". */
function isUnreadFallback(f: PackManifest["files"][number], manifest: PackManifest): boolean {
  return f.path === "data/kjv.jsonl" && manifest.files.some((o) => o.cache);
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
  // Ask for durability while we are on the subject: the reader has just told us
  // they want this to work with no signal, which is exactly the engagement
  // signal browsers grant persistence on.
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
