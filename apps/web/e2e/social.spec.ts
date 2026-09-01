import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

// The link card a stranger sees before the app. The failure worth testing is not a
// missing tag — a missing card is obvious the first time anyone shares the link — it
// is `og:image:width` drifting from the shipped PNG. Scrapers trust the declared size
// rather than measuring the file, so a card claiming 1200×630 that ships a 512×512
// image renders cropped or not at all, while every local previewer looks fine. Same
// shape for the touch icon: a transparent PNG looks white everywhere except on an iOS
// home screen, which composites it on black. So each test DECODES the served bytes
// and checks them against what the markup claims.
//
// The two PNGs come from public/icon.svg on the Light theme's paper/ink/gold
// (crates/core/src/theme.rs); regenerate them from there.

// The one origin the app is shared from (CLAUDE.md § Releases). A typo'd or stale host
// in a card tag serves nothing, and the app itself would never notice — every in-app
// url is relative.
const PROD = "https://plumblinebible.org";

/** A `<meta>` value by name or property, or "" when the tag is absent. */
async function meta(page: import("@playwright/test").Page, key: string): Promise<string> {
  const el = page.locator(`meta[property="${key}"], meta[name="${key}"]`).first();
  return (await el.count()) ? ((await el.getAttribute("content")) ?? "") : "";
}

/** Fetch an asset a tag names and decode it, returning its real pixel size and whether
 *  any corner is transparent.
 *
 *  Card urls are absolute and point at production, which this run is not; the icon href
 *  is relative. Both are reduced to a basename and resolved against the page, which also
 *  works whether the suite is served from a domain root or a repo subpath. */
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

  // The sentence a stranger reads. The shipped description is deliberately shorter
  // than the ~155 characters a card will show, so the floor of 40 is here to catch an
  // empty or one-word description rather than to enforce a length.
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
  // iOS composites a touch icon on black, so a transparent corner is a black corner on
  // the reader's home screen — and it looks perfect everywhere else.
  expect(got.transparentCorner, "the touch icon has transparent corners; iOS will fill them black").toBe(
    false,
  );
});

test("every icon this page links is part of the offline shell", async ({ page }) => {
  // The depot precaches the shell manifest's `publicFiles` list, maintained by hand in
  // vite.config.ts, so a new icon link and a new depot entry are two separate edits and
  // missing the second costs a reader their home-screen icon offline. The config is read
  // as text so this holds on a dev server too; the built-manifest side is covered in
  // app.spec.ts. og-image.png is deliberately not on that list — only remote crawlers
  // fetch it — so this walks the icon links and not the card tags.
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
