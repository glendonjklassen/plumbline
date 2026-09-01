// The web's church helpers against the core's, row for row.
//
// `crates/core/src/church.rs` is the one implementation of the share link: the
// clamps, the query encoding, the Church button's label and the http(s) check.
// Android calls it over the ABI. This shell cannot — a share link is read
// synchronously out of derived state (`s.shareLink`) and inside `{#if}`
// (`safeChurchUrl`), and the engine lives in a worker — so `src/shell/church.ts`
// keeps a copy, and this is what holds the copy honest.
//
// The table is `crates/core/src/church_vectors.json`, read by BOTH sides:
// `church::tests::matches_the_shared_vector_table` in Rust and this file in
// TypeScript. Neither owns it. Add a row and both have to agree about it.
//
// The two copies really had drifted before this existed (2026-08-01):
// `URLSearchParams` form-encodes and Android's `Uri` percent-encodes, so a
// church called "Faith + Hope Chapel" reached the recipient from the phone as
// "Faith  Hope Chapel" — `Uri` left the `+` alone and the receiving
// `URLSearchParams` read it as a space. And the web's `shareUrl` never cleaned
// its argument, so an over-long church could reach a URL from here and not from
// there.
//
// No browser: this is a pure module test that happens to live in the Playwright
// suite, because Playwright is the only test runner this shell has.

import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  PWA_URL,
  cleanChurch,
  clockLabel,
  churchTitle,
  safeChurchUrl,
  shareUrl,
  sharedDevotional,
  sharedLang,
  sharedThread,
  type Church,
} from "../src/shell/church";

interface Row {
  name: string;
  church: Partial<Church>;
  at: string | null;
  /** The share palette's columns. Absent on the rows that predate it, which is
   *  why they are optional here and default to undefined below. */
  lang?: string | null;
  thread?: string | null;
  devotional?: string | null;
  cleaned: Church;
  url: string;
  title: string;
  safeUrl: string | null;
}

const VECTORS = fileURLToPath(new URL("../../../crates/core/src/church_vectors.json", import.meta.url));
const rows: Row[] = JSON.parse(readFileSync(VECTORS, "utf8"));
const EN: Record<string, string> = JSON.parse(
  readFileSync(new URL("../../../crates/core/src/i18n/en.json", import.meta.url), "utf8"),
);

test("the web builds the same share link the core does", () => {
  expect(rows.length, "the table is the parity contract; it should not shrink").toBeGreaterThanOrEqual(8);
  for (const row of rows) {
    const where = `[${row.name}]`;
    expect(cleanChurch(row.church), `cleaned ${where}`).toEqual(row.cleaned);
    expect(
      shareUrl(PWA_URL, row.church as Church, {
        at: row.at,
        lang: row.lang,
        thread: row.thread,
        devotional: row.devotional,
      }),
      `url ${where}`,
    ).toBe(row.url);
    // The fallback is the CALLER's to supply now (church.ts must stay importable
    // outside a browser, and the catalogue is a Svelte module), so this passes
    // the same English string the core's `title` uses — read from the catalogue
    // rather than typed here, or the two could drift and this test would not see
    // it.
    // The meeting line is the caller's too, for the same reason, and it is the
    // half that replaced the old free-text `info`: the clock comes from
    // church.ts (12-hour for English) and the words from the catalogue, so this
    // builds it exactly as the shell does rather than hard-coding a sentence.
    const meets =
      row.church.service == null
        ? ""
        : EN["church.meets"].replace("{time}", clockLabel(row.church.service, "en"));
    expect(churchTitle(row.church as Church, EN["shell.churchFallback"], meets), `title ${where}`).toBe(row.title);
    // Against the CLEANED url: that is the only form a shell ever holds (a
    // church arrives from the config or a query string, both cleaned).
    expect(safeChurchUrl(row.cleaned.url), `safeUrl ${where}`).toBe(row.safeUrl);
  }
});

// The rule the table cannot carry without a thousand percent escapes in it:
// `.slice()` cuts UTF-16 units, so an emoji straddling the 80-character cap
// loses half of itself and the URL carries a lone surrogate.
test("clamping a church name never cuts a character in half", () => {
  const name = "\u{1F600}".repeat(85);
  const cut = cleanChurch({ name }).name;
  expect([...cut]).toHaveLength(80);
  expect(cut).toBe("\u{1F600}".repeat(80));
  expect(cut, "a lone surrogate reaches the recipient as U+FFFD").not.toContain("�");
});

test("only http(s) links are offered as links", () => {
  expect(safeChurchUrl("https://gracebible.org")).toBe("https://gracebible.org");
  expect(safeChurchUrl("  http://gracebible.org/x?y#z  ")).toBe("http://gracebible.org/x?y#z");
  expect(safeChurchUrl("HTTPS://GRACE.ORG")).toBe("HTTPS://GRACE.ORG");

  // A church writes its address the way it is on the sign, so the scheme is
  // supplied when there is none — https, never the insecure one. Mirrors
  // `church::tests::only_http_urls_are_offered_as_links`.
  expect(safeChurchUrl("gracebible.org")).toBe("https://gracebible.org");
  expect(safeChurchUrl("www.gracebible.org")).toBe("https://www.gracebible.org");
  expect(safeChurchUrl("  gracebible.org/welcome  ")).toBe("https://gracebible.org/welcome");
  expect(safeChurchUrl("grace.org:8080/x"), "a port is not a scheme").toBe("https://grace.org:8080/x");

  for (const bad of [
    "javascript:alert(1)",
    "JavaScript:alert(1)",
    // `javascript://…` IS a valid javascript URL — everything after the newline
    // runs, and the `//` makes the first line a comment. It is the case the
    // scheme check exists for; the others have no `://` at all.
    "javascript://grace.org/%0aalert(1)",
    "JAVASCRIPT://grace.org/\nalert(1)",
    "data:text/html,<script>",
    "ftp://files.grace.org",
    "mailto:pastor@grace.org",
    "https://",
    "https://@",
    "",
    "   ",
    null,
    "https://grace.org\njavascript:alert(1)",
    // Supplying a scheme must not make a link out of the church NAME typed into
    // the website field, nor repair a scheme we refuse.
    "Grace Bible Church",
    "gracebible",
    "javascript:void(0)",
  ]) {
    expect(safeChurchUrl(bad), `${JSON.stringify(bad)} must not be offered as a link`).toBeNull();
  }
});

// The table pins what the two implementations BUILD. What they READ is pinned
// here, off the very same rows: every row's url is fed back through the parsers
// and has to yield the columns it was built from.
//
// This is the half a shared vector table cannot check by itself. A builder that
// drops `lang` and a parser that never looks for it agree perfectly and lose the
// reader's language — so the assertion is against the ROW, not against whatever
// the builder happened to emit.
test("the web reads back every parameter the table says the link carries", () => {
  for (const row of rows) {
    const where = `[${row.name}]`;
    const q = new URL(row.url).search;
    expect(sharedLang(q), `lang ${where}`).toBe(row.lang ?? null);
    expect(sharedThread(q), `thread ${where}`).toBe(row.thread ?? null);
    expect(sharedDevotional(q), `devotional ${where}`).toBe(row.devotional ?? null);
  }
});

// A stranger's query string is the least trusted input the app has. Each of
// these must be IGNORED rather than half-applied: an unknown-shaped language
// reaching the engine as a "choice" would leave the reader stuck in it.
test("a stranger's palette parameters are refused", () => {
  expect(sharedLang("?lang=")).toBeNull();
  expect(sharedLang("?lang=not-a-language-code")).toBeNull();
  expect(sharedLang("?church=Grace")).toBeNull();
  expect(sharedLang("?lang=pa")).toBe("pa");
  expect(sharedLang("?lang=pa-IN")).toBe("pa-IN");

  expect(sharedThread("?thread=%20%20")).toBeNull();
  expect(sharedThread(`?thread=${"t".repeat(121)}`)).toBeNull();
  expect(sharedThread(`?thread=${"t".repeat(120)}`)).toHaveLength(120);
  expect(sharedDevotional("?devotional=")).toBeNull();
  // Counted in CODE POINTS, like the name cap: 120 emoji are 120 characters,
  // and a slice by UTF-16 unit would refuse this and cut the next one in half.
  expect(sharedThread(`?thread=${encodeURIComponent("\u{1F600}".repeat(120))}`)).not.toBeNull();
});
