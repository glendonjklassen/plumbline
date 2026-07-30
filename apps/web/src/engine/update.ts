// "A new version is ready" — noticing a deploy while the app is still open.
//
// The service worker is NOT the signal. `sw.js` is a static file that rarely
// changes between releases, so `updatefound` stays silent through a release that
// only rebuilt the app; and because index.html is network-first, a plain relaunch
// already picks new code up. What was missing is the reader who simply never
// closes the tab — an installed PWA on a phone can sit for weeks, running
// whatever it booted.
//
// So compare what is DEPLOYED against what is RUNNING, using the build id the
// shell manifest carries. The request goes through the SW's network-first path,
// so offline it answers from cache — the same id, no false alarm.
//
// THIS USED TO SCRAPE index.html, AND THAT WAS A WHITE-SCREEN BUG. It fetched
// index.html as data and regexed out the entry bundle's filename; the service
// worker's network-first branch caches every ok response, so a session that
// merely CHECKED for updates wrote a NEWER index.html into the cache while that
// build's `/assets/*` were absent. A later offline launch was then served the new
// shell, asked for a bundle nobody had, and got nothing — a white screen on a
// device holding every byte of scripture. sw.js now refuses to cache `no-store`
// requests and non-navigation index.html, but the deeper fix is not to ask for
// index.html at all: shell-manifest.json is a few hundred bytes, it is not the
// document, and `buildId` is a field instead of a regex over markup.

import { assetUrl } from "./pack";
import type { ShellManifest } from "./precache";

/** The build id deployed right now, or null if it cannot be read.
 *
 *  Module-private: `updateAvailable` is the whole public surface. A caller
 *  outside would have to reimplement the "false on any doubt" rule, which is the
 *  only part of this that keeps a spurious "update ready" from reloading a reader
 *  into the same build. */
async function deployedBuildId(): Promise<string | null> {
  try {
    // no-store: this must see the deploy, not our own stored copy. Paired with
    // sw.js declining to cache no-store requests, so asking cannot poison
    // anything.
    const res = await fetch(assetUrl("shell-manifest.json"), { cache: "no-store" });
    if (!res.ok) return null;
    const m = (await res.json()) as ShellManifest;
    return typeof m.buildId === "string" ? m.buildId : null;
  } catch {
    return null;
  }
}

/** Whether a newer build is deployed. False on any doubt — an unreachable
 *  manifest, a malformed one, no network — because a spurious "update ready"
 *  that reloads into the same build is worse than a late one. */
export async function updateAvailable(): Promise<boolean> {
  const theirs = await deployedBuildId();
  return !!theirs && theirs !== __BUILD_ID__;
}
