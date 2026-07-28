// "A new version is ready" — noticing a deploy while the app is still open.
//
// The service worker is NOT the signal. `sw.js` is a static file that rarely
// changes between releases, so `updatefound` stays silent through a release
// that only rebuilt the app; and because index.html is network-first, a plain
// relaunch already picks new code up. What was missing is the case where the
// reader simply never closes the tab (an installed PWA on a phone can sit for
// weeks) — they would keep running whatever they booted.
//
// So compare what is DEPLOYED against what is RUNNING: index.html names the
// content-hashed entry bundle, and this page knows which one it loaded. A
// different name means a different build. The request goes through the SW's
// network-first path, so offline it answers from cache — the same bundle, no
// false alarm.

/** The hashed entry bundle named by an index.html, e.g. `index-bK5YUOSQ.js`. */
export function bundleIn(html: string): string | null {
  const m = /<script[^>]*\ssrc="([^"]*\/assets\/[^"]+\.js)"/i.exec(html);
  return m ? (m[1].split("/").pop() ?? null) : null;
}

/** The bundle THIS page is running. */
export function runningBundle(): string | null {
  const el = document.querySelector<HTMLScriptElement>('script[type="module"][src*="/assets/"]');
  return el ? (new URL(el.src).pathname.split("/").pop() ?? null) : null;
}

/** Whether a newer build is deployed. False on any doubt — an unreadable
 *  index.html, a missing script tag, no network — because a spurious "update
 *  ready" that reloads into the same build is worse than a late one. */
export async function updateAvailable(): Promise<boolean> {
  const mine = runningBundle();
  if (!mine) return false;
  try {
    const res = await fetch(new URL("index.html", document.baseURI).href, { cache: "no-store" });
    if (!res.ok) return false;
    const theirs = bundleIn(await res.text());
    return !!theirs && theirs !== mine;
  } catch {
    return false;
  }
}
