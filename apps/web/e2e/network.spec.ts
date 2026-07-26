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
