import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";

// The two ways into the app that are NOT "the happy first visit", and both used
// to be dead ends:
//
//   1. A browser with JavaScript switched off — and, the same code path, every
//      text-only crawler. It got `<div id="app"></div>` on cream paper and
//      nothing else, for ever, which reads as a broken site rather than an
//      unmet requirement.
//   2. Any address the host does not have. GitHub Pages answers those with
//      `404.html`, so a mistyped path, or an old link that still carries a path
//      segment, ended at a page that went nowhere — taking the `?at=` /
//      `?church=` payload (src/shell/church.ts) and the hash with it.
//
// Written 2026-07-29 for the pre-release audit item D.
//
// WHY THE 404 TESTS RUN THEIR OWN SERVER. `vite preview` is a single-page host:
// it rewrites an unknown path to index.html, so under the suite's normal server
// `404.html` is never served and the redirect it exists for is never exercised.
// The server below does the one thing GitHub Pages does that matters here —
// answer an unknown path with `404.html` and a 404 status — and it serves the
// exact bytes from `public/`, which is what ships (Vite copies public/ verbatim
// — verified against a build). That covers the RULE; "lands on the real app"
// below covers the DESTINATION, by asking the suite's own server for `404.html`
// by name, which every host that serves it will answer.
const PUBLIC_404 = new URL("../public/404.html", import.meta.url);
const CNAME = new URL("../public/CNAME", import.meta.url);
const INDEX_HTML = new URL("../index.html", import.meta.url);

/** A marker only the stub root serves, so "we landed" is a document we can see
 *  rather than a URL string that changed. */
const ROOT_MARKER = "plumbline-stub-root";

/** GitHub Pages, reduced to the behaviour under test.
 *
 *  `rootExists: false` is the pathological host — even `/` is missing — which is
 *  the only shape that can make this page redirect to itself. */
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
    // No caching, so a repeat request is a repeat REQUEST — the loop test counts
    // them and a cached 404 would hide the loop it is looking for.
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

    // The premise: nothing mounted. Without this the test could pass on the
    // running app's own text.
    await expect(page.locator("#app")).toBeEmpty();

    // What it IS, first — this is also the sentence a text-only crawler reads.
    await expect(page.getByRole("heading", { name: "Plumbline" })).toBeVisible();
    await expect(page.getByText(/1769 King James Version/)).toBeVisible();
    // ...then the requirement, named outright.
    await expect(page.getByText(/needs JavaScript/i)).toBeVisible();
    await expect(page.getByText(/Switch JavaScript on for this site and reload/i)).toBeVisible();

    // Styled enough not to look broken: the block carries its own paper colour,
    // because the bundle's stylesheet is part of what did not run.
    await expect(page.locator("body")).toHaveCSS("background-color", "rgb(252, 249, 244)");
  });
});

test("the JavaScript-off page cannot leak into the running app", async ({ page }) => {
  // The reason its <style> is allowed to restyle `body` and hide `#app` at all:
  // with scripting ENABLED a browser does not parse a noscript element's
  // contents — they are inert text. So none of it becomes DOM, which means it
  // cannot reach the palette, the accessibility tree (the hidden scripture
  // mirror is the only text there), or Ctrl+F. Asserted rather than assumed,
  // because if it ever DID parse, `#app { display: none }` would blank the app.
  await page.goto("/");
  await expect(page.locator("noscript")).toHaveCount(1);
  await expect(page.locator(".noscript-card")).toHaveCount(0);
  await expect(page.locator("#app")).toBeVisible();
});

test.describe("an address the host does not have", () => {
  test("forwards to the app carrying BOTH the search string and the hash", async ({ page }) => {
    const host = await pagesHost({ rootExists: true });
    try {
      // Everything a shared link or a printed QR can carry (church.ts) plus a
      // hash of the kind the reader mirrors into the URL.
      const search = "?at=Ps%2023%3A1&church=Grace%20Chapel&churchInfo=Sundays%2010%20am&start=new";
      const hash = "#/John/3";
      await page.goto(`${host.base}/an/old/path/${search}${hash}`);

      // A document, not just a URL that moved.
      await expect(page.locator(`#${ROOT_MARKER}`)).toBeVisible();

      const landed = new URL(page.url());
      expect(landed.pathname).toBe("/");
      expect(landed.search).toBe(search);
      expect(landed.hash).toBe(hash);
      // And it still MEANS what it meant — this is the form church.ts reads.
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
    // The tests above prove the forwarding rule against a host that behaves like
    // Pages. This one proves the DESTINATION: `404.html` is fetchable by name on
    // every host that serves it, so ask for it on the suite's own server and see
    // the app boot with the payload intact. (An unknown PATH cannot be used here
    // — `vite preview` rewrites those to index.html, which is why the rule needs
    // its own host above.)
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
    // The only redirect loop this page can have: it forwards to `/`, so a host
    // that 404s `/` too would send it straight back to itself.
    const host = await pagesHost({ rootExists: false });
    try {
      await page.goto(`${host.base}/`, { waitUntil: "domcontentloaded" }).catch(() => {});
      // Counted at the SERVER, so it survives the page context being torn down
      // by the very loop it is looking for. The forward is synchronous in
      // <head>, so by domcontentloaded it has either happened or been refused.
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
    // The no-flash guarantee is structural, not a timing measurement: the
    // forward runs while the parser is still in <head>, and the body is hidden
    // the moment it commits, in case the parser gets there first.
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
  // Belt-and-braces against the Pages setting being lost — which is only true
  // if the file agrees with the address in the link card and in church.ts. A
  // typo'd domain here would REPLACE a working setting at the next deploy.
  const domain = readFileSync(CNAME, "utf8").trim();
  expect(domain).toBe("plumblinebible.org");
  expect(domain.split("\n")).toHaveLength(1);
  const ogUrl = /property="og:url" content="([^"]+)"/.exec(readFileSync(INDEX_HTML, "utf8"))?.[1];
  expect(new URL(ogUrl!).host).toBe(domain);
});
