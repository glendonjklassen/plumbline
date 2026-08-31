// "A new version is ready" — noticing a deploy while the app is still open.
//
// The service worker is NOT the signal: `sw.js` rarely changes between releases, so
// `updatefound` stays silent through a release that only rebuilt the app. The case
// this covers is the reader who never closes the tab — an installed PWA can sit for
// weeks running whatever it booted.
//
// So compare what is DEPLOYED against what is RUNNING, using the build id the shell
// manifest carries. Offline the request answers from cache — the same id, no false
// alarm.
//
// It asks shell-manifest.json, never index.html: scraping the document caused a
// white screen, because the SW's network-first branch stored a newer index.html
// while that build's `/assets/*` were absent, and the next offline launch asked for
// a bundle nobody had. A field beats a regex over markup.

import { assetUrl } from "./pack";
import type { ShellManifest } from "./precache";

/** The build id deployed right now, or null if it cannot be read. Module-private:
 *  `updateAvailable` is the whole public surface, so the "false on any doubt" rule
 *  is implemented once. */
async function deployedBuildId(): Promise<string | null> {
  try {
    // no-store: this must see the deploy, not our own stored copy. sw.js declines
    // to cache no-store requests, so asking cannot poison anything.
    const res = await fetch(assetUrl("shell-manifest.json"), { cache: "no-store" });
    if (!res.ok) return null;
    const m = (await res.json()) as ShellManifest;
    return typeof m.buildId === "string" ? m.buildId : null;
  } catch {
    return null;
  }
}

/** Whether a newer build is deployed. False on any doubt (unreachable manifest,
 *  malformed one, no network): a spurious "update ready" that reloads into the same
 *  build is worse than a late one. */
export async function updateAvailable(): Promise<boolean> {
  const theirs = await deployedBuildId();
  return !!theirs && theirs !== __BUILD_ID__;
}
