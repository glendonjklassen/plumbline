// Boot under a hostile network — the class of bug that shipped in v0.12.x:
// a reload that sat forever on "preparing your study tools…" because the
// service worker's network-first fetch had no timeout, on a device that
// already held every byte it needed.
//
// These tests do NOT use page.route(): Playwright's interception bypasses
// service workers entirely, so a route-based "stall" tests nothing at all
// (the first attempt at this repro passed while the bug was live). Instead a
// proxy origin forwards to the preview server and can hold a request open
// forever — a real stalled socket, below the browser.

import { expect, test, type Page } from "@playwright/test";
import http from "node:http";
import type { AddressInfo } from "node:net";

const UPSTREAM = "http://localhost:4173";

/** A forwarding origin whose `stall` predicate holds matching requests open
 *  (no response, no error) for as long as the test needs. */
function stallableOrigin(): Promise<{
  url: string;
  stall: (match: string | null) => void;
  close: () => Promise<void>;
}> {
  let stalled: string | null = null;
  const held: http.ServerResponse[] = [];
  const server = http.createServer((req, res) => {
    if (stalled && req.url?.includes(stalled)) {
      held.push(res); // never answered; released on close()
      return;
    }
    const up = http.request(
      UPSTREAM + req.url,
      { method: req.method, headers: { ...req.headers, host: "localhost:4173" } },
      (ur) => {
        res.writeHead(ur.statusCode ?? 502, ur.headers);
        ur.pipe(res);
      },
    );
    up.on("error", () => res.destroy());
    req.pipe(up);
  });
  return new Promise((resolve) => {
    server.listen(0, () => {
      const { port } = server.address() as AddressInfo;
      resolve({
        url: `http://localhost:${port}/`,
        stall: (match) => (stalled = match),
        close: () =>
          new Promise((done) => {
            for (const r of held) r.destroy();
            server.close(() => done());
          }),
      });
    });
  });
}

/** First visit through the given origin, first-run dismissed, reader up. */
async function firstVisit(page: Page, url: string): Promise<void> {
  await page.goto(url);
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  // The shell precache runs at the first idle after boot.
  await page.waitForTimeout(1_500);
}

test("a stalled network cannot hang the boot (service-worker timebox)", async ({ page }) => {
  const origin = await stallableOrigin();
  try {
    await firstVisit(page, origin.url);

    // The radio dozes / the network hands over mid-reconnect: the request is
    // accepted and then simply never answered. Everything needed is cached.
    origin.stall("pack/manifest.json");
    const t0 = Date.now();
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 45_000 });
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/);
    // The SW gives the network 3.5 s before serving its copy; the whole boot
    // should land well inside that plus a normal cached boot.
    expect(Date.now() - t0).toBeLessThan(20_000);
  } finally {
    await origin.close();
  }
});

test("a stalled navigation still reaches the reader (app shell from cache)", async ({ page }) => {
  // The document itself is network-first too — the same hang, one layer up.
  const origin = await stallableOrigin();
  try {
    await firstVisit(page, origin.url);
    origin.stall("/"); // the navigation AND everything else unversioned
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 45_000 });
  } finally {
    await origin.close();
  }
});

/** A forwarding origin that rewrites `pack/manifest.json` on the way past —
 *  `mutate` receives the parsed manifest and returns the one to serve. Everything
 *  else is proxied verbatim. This is how a "new deploy" is simulated without a
 *  second server: page.route() would bypass the service worker AND cannot see
 *  requests the engine worker makes at all. */
function rewritingOrigin(): Promise<{
  url: string;
  mutate: (f: ((m: any) => any) | null) => void;
  close: () => Promise<void>;
}> {
  let mutate: ((m: any) => any) | null = null;
  const server = http.createServer((req, res) => {
    const isManifest = req.url?.startsWith("/pack/manifest.json");
    // Identity encoding when we intend to REWRITE the body: vite preview
    // compresses JSON, and parsing gzip bytes as text fails silently — the
    // rewrite is skipped and the test appears to prove the opposite of what it
    // claims. (It did, for one run.)
    const headers = { ...req.headers, host: "localhost:4173" };
    if (isManifest) {
      // A body we intend to REWRITE has to arrive as plain, complete text.
      // Two things prevent that by default, and both made this fake silently
      // serve the ORIGINAL manifest while the test looked like it was testing
      // the rewrite:
      //   - vite preview gzips JSON, so parsing the bytes as text fails;
      //   - a reload sends If-None-Match, upstream answers 304 with NO body,
      //     and the browser then serves its own cached copy.
      headers["accept-encoding"] = "identity";
      delete headers["if-none-match"];
      delete headers["if-modified-since"];
    }
    const up = http.request(
      UPSTREAM + req.url,
      { method: req.method, headers },
      (ur) => {
        if (!(isManifest && mutate)) {
          res.writeHead(ur.statusCode ?? 502, ur.headers);
          ur.pipe(res);
          return;
        }
        const chunks: Buffer[] = [];
        ur.on("data", (c) => chunks.push(c));
        ur.on("end", () => {
          const upHeaders = { ...ur.headers };
          delete upHeaders["content-length"];
          delete upHeaders["content-encoding"];
          delete upHeaders["etag"];
          delete upHeaders["last-modified"];
          let body: string;
          try {
            body = JSON.stringify(mutate!(JSON.parse(Buffer.concat(chunks).toString("utf8"))));
          } catch {
            body = Buffer.concat(chunks).toString("utf8");
          }
          res.writeHead(ur.statusCode ?? 502, {
            ...upHeaders,
            "content-type": "application/json",
            // So the browser cannot answer a later request for this from its own
            // HTTP cache and hide the rewrite.
            "cache-control": "no-store",
          });
          res.end(body);
        });
      },
    );
    up.on("error", () => res.destroy());
    req.pipe(up);
  });
  return new Promise((resolve) => {
    server.listen(0, () => {
      const { port } = server.address() as AddressInfo;
      resolve({
        url: `http://localhost:${port}/`,
        mutate: (f) => (mutate = f),
        close: () => new Promise((done) => server.close(() => done())),
      });
    });
  });
}

test("a data update re-pins without re-downloading what did not change", async ({ page }) => {
  // The whole point of per-file content hashes. A release rotates the pack
  // version, but every file whose bytes are unchanged keeps its `?h=` URL — so the
  // reconciler should find them all already on the device and download NOTHING,
  // then re-pin to the new version.
  //
  // Before this, one whole-pack `?v=` stamp meant a version bump invalidated all
  // 44 URLs and every reader re-downloaded 10 MB for a release that might have
  // changed one weave.
  const origin = await rewritingOrigin();
  try {
    await firstVisit(page, origin.url);
    const before = await page.evaluate(async () => {
      const hit = await caches.match(new URL("__depot/pack-pin.json", location.href).href, {
        ignoreVary: true,
      });
      return (await hit!.json()).packVersion as string;
    });

    // "Deploy": same files, new version stamp. Nothing's content changed.
    origin.mutate((m) => ({ ...m, version: "beefbeefbeefbeef" }));

    const packRequests: string[] = [];
    page.on("request", (r) => {
      const u = new URL(r.url());
      if (u.pathname.includes("/pack/") && !u.pathname.endsWith("manifest.json")) {
        packRequests.push(u.pathname);
      }
    });
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

    // The reconciler runs after the reader is served — the poll waits it out
    // rather than guessing a duration.
    await expect
      .poll(
        async () =>
          page.evaluate(async () => {
            const hit = await caches.match(new URL("__depot/pack-pin.json", location.href).href, {
              ignoreVary: true,
            });
            return hit ? ((await hit.json()).packVersion as string) : null;
          }),
        { timeout: 90_000 },
      )
      .toBe("beefbeefbeefbeef");

    expect(before).not.toBe("beefbeefbeefbeef");
    expect(
      packRequests,
      "a version bump with no content change re-downloaded pack files — per-file hashing is not working",
    ).toEqual([]);
  } finally {
    await origin.close();
  }
});
