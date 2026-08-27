// The home church a shared link carries — the web half of `core::church`.
//
// The point: one QR hands over both the Bible and the people who
// sent it. Whoever shares sets their church in Settings; the link they share
// carries it; whoever opens that link has it saved locally and sees it in the
// welcome, so a card handed out at a service leads back to that service.
//
// Carried as READABLE query parameters rather than an encoded blob: someone
// deciding whether to open a link should be able to see what is in it, and a
// church that mistypes its own details can fix them by reading the URL.
//
// ── why this is still TypeScript ──
//
// The clamps, the link and the checks now live in `crates/core/src/church.rs`,
// and Android reaches them through `plumbline_share_url_json`. This shell cannot:
// `s.shareLink` and `safeChurchUrl(...)` are read SYNCHRONOUSLY out of derived
// state and inside `{#if}`, and the engine lives in a worker, so every engine
// call here is a promise. So the web keeps a copy — and the copy is held to the
// core by a shared expectation table: `crates/core/src/church_vectors.json` is
// checked against this file by `e2e/church-parity.spec.ts` and against the core
// by `church::tests::matches_the_shared_vector_table`. Add a case to the table
// and both sides have to agree about it.
//
// That table was generated from THIS file's primitives (URL / URLSearchParams),
// because the thing that opens a shared link is always the web app: form
// encoding is what round-trips a literal `+`, and Android's `Uri` encoding did
// not (a church called "Faith + Hope Chapel" arrived as "Faith  Hope Chapel").

/** The hosted PWA — what every share hands over. The core's `church::PWA_URL`
 *  is the same string, and the vector table's `url` column pins them together. */

export const PWA_URL = "https://plumblinebible.org/";

export interface Church {
  name: string;
  /** When they meet, as MINUTES SINCE LOCAL MIDNIGHT — the same grain as
   *  `config.sundayService`, which for the reader's OWN church is where the
   *  number actually lives (there is one stored value, and `shareUrl` is handed
   *  it). A church arriving from someone else's link carries its own. */
  service: number | null;
  url: string;
}

// No EMPTY_CHURCH constant: `cleanChurch(null)` already returns the empty
// church, and it is what every caller goes through anyway (a church arrives
// from a query string or a settings field, both of which need normalizing). A
// second source for the same value is a second thing to keep in step with
// `Church`.

export const hasChurch = (c: Church | undefined | null): c is Church =>
  !!c && c.name.trim().length > 0;

/** Normalize whatever came off a query string or a settings field.
 *
 *  Truncation counts CODE POINTS (`[...s]`), not UTF-16 units: plain `.slice()`
 *  cuts an emoji straddling the cap in half and puts a lone surrogate in the
 *  URL, which the recipient reads as U+FFFD. Same rule as `church::clean`. */
export function cleanChurch(c: Partial<Church> | undefined | null): Church {
  const cut = (v: unknown, max: number) =>
    typeof v === "string" ? [...v.trim()].slice(0, max).join("") : "";
  return {
    // Capped: these end up in a URL, in a QR, and on a welcome screen, and
    // the length that still scans is finite. Same caps as `church::NAME_MAX`.
    name: cut(c?.name, 80),
    // A minute outside a day is nonsense rather than something to truncate:
    // dropped, so a bad link reads as "never said" (`church::clean`).
    service:
      typeof c?.service === "number" && Number.isInteger(c.service) && c.service >= 0 && c.service < 24 * 60
        ? c.service
        : null,
    url: cut(c?.url, 200),
  };
}

/** The church encoded in `search`, if any. */
export function churchFromQuery(search: string): Church | null {
  const q = new URLSearchParams(search);
  const c = cleanChurch({
    name: q.get("church") ?? "",
    // A time that is not a number, or not a time, reads as "never said".
    service: Number.parseInt(q.get("churchService") ?? "", 10),
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
 *  ordinary link. */
export function shareUrl(
  base: string,
  church: Church | undefined | null,
  opts: { startAsNewBeliever?: boolean; at?: string | null } = {},
): string {
  const u = new URL(base);
  // Cleaned here, not left to the caller, so the web and the Kotlin `shareUrl`
  // twin cap identically — an uncleaned church must not be able to reach a
  // shared URL from one shell and not the other.
  const c = cleanChurch(church);
  if (hasChurch(c)) {
    u.searchParams.set("church", c.name);
    // A NUMBER on the wire, so the recipient's app writes the time their way
    // instead of reading someone else's formatting.
    if (c.service !== null) u.searchParams.set("churchService", String(c.service));
    if (c.url) u.searchParams.set("churchUrl", c.url);
  }
  if (opts.startAsNewBeliever) u.searchParams.set("start", "new");
  // `at` opens the recipient straight at a verse — what a shared PASSAGE means.
  // A refKey ("Ps 23:1") is the frozen compact form, so it travels as-is.
  const at = (opts.at ?? "").trim();
  if (at) u.searchParams.set("at", at);
  return u.href;
}

/** Whether this link asks the welcome to open on the new-believer path. */
export const startsAsNewBeliever = (search: string): boolean =>
  new URLSearchParams(search).get("start") === "new";

/** The verse a link opens at (`?at=Ps 23:1`), or null. Shape-checked here so a
 *  stranger's query string can't send the reader somewhere absurd — the engine
 *  still has the last word on whether the ref exists. */
export function sharedAtRef(search: string): string | null {
  const raw = new URLSearchParams(search).get("at")?.trim();
  if (!raw) return null;
  return /^[1-3]?[A-Za-z]{2,6} \d{1,3}:\d{1,3}$/.test(raw) ? raw : null;
}

/** The destination a launcher shortcut opens (`?open=review`), or null.
 *
 *  The values are the manifest.webmanifest `shortcuts` URLs — the long-press
 *  menu on the installed icon — so this is web-only by nature (an APK shortcut
 *  would be a static `<shortcuts>` resource, not a URL). A whitelist, not a
 *  passthrough: the query string is untrusted input, and anything unrecognized
 *  must fall through to a normal boot, never to a blank surface. */
export type LaunchDestination = "review" | "memorize" | "hymnal";

export function launchDestination(search: string): LaunchDestination | null {
  const raw = new URLSearchParams(search).get("open");
  return raw === "review" || raw === "memorize" || raw === "hymnal" ? raw : null;
}

/** Only http(s) links are offered as links — a shared parameter is untrusted
 *  input, and `javascript:` must never reach an anchor's href. ASCII control
 *  characters are refused for the same reason: a newline inside an href is not
 *  something a church typed.
 *
 *  Returns the input TRIMMED BUT OTHERWISE UNTOUCHED. The reader typed it; the
 *  address bar should show what they typed — no normalization, matching the
 *  Kotlin twin. */
export function safeChurchUrl(url: string | null | undefined): string | null {
  const t = (url ?? "").trim();
  // The cap is the URL field's own (200) with room for percent escapes.
  if (!t || t.length > 800) return null;
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(t)) return null;
  const m = /^(https?):\/\/([^/?#]*)/i.exec(t);
  if (!m) return null;
  // Anything before an `@` in the authority is userinfo, not the host.
  const host = m[2].split("@").pop();
  return host ? t : null;
}

/** What the reader sees on the Church button when there is no site to open:
 *  who and when, which is all we were given. The Kotlin twin is `churchTitle`
 *  in ui/Church.kt.
 *
 *  `fallback` is PASSED IN rather than looked up here. This module is the web
 *  twin of `core::church`, pinned to it by a shared vector table that
 *  e2e/church-parity.spec.ts drives directly in Node — so it has to stay
 *  importable outside a browser, and the catalogue is a reactive Svelte module
 *  that is not. The one string it needed is the caller's to supply. */
export function churchTitle(church: Church | undefined | null, fallback = "", meets = ""): string {
  const c = cleanChurch(church);
  return [c.name, c.service !== null ? meets : ""].filter(Boolean).join(": ") || fallback;
}

/** "10:00 AM" / "10:00" — the clock the reader's language writes.
 *
 *  Twin of `church::clock`. 12-hour for English and 24-hour otherwise, which is
 *  the half of a meeting time that actually differs between them; the words
 *  around it come from the catalogue (`church.meets`) and are the caller's to
 *  supply, for the same reason `churchTitle`'s fallback is. */
export function clockLabel(minutes: number, lang: string): string {
  const m = ((minutes % (24 * 60)) + 24 * 60) % (24 * 60);
  const h = Math.floor(m / 60);
  const min = String(m % 60).padStart(2, "0");
  if (lang !== "en") return `${String(h).padStart(2, "0")}:${min}`;
  return `${h % 12 === 0 ? 12 : h % 12}:${min} ${h < 12 ? "AM" : "PM"}`;
}

/** Open the church's website, or hand `onNoSite` the label instead — the same
 *  two outcomes as Kotlin's `visitChurch`, which fires an Intent where this
 *  opens a tab. */
export function visitChurch(
  church: Church | undefined | null,
  onNoSite: (title: string) => void,
  fallback = "",
): void {
  const url = safeChurchUrl(church?.url);
  if (url) window.open(url, "_blank", "noopener,noreferrer");
  else onNoSite(churchTitle(church, fallback));
}
