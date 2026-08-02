// The home church a shared link carries — the web half of `core::church`.
//
// The point (2026-07-27): one QR hands over both the Bible and the people who
// sent it. Whoever shares sets their church in Settings; the link they share
// carries it; whoever opens that link has it saved locally and sees it in the
// welcome, so a card handed out at a service leads back to that service.
//
// Carried as READABLE query parameters rather than an encoded blob: someone
// deciding whether to open a link should be able to see what is in it, and a
// church that mistypes its own details can fix them by reading the URL.
//
// ── why this is still TypeScript (2026-08-01) ──
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
  /** One free line — when and where they meet. */
  info: string;
  url: string;
}

// No EMPTY_CHURCH constant. One shipped with this module and was never read
// once: `cleanChurch(null)` already returns the empty church, and it is what
// every caller goes through anyway (a church arrives from a query string or a
// settings field, both of which need normalizing). A second source for the same
// value is a second thing to keep in step with `Church`.

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
    info: cut(c?.info, 120),
    url: cut(c?.url, 200),
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
  opts: { startAsNewBeliever?: boolean; at?: string | null } = {},
): string {
  const u = new URL(base);
  // Cleaned here, not left to the caller: an uncapped church used to be able to
  // reach a shared URL from the web and not from the phone, because only the
  // Kotlin twin cleaned inside its own `shareUrl`.
  const c = cleanChurch(church);
  if (hasChurch(c)) {
    u.searchParams.set("church", c.name);
    if (c.info) u.searchParams.set("churchInfo", c.info);
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

/** Only http(s) links are offered as links — a shared parameter is untrusted
 *  input, and `javascript:` must never reach an anchor's href. ASCII control
 *  characters are refused for the same reason: a newline inside an href is not
 *  something a church typed.
 *
 *  Returns the input TRIMMED BUT OTHERWISE UNTOUCHED. The reader typed it; the
 *  address bar should show what they typed. (It used to return `new URL(u).href`,
 *  which normalizes — `https://x.org` came back as `https://x.org/` — while the
 *  Kotlin twin returned the raw string.) */
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
 *  in ui/Church.kt; this used to live inline in Shell.svelte. */
export function churchTitle(church: Church | undefined | null): string {
  const c = cleanChurch(church);
  return [c.name, c.info].filter(Boolean).join(": ") || "Your church";
}

/** Open the church's website, or hand `onNoSite` the label instead — the same
 *  two outcomes as Kotlin's `visitChurch`, which fires an Intent where this
 *  opens a tab. */
export function visitChurch(
  church: Church | undefined | null,
  onNoSite: (title: string) => void,
): void {
  const url = safeChurchUrl(church?.url);
  if (url) window.open(url, "_blank", "noopener,noreferrer");
  else onNoSite(churchTitle(church));
}
