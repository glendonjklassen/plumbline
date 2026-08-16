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

// 127.0.0.1 BY NUMBER, both here and in `npm run preview`'s --host: this
// origin is Node dialling Node, and inside a container the two runtimes can
// resolve `localhost` to different address families — vite bound to one,
// http.request trying the other, and every proxied request answered with
// res.destroy() (net::ERR_EMPTY_RESPONSE). The browser is unaffected either
// way (its own happy-eyeballs falls back); only this Node-to-Node hop needs
// the family pinned.
const UPSTREAM = "http://127.0.0.1:4173";

/** A forwarding origin whose `stall` predicate holds matching requests open
 *  (no response, no error) for as long as the test needs.
 *
 *  `close()` also KILLS the origin mid-test — the port stops listening and every
 *  socket is dropped, so the next request is refused rather than answered. That is
 *  the only "offline" WebKit can be tested with (see the offline test below). */
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
            // closeAllConnections BEFORE close's callback is waited on: the
            // browser holds keep-alive sockets to this origin, and server.close()
            // resolves only once every connection has ended — so without this the
            // await simply never returns and the test's own cleanup is the hang.
            server.closeAllConnections();
            server.close(() => done());
          }),
      });
    });
  });
}

/** Reload and return how long it took to get text on screen. */
async function timedReload(page: Page): Promise<number> {
  const t0 = Date.now();
  // An explicit navigation timeout. Every test in this file gets 240 s
  // (test.setTimeout below), so a navigation that never resolves spends the whole
  // budget and is then reported as the TEST timing out, which names nothing. That
  // is exactly how the held-back first-run fix presented (TODO D-08, 2026-07-29):
  // this file went from 27 s and 3/3 to 4.3 min with one test hung out to the
  // timeout, and no line in the log pointed at a reload. 45 s matches the canvas
  // budget below, so a navigation that cannot land fails as itself.
  await page.reload({ timeout: 45_000 });
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 45_000 });
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/);
  return Date.now() - t0;
}

/** First visit through the given origin, first-run dismissed, reader up.
 *
 *  The analysis tiers are ticked on the way through: they became opt-in
 *  2026-07-28, and the pack-update tests below are about a device that HAS the
 *  analysis pack — without this they would be asserting over a pack the app
 *  correctly never downloaded. */
async function firstVisit(page: Page, url: string): Promise<void> {
  await page.goto(url);
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    for (const box of await page.locator(".dialog label.card input[type=checkbox]").all())
      if (!(await box.isChecked())) await box.check();
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
    //
    // Stalls `fonts.css`, NOT the pack manifest. The manifest was the original
    // repro, but it is no longer in the service worker's path (the depot owns
    // /pack/) and no longer on the boot path at all (the pin replaced it) — so
    // stalling it would prove nothing and this test would pass while the timebox
    // rotted. fonts.css is the remaining render-blocking unversioned file, and it
    // stalls exactly the same way.
    // MEASURE THIS MACHINE FIRST. The budget below is derived from an unstalled
    // reload rather than being a constant: a fixed millisecond ceiling passed
    // here for weeks and then failed inside a loaded full run, which is exactly
    // the trap CLAUDE.md records. Under load the baseline and the stalled boot
    // inflate together, so the DIFFERENCE between them is the stable quantity —
    // and the difference is what the timebox governs.
    const baseline = await timedReload(page);

    origin.stall("fonts.css");
    const stalled = await timedReload(page);

    // The stall may cost the 3.5 s timebox, and generous slack on top for the
    // extra paint. What it must NOT cost is forever — which is the bug, and which
    // the reload above would have failed on outright by never showing the canvas.
    const TIMEBOX = 3500;
    expect(
      stalled - baseline,
      `a stalled fonts.css cost ${stalled - baseline}ms over a ${baseline}ms baseline — ` +
        `the service worker's ${TIMEBOX}ms timebox is not bounding it`,
    ).toBeLessThan(TIMEBOX * 3);
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

test("boots offline after ONE visit — the whole promise of the thing", async ({ page }) => {
  // A first visit must leave the device self-sufficient: someone opens a shared
  // link once, then reads on a plane. The service worker cannot manage this alone
  // (it isn't controlling the page while the shell loads, and it claims the engine
  // worker mid-boot — a race the pack used to lose), so the page and the worker
  // stash their own downloads.
  //
  // OFFLINE HERE IS A DEAD ORIGIN, not context.setOffline(true), and that is not
  // a stylistic preference. Playwright's offline emulation makes WebKit stop
  // consulting the service worker at all: the reload dies with "WebKit
  // encountered an internal error" and a page fetch throws TypeError. It was
  // proven to be the harness and not us — a minimal cache-first service worker on
  // a throwaway origin fails identically there, while chromium serves it from
  // cache — so this test simply could not run on the one engine where the Cache
  // API, eviction and the storage budget actually differ, and on iOS is the only
  // engine there is. A closed port is what a plane does anyway: it sits below the
  // browser where no emulation can intervene, and the same WebKit device booted
  // to John 3 in 222 ms through one.
  const origin = await stallableOrigin();
  let dead = false;
  try {
    await firstVisit(page, origin.url);
    // The document can only come out of storage if the worker is CONTROLLING: on
    // a first visit it claims clients somewhere mid-boot, and a reload started
    // before that reaches the network with nothing in its path to answer for it.
    await expect
      .poll(async () => page.evaluate(() => !!navigator.serviceWorker.controller), { timeout: 30_000 })
      .toBe(true);
    // And the shell has to be COMPLETE before the network goes away. The precache
    // runs at the first idle after boot; polling it beats the 1.5 s sleep in
    // firstVisit, because a reload missing one lazily-imported bundle white-screens
    // for a reason that has nothing to do with what this test is about.
    await expect
      .poll(
        async () =>
          page.evaluate(async () => {
            const m = await (await fetch("shell-manifest.json")).json();
            const cache = await caches.open("plumbline-v1");
            for (const f of m.files)
              if (!(await cache.match(new URL(f, location.href).href, { ignoreVary: true }))) return f;
            return null;
          }),
        { timeout: 60_000, message: "the shell precache never finished, so going offline proves nothing yet" },
      )
      .toBeNull();

    // Kill it. From here every request is refused at the socket, so anything the
    // boot still needs from the network is a failure and not a slow pass.
    await origin.close();
    dead = true;
    await timedReload(page);
  } finally {
    if (!dead) await origin.close();
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

// Two full boots plus a background reconcile, so it needs more than the suite
// default: it passed alone and timed out inside a loaded full run, which is a
// flake, not a finding.
test.setTimeout(240_000);
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

test.setTimeout(240_000);
test("a deploy does not push the optional bundle onto a device that declined it", async ({ page }) => {
  // The `optional` stage exists so nobody downloads the 194 suggested weaves
  // without asking. The update sweep is where that promise is easiest to break:
  // `reconcilePack` walks the manifest and fetches whatever is missing, and a
  // file the reader deliberately does not have is missing on every device
  // forever. It would also have jammed the pin — the completeness gate refuses
  // to advance while a listed file is absent, so a device that never wanted the
  // bundle would have stopped re-pinning for good.
  //
  // Mutation-tested 2026-08-02: with `thisDevicesFiles` reduced to
  // `live.files`, this goes red on the download AND on the pin, which is the
  // pair of failures the fix addresses.
  const origin = await rewritingOrigin();
  try {
    await firstVisit(page, origin.url);
    // The pin LISTS the bundle (a warm boot rebuilds the manifest from it, and
    // Settings has to be able to offer a download) but gives it no url, which
    // is how it records "offered, not here". Both halves matter: without the
    // entry the row vanishes after the first visit, and with a url prune would
    // be told to keep bytes that were never fetched.
    const offered = await page.evaluate(async () => {
      const hit = await caches.match(new URL("__depot/pack-pin.json", location.href).href, {
        ignoreVary: true,
      });
      const pin = await hit!.json();
      const f = (pin.files as { path: string; url?: string }[]).find(
        (x) => x.path === "weaves/suggested.bundle.json",
      );
      return { listed: !!f, url: f?.url ?? null };
    });
    expect(offered.listed, "this pack has no optional bundle to decline").toBe(true);
    expect(offered.url, "the pin claims a bundle this device never downloaded").toBe(null);

    // A release that changes the version and nothing else.
    origin.mutate((m) => ({ ...m, version: "0pt10na1deploy00" }));

    const fetched: string[] = [];
    page.on("request", (r) => {
      const u = new URL(r.url());
      if (u.pathname.includes("/pack/")) fetched.push(u.pathname);
    });
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

    // The pin must still advance: an optional file nobody has is not a hole in
    // this device's pack, and treating it as one strands the reader on an old
    // pin release after release.
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
      .toBe("0pt10na1deploy00");

    expect(
      fetched.filter((p) => p.includes("suggested.bundle.json")),
      "the update sweep downloaded the optional bundle onto a device that never asked for it",
    ).toEqual([]);
  } finally {
    await origin.close();
  }
});

test.setTimeout(240_000);
test("a release that ADDS a file reaches the session that discovers it", async ({ page }) => {
  // How v0.39.0 shipped a hymnal nobody could open. On a warm boot the PIN is
  // this device's manifest, and the pin predates the release — so stage 2
  // fetches the study files the OLD manifest listed and never hears of the new
  // one. Every existing reader tapped the hymn tab and got "The hymnal has not
  // finished loading yet."
  //
  // The whole hymnal e2e suite passed throughout, because every one of its
  // tests boots FRESH, which is the one case that always worked. This test is
  // the upgrade, and it is the shape that was missing.
  //
  // Mutation-tested 2026-08-02: remove the `arrived` block from reconcilePack
  // and this goes red with the hymnal still empty after the update.
  const origin = await rewritingOrigin();
  try {
    // FIRST VISIT ON THE OLD RELEASE: a pack with no hymnal in it at all.
    origin.mutate((m) => ({
      ...m,
      version: "0ldrelease00000",
      files: (m.files as { path: string }[]).filter((f) => f.path !== "data/hymnal.json"),
    }));
    await firstVisit(page, origin.url);
    await expect
      .poll(
        () =>
          page.evaluate(async () => {
            const ix = await (window as any).__plumbline.rpc.call("hymnal");
            return (ix?.hymns ?? []).length;
          }),
        { timeout: 60_000 },
      )
      .toBe(0);

    // Age the pin's build id. The refresh is deliberately gated on "my code is
    // newer than my pin" — a warm boot on an UNCHANGED release must ask the
    // network for nothing at all — and one Playwright run only ever has a single
    // build, so the upgrade has to be staged here.
    await page.evaluate(async () => {
      const url = new URL("__depot/pack-pin.json", location.href).href;
      const hit = await caches.match(url, { ignoreVary: true });
      const pin = await hit!.json();
      pin.buildId = "an-older-build";
      const c = await caches.open("plumbline-v1");
      await c.put(url, new Response(JSON.stringify(pin), { headers: { "content-type": "application/json" } }));
    });

    // THE UPGRADE: the real manifest, hymnal and all. Identity rather than
    // clearing the hook — passthrough there works only by throwing into the
    // catch, which would swallow a genuine mistake in this test too.
    origin.mutate((m) => m);
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

    // Without leaving the page or reloading a second time, the book fills in —
    // reconcile downloads it, hands it to the engine, and the shell re-asks.
    await expect
      .poll(
        () =>
          page.evaluate(async () => {
            const ix = await (window as any).__plumbline.rpc.call("hymnal");
            return (ix?.hymns ?? []).length;
          }),
        { timeout: 90_000 },
      )
      .toBeGreaterThan(50);

    // And the reader sees it: the empty state is gone from the hymnal screen.
    await page.evaluate(() => ((window as any).__plumbline.screen = "hymnal"));
    await expect(page.locator(".row").first()).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("The hymnal has not finished loading yet.")).toHaveCount(0);
  } finally {
    await origin.close();
  }
});
