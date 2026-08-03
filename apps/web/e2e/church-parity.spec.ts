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
  churchTitle,
  safeChurchUrl,
  shareUrl,
  type Church,
} from "../src/shell/church";

interface Row {
  name: string;
  church: Partial<Church>;
  startAsNewBeliever: boolean;
  at: string | null;
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
        startAsNewBeliever: row.startAsNewBeliever,
        at: row.at,
      }),
      `url ${where}`,
    ).toBe(row.url);
    // The fallback is the CALLER's to supply now (church.ts must stay importable
    // outside a browser, and the catalogue is a Svelte module), so this passes
    // the same English string the core's `title` uses — read from the catalogue
    // rather than typed here, or the two could drift and this test would not see
    // it.
    expect(churchTitle(row.church as Church, EN["shell.churchFallback"]), `title ${where}`).toBe(row.title);
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
    "gracebible.org",
    "https://",
    "https://@",
    "",
    "   ",
    null,
    "https://grace.org\njavascript:alert(1)",
  ]) {
    expect(safeChurchUrl(bad), `${JSON.stringify(bad)} must not be offered as a link`).toBeNull();
  }
});
