import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";

// The two entries that are not the happy first visit: a browser with JavaScript
// off (the same code path every text-only crawler takes), and an address the host
// does not have. GitHub Pages answers the second with `404.html`, which must
// forward to the app carrying the `?at=` / `?church=` payload (src/shell/church.ts)
// and the hash.
//
// The 404 tests run their own server because `vite preview` rewrites an unknown
// path to index.html, so under the suite's normal server `404.html` is never
// served. The stub answers an unknown path with `404.html` and a 404 status, from
// the exact `public/` bytes that ship. That covers the rule; "lands on the real
// app" covers the destination by asking the suite's own server for `404.html` by
// name, which every host that serves it will answer.
const PUBLIC_404 = new URL("../public/404.html", import.meta.url);
const CNAME = new URL("../public/CNAME", import.meta.url);
const INDEX_HTML = new URL("../index.html", import.meta.url);

/** A marker only the stub root serves, so "we landed" is a visible document
 *  rather than a URL string that changed. */
const ROOT_MARKER = "plumbline-stub-root";

/** GitHub Pages, reduced to the behaviour under test. `rootExists: false` is the
 *  pathological host whose `/` is missing too — the only shape that can make
 *  `404.html` redirect to itself. */
async function pagesHost(opts: { rootExists: boolean }): Promise<{
  base: string;
  hits: (path: string) => number;
  close: () => Promise<void>;
}> {
  const body404 = readFileSync(PUBLIC_404);
  const counts = new Map<string, number>();
  const server: Server = createServer((req, res) => {
    const path = new URL(req.url ?? "/", "http://x").pathname;
    counts.set(path, (counts.get(path) ?? 0) + 1);
    // No caching: the loop test counts requests, and a cached 404 would hide the loop.
    res.setHeader("cache-control", "no-store");
    if (path === "/" && opts.rootExists) {
      res.writeHead(200, { "content-type": "text/html" });
      res.end(`<!doctype html><title>root</title><p id="${ROOT_MARKER}">the app</p>`);
      return;
    }
    res.writeHead(404, { "content-type": "text/html" });
    res.end(body404);
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const { port } = server.address() as AddressInfo;
  return {
    base: `http://127.0.0.1:${port}`,
    hits: (path) => counts.get(path) ?? 0,
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}

test.describe("with JavaScript switched off", () => {
  test.use({ javaScriptEnabled: false });

  test("the page says what Plumbline is and that it needs JavaScript", async ({ page }) => {
    await page.goto("/");

    // Nothing mounted, or the assertions below could pass on the running app's text.
    await expect(page.locator("#app")).toBeEmpty();

    await expect(page.getByRole("heading", { name: "Plumbline" })).toBeVisible();
    await expect(page.getByText(/1769 King James Version/)).toBeVisible();
    await expect(page.getByText(/needs JavaScript/i)).toBeVisible();
    await expect(page.getByText(/Switch JavaScript on for this site and reload/i)).toBeVisible();

    // The block carries its own paper colour: the bundle's stylesheet did not run either.
    await expect(page.locator("body")).toHaveCSS("background-color", "rgb(252, 249, 244)");
  });
});

test("the JavaScript-off page cannot leak into the running app", async ({ page }) => {
  // With scripting enabled a browser does not parse a noscript element's contents,
  // so none of it becomes DOM. That is what lets its <style> restyle `body` and hide
  // `#app`; if it ever did parse, `#app { display: none }` would blank the app.
  await page.goto("/");
  await expect(page.locator("noscript")).toHaveCount(1);
  await expect(page.locator(".noscript-card")).toHaveCount(0);
  await expect(page.locator("#app")).toBeVisible();
});

test.describe("an address the host does not have", () => {
  test("forwards to the app carrying BOTH the search string and the hash", async ({ page }) => {
    const host = await pagesHost({ rootExists: true });
    try {
      // Everything a shared link or a printed QR can carry (church.ts), plus a hash.
      const search = "?at=Ps%2023%3A1&church=Grace%20Chapel&churchService=600&start=new";
      const hash = "#/John/3";
      await page.goto(`${host.base}/an/old/path/${search}${hash}`);

      // A document, not just a URL that moved.
      await expect(page.locator(`#${ROOT_MARKER}`)).toBeVisible();

      const landed = new URL(page.url());
      expect(landed.pathname).toBe("/");
      expect(landed.search).toBe(search);
      expect(landed.hash).toBe(hash);
      // And it still decodes to the form church.ts reads.
      expect(landed.searchParams.get("at")).toBe("Ps 23:1");
      expect(landed.searchParams.get("church")).toBe("Grace Chapel");
      expect(landed.searchParams.get("start")).toBe("new");
    } finally {
      await host.close();
    }
  });

  test("forwards a search string with no hash", async ({ page }) => {
    const host = await pagesHost({ rootExists: true });
    try {
      await page.goto(`${host.base}/plumbline/?at=Ps%2023%3A1`);
      await expect(page.locator(`#${ROOT_MARKER}`)).toBeVisible();
      const landed = new URL(page.url());
      expect(landed.pathname).toBe("/");
      expect(landed.searchParams.get("at")).toBe("Ps 23:1");
      expect(landed.hash).toBe("");
    } finally {
      await host.close();
    }
  });

  test("forwards a hash with no search string", async ({ page }) => {
    const host = await pagesHost({ rootExists: true });
    try {
      await page.goto(`${host.base}/plumbline/#/Rom/8`);
      await expect(page.locator(`#${ROOT_MARKER}`)).toBeVisible();
      const landed = new URL(page.url());
      expect(landed.pathname).toBe("/");
      expect(landed.search).toBe("");
      expect(landed.hash).toBe("#/Rom/8");
    } finally {
      await host.close();
    }
  });

  test("lands on the real app, not just a URL that moved", async ({ page }) => {
    // The destination rather than the rule: `404.html` asked for by name on the
    // suite's own server, since an unknown path here would be rewritten to index.html.
    await page
      .goto("./404.html?at=Ps%2023%3A1#/John/3", { waitUntil: "commit" })
      .catch(() => {}); // the client-side forward interrupts this navigation
    await page.waitForURL((u) => !u.pathname.endsWith("404.html"), { timeout: 15_000 });
    const landed = new URL(page.url());
    expect(landed.searchParams.get("at")).toBe("Ps 23:1");
    expect(landed.hash).toBe("#/John/3");
    await expect(page.locator("#app")).not.toBeEmpty();
  });

  test("cannot loop when the app root is the thing that is missing", async ({ page }) => {
    // The only loop this page can have: it forwards to `/`, so a host that 404s
    // `/` too would send it straight back to itself.
    const host = await pagesHost({ rootExists: false });
    try {
      await page.goto(`${host.base}/`, { waitUntil: "domcontentloaded" }).catch(() => {});
      // Counted at the server, so it survives the page context being torn down by the
      // loop itself. The forward is synchronous in <head>, so by domcontentloaded it
      // has either happened or been refused.
      expect(
        host.hits("/"),
        "404.html forwarded to the app root while already ON the app root — that is the loop",
      ).toBe(1);
      await expect(page.getByText("Nothing is at this address.")).toBeVisible();
    } finally {
      await host.close();
    }
  });

  test("hides itself before leaving, and decides before the body exists", () => {
    // No-flash is structural rather than a timing measurement: the forward runs while
    // the parser is in <head>, and the body is hidden the moment it commits.
    const html = readFileSync(PUBLIC_404, "utf8");
    const script = html.indexOf("location.replace(");
    const bodyOpen = html.indexOf("<body");
    expect(script).toBeGreaterThan(-1);
    expect(script).toBeLessThan(bodyOpen);
    expect(html).toContain('document.documentElement.className = "leaving"');
    expect(html.replace(/\s+/g, " ")).toContain("html.leaving > body { display: none; }");
    // `replace`, not `assign`: Back must go where the reader came from.
    expect(html).not.toContain("location.assign(");
    expect(html).not.toContain("location.href =");
  });
});

test("public/CNAME names the same origin the app tells the world about", () => {
  // This file restores the Pages setting if it is ever lost, so a typo'd domain here
  // would replace a working one at the next deploy. It must agree with the link card.
  const domain = readFileSync(CNAME, "utf8").trim();
  expect(domain).toBe("plumblinebible.org");
  expect(domain.split("\n")).toHaveLength(1);
  const ogUrl = /property="og:url" content="([^"]+)"/.exec(readFileSync(INDEX_HTML, "utf8"))?.[1];
  expect(new URL(ogUrl!).host).toBe(domain);
});
