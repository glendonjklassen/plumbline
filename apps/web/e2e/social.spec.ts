import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

// What a stranger sees before they see the app: the link card.
//
// A pasted address used to produce nothing — no title beyond the tab name, no
// description, no image — and iOS had no touch icon, so "Add to Home Screen"
// saved a screenshot of the page instead of the mark. The tags in index.html fix
// that; these tests are here because the way that fix rots is INVISIBLE.
//
// The failure mode worth a test is not a missing tag — a missing card is obvious
// the first time anyone shares the link. It is `og:image:width` and the shipped
// PNG drifting apart. Scrapers trust the declared size (many will not download a
// file to measure it), so a card that claims 1200×630 and ships a 512×512 image
// renders cropped, or letterboxed, or not at all — and every previewer on this
// machine shows the file looking perfectly fine. Same shape for the touch icon:
// a transparent PNG looks white on white everywhere except on an iOS home
// screen, where it is composited on BLACK. So each test DECODES the bytes the
// server actually returns and checks them against what the markup claims.
//
// Written 2026-07-29, for the pre-v1.0.0 audit item about sharing the link.
//
// The two PNGs were generated from public/icon.svg (rsvg-convert for the mark,
// PIL for the composition) so the mark is the real one, on the Light theme's
// paper/ink/gold from crates/core/src/theme.rs.

// The one origin the app is shared from (CLAUDE.md § Releases). Pinned here on
// purpose: a typo'd or stale host in a card tag serves nothing, and nothing in
// the app itself would ever notice — every in-app url is relative.
const PROD = "https://plumblinebible.org";

/** A `<meta>` value by name or property, or "" when the tag is absent. */
async function meta(page: import("@playwright/test").Page, key: string): Promise<string> {
  const el = page.locator(`meta[property="${key}"], meta[name="${key}"]`).first();
  return (await el.count()) ? ((await el.getAttribute("content")) ?? "") : "";
}

/** Fetch an asset a tag names and DECODE it, returning its real pixel size and
 *  whether any of its four corners is transparent.
 *
 *  The card urls are absolute and point at production, which this test run is
 *  not; the icon href is relative. Both are reduced to a BASENAME and resolved
 *  against the page, which also keeps this working whether the suite is served
 *  from a domain root or a repo subpath. */
async function decode(
  page: import("@playwright/test").Page,
  url: string,
): Promise<{ status: number; width: number; height: number; transparentCorner: boolean }> {
  return await page.evaluate(async (u) => {
    const name = new URL(u, location.href).pathname.split("/").pop()!;
    const res = await fetch(new URL(name, location.href).href);
    if (!res.ok) return { status: res.status, width: 0, height: 0, transparentCorner: false };
    const bmp = await createImageBitmap(await res.blob());
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = c.getContext("2d")!;
    ctx.drawImage(bmp, 0, 0);
    const corners: [number, number][] = [
      [0, 0],
      [bmp.width - 1, 0],
      [0, bmp.height - 1],
      [bmp.width - 1, bmp.height - 1],
    ];
    const transparentCorner = corners.some(([x, y]) => ctx.getImageData(x, y, 1, 1).data[3] < 255);
    return { status: res.status, width: bmp.width, height: bmp.height, transparentCorner };
  }, url);
}

test("a pasted link has everything a crawler needs to draw a card", async ({ page }) => {
  await page.goto("/");

  // The sentence a stranger reads. Long enough to say what this is, short enough
  // that no scraper cuts it mid-word.
  //
  // The floor was 60 and is 40, because the description was deliberately cut to
  // "The Holy Bible in a free, private, offline application." (54 chars,
  // 2026-07-30). That is shorter than a search result or a social card will
  // happily show — both have room for roughly 155 — so there is unused space
  // here. It is a judgement about voice, not an oversight, and the floor exists
  // to catch an EMPTY or one-word description rather than to enforce a length.
  const description = await meta(page, "description");
  expect(description.length, "no meta description — a pasted link says nothing about the app").toBeGreaterThan(
    40,
  );
  expect(description.length, "the description will be truncated in the card").toBeLessThan(200);
  expect(description).toContain("The Holy Bible");

  expect(await meta(page, "og:type")).toBe("website");
  expect(await meta(page, "og:site_name")).toBe("Plumbline");
  expect(await meta(page, "og:locale")).toBe("en_US");
  expect(await meta(page, "og:title")).toContain("Plumbline");
  expect(await meta(page, "og:description")).toBe(description);
  expect(await meta(page, "og:image:alt")).not.toBe("");
  expect(await meta(page, "twitter:card")).toBe("summary_large_image");
  expect(await meta(page, "twitter:title")).toContain("Plumbline");
  expect(await meta(page, "twitter:description")).toBe(description);
  expect(await meta(page, "twitter:image:alt")).not.toBe("");

  // Absolute, https, and the production host. A crawler resolves nothing.
  for (const key of ["og:url", "og:image", "twitter:image"]) {
    const value = await meta(page, key);
    expect(value, `${key} must be an absolute url on the production origin`).toMatch(
      new RegExp(`^${PROD}/`),
    );
  }
});

test("the card image is exactly the size the card claims", async ({ page }) => {
  await page.goto("/");

  const declared = {
    width: Number(await meta(page, "og:image:width")),
    height: Number(await meta(page, "og:image:height")),
  };
  // 1200×630 is the size every scraper's large-summary layout is cut for.
  expect(declared, "og:image:width/height must declare the 1.91:1 card size").toEqual({
    width: 1200,
    height: 630,
  });
  expect(await meta(page, "og:image:type")).toBe("image/png");

  for (const key of ["og:image", "twitter:image"]) {
    const url = await meta(page, key);
    const got = await decode(page, url);
    expect(got.status, `${key} names ${url}, which is not being served`).toBe(200);
    expect(
      { width: got.width, height: got.height },
      `${key} ships a ${got.width}×${got.height} image while the tags claim ${declared.width}×${declared.height} — scrapers trust the tags and will crop or drop it`,
    ).toEqual(declared);
  }
});

test("iOS gets a 180x180 opaque touch icon", async ({ page }) => {
  await page.goto("/");

  const link = page.locator('link[rel="apple-touch-icon"]').first();
  await expect(link, "no apple-touch-icon: Add to Home Screen saves a screenshot").toHaveCount(1);
  expect(await link.getAttribute("sizes")).toBe("180x180");

  const href = (await link.getAttribute("href")) ?? "";
  const got = await decode(page, href);
  expect(got.status, `the touch icon href points at ${href}, which is not being served`).toBe(200);
  expect(
    { width: got.width, height: got.height },
    `the touch icon is ${got.width}×${got.height}, but the link says 180x180`,
  ).toEqual({ width: 180, height: 180 });
  // iOS composites a touch icon on BLACK, so a transparent corner is a black
  // corner on the reader's home screen — and it looks perfect everywhere else.
  expect(got.transparentCorner, "the touch icon has transparent corners; iOS will fill them black").toBe(
    false,
  );
});

test("every icon this page links is part of the offline shell", async ({ page }) => {
  // The shell manifest's `publicFiles` list is what the depot precaches, and it
  // is maintained BY HAND in vite.config.ts — so a new icon link and a new depot
  // entry are two separate edits, and forgetting the second one costs a reader
  // their home-screen icon on a device with no network. Read the config as text
  // rather than the built manifest: this holds on a dev server too, and the
  // built-manifest side is already covered (app.spec.ts, "the whole shell is
  // stored after one visit").
  //
  // og-image.png is deliberately NOT on that list — only remote crawlers fetch
  // it — which is why this walks the icon LINKS and not the card tags.
  await page.goto("/");
  const config = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8");
  const hrefs = await page.locator('link[rel~="icon"], link[rel="apple-touch-icon"]').evaluateAll((ls) =>
    ls.map((l) => (l as HTMLLinkElement).getAttribute("href") ?? ""),
  );
  expect(hrefs.length).toBeGreaterThan(1);
  for (const href of hrefs) {
    const name = href.replace(/^\.?\//, "");
    expect(config, `${name} is linked but not in vite.config.ts publicFiles — missing offline`).toContain(
      `"${name}"`,
    );
  }
});
