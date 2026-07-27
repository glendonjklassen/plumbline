// The home church a shared link carries.
//
// The point (2026-07-27): one QR hands over both the Bible and the people who
// sent it. Whoever shares sets their church in Settings; the link they share
// carries it; whoever opens that link has it saved locally and sees it in the
// welcome — so a card handed out at a service leads back to that service.
//
// Carried as READABLE query parameters rather than an encoded blob: someone
// deciding whether to open a link should be able to see what is in it, and a
// church that mistypes its own details can fix them by reading the URL.

/** The hosted PWA — what every share hands over. Keep in sync with the
 *  Android twin (QrShare.kt). */
export const PWA_URL = "https://plumblinebible.org/";

export interface Church {
  name: string;
  /** One free line — when and where they meet. */
  info: string;
  url: string;
}

export const EMPTY_CHURCH: Church = { name: "", info: "", url: "" };

export const hasChurch = (c: Church | undefined | null): c is Church =>
  !!c && c.name.trim().length > 0;

/** Normalize whatever came off a query string or a settings field. */
export function cleanChurch(c: Partial<Church> | undefined | null): Church {
  const trim = (v: unknown, max: number) => (typeof v === "string" ? v.trim().slice(0, max) : "");
  return {
    // Capped: these end up in a URL, in a QR, and on a welcome screen, and
    // the length that still scans is finite.
    name: trim(c?.name, 80),
    info: trim(c?.info, 120),
    url: trim(c?.url, 200),
  };
}

/** The church encoded in `search`, if any. */
export function churchFromQuery(search: string): Church | null {
  const q = new URLSearchParams(search);
  const c = cleanChurch({
    name: q.get("church") ?? "",
    info: q.get("churchInfo") ?? "",
    url: q.get("churchUrl") ?? "",
  });
  return hasChurch(c) ? c : null;
}

/** A share link for `base` carrying `church` (plain `base` when unset).
 *
 *  `startAsNewBeliever` marks the link as one handed to someone meeting the
 *  Bible — the recipient's welcome opens on the new-believer path instead of
 *  asking them to pick. ONLY the Present screen sets it: an ordinary share
 *  goes to whoever, often someone from the same church, and must stay an
 *  ordinary link (2026-07-27). */
export function shareUrl(
  base: string,
  church: Church | undefined | null,
  opts: { startAsNewBeliever?: boolean } = {},
): string {
  const u = new URL(base);
  if (hasChurch(church)) {
    u.searchParams.set("church", church.name);
    if (church.info) u.searchParams.set("churchInfo", church.info);
    if (church.url) u.searchParams.set("churchUrl", church.url);
  }
  if (opts.startAsNewBeliever) u.searchParams.set("start", "new");
  return u.href;
}

/** Whether this link asks the welcome to open on the new-believer path. */
export const startsAsNewBeliever = (search: string): boolean =>
  new URLSearchParams(search).get("start") === "new";

/** Only http(s) links are offered as links — a shared parameter is untrusted
 *  input, and `javascript:` must never reach an anchor's href. */
export function safeChurchUrl(url: string): string | null {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:" ? u.href : null;
  } catch {
    return null;
  }
}
