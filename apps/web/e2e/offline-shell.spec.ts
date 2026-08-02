import { expect, test, type Page } from "@playwright/test";
import http from "node:http";
import type { AddressInfo } from "node:net";

// THE WHITE SCREEN ON A PLANE (reported 2026-07-31).
//
// "When I'm on airplane mode and I open the PWA it just shows a white screen."
//
// `network.spec.ts` already boots through a dead origin after one visit and
// passes. What nothing covered is the state a DEPLOY leaves behind, which is the
// state a reader who has updated at least once is actually in.
//
// THE MECHANISM, and it needs no bad luck:
//
//   1. Build A. `precache.ts` stores the document under BOTH keys a navigation
//      can land on — the bare base `./` and `./index.html` — plus A's assets.
//   2. Build B deploys and the reader opens the app online. `sw.js` served
//      navigations network-first and cached the response, so `./index.html`
//      became document B. `precache.ts` could not follow: every URL was guarded
//      by `if (await depotHas(url)) return`, so the bare-base key kept
//      DOCUMENT A FOREVER. `pruneToPin` then kept only the current build's shell
//      and DELETED A's bundle.
//   3. Airplane mode. An installed PWA opens `start_url: "./"` — the bare-base
//      key — and is served document A, which asks for a bundle that has been
//      reclaimed. Nothing mounts: no splash, no error, nothing to tap.
//
// So the two document keys could hold DIFFERENT BUILDS at once while the assets
// were pruned to a third opinion. That inconsistency IS the bug, and it is what
// this file asserts against, because the end-to-end symptom is not reliably
// reproducible on chromium: the stale bare-base entry happens to rescue the
// reload, and `vite preview` answers a missing bundle with index.html at status
// 200 rather than 404. Both accidents hid this fault while it was live, and a
// test that only watched for a blank page passed against the bug.
//
// The fix makes the shell single-writer and atomic: `sw.js` refuses the document
// entirely (refusal 3 in `mayCache`), and `precache.ts` writes it LAST, from one
// response into both keys, only once every other shell file is confirmed present.

const UPSTREAM = "http://localhost:4173";

/** A proxy to the preview server that can stand in front of it as a NEW DEPLOY.
 *
 *  `deploy(from, to)` renames one bundle across everything that names it — the
 *  document, both of its keys, and `shell-manifest.json` — and serves the renamed
 *  asset from the real bytes upstream. That coherence is the point: a deploy that
 *  moved only the document would leave `pruneToPin` still keeping the old build,
 *  and the very fault this file exists for (a stale document whose assets have
 *  been reclaimed) could not arise. An earlier version of this helper did exactly
 *  that, and the test passed against the bug. */
function deployableOrigin(): Promise<{
  url: string;
  deploy: (renames: Array<[from: string, to: string]>) => void;
  /** 404 anything whose path contains this — one shell file the deploy cannot
   *  deliver, which is what makes a shell INCOMPLETE rather than merely new. */
  refuse: (match: string | null) => void;
  close: () => Promise<void>;
}> {
  let renames: Array<[string, string]> = [];
  let refused: string | null = null;

  const fetchUpstream = (path: string): Promise<{ status: number; headers: http.IncomingHttpHeaders; body: Buffer }> =>
    new Promise((resolve, reject) => {
      const up = http.request(UPSTREAM + path, { headers: { host: "localhost:4173" } }, (ur) => {
        const chunks: Buffer[] = [];
        ur.on("data", (c) => chunks.push(c));
        ur.on("end", () => resolve({ status: ur.statusCode ?? 502, headers: ur.headers, body: Buffer.concat(chunks) }));
      });
      up.on("error", reject);
      up.end();
    });

  const server = http.createServer(async (req, res) => {
    const path = (req.url ?? "/").split("?")[0];
    if (refused && path.includes(refused)) {
      res.writeHead(404).end();
      return;
    }
    try {
      if (renames.length) {
        // A renamed file: real bytes, new name. A coherent build, not a ghost.
        const hit = renames.find(([, t]) => path === `/${t}`);
        if (hit) {
          const real = await fetchUpstream(`/${hit[0]}`);
          res.writeHead(real.status, { "content-type": real.headers["content-type"] ?? "application/octet-stream" });
          res.end(real.body);
          return;
        }
        // The document (both keys) and the shell manifest, rewritten.
        if (path === "/" || path === "/index.html" || path === "/shell-manifest.json") {
          const real = await fetchUpstream(path === "/" ? "/index.html" : path);
          let body = real.body.toString("utf8");
          for (const [f, t] of renames) body = body.split(f).join(t);
          if (path === "/shell-manifest.json") body = body.replace(/"buildId":\s*"[^"]*"/, '"buildId": "deployB"');
          res.writeHead(200, {
            "content-type": path.endsWith(".json") ? "application/json" : "text/html; charset=utf-8",
          });
          res.end(body);
          return;
        }
      }
    } catch {
      res.writeHead(502).end();
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
        deploy: (r) => (renames = r),
        refuse: (m) => (refused = m),
        close: () =>
          new Promise((done) => {
            server.closeAllConnections();
            server.close(() => done());
          }),
      });
    });
  });
}

/** First visit, reader up, shell fully stored — a device that is legitimately
 *  offline-capable BEFORE the deploy below lands. */
async function firstVisit(page: Page, url: string): Promise<void> {
  await page.goto(url);
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  await expect
    .poll(() => page.evaluate(() => !!navigator.serviceWorker.controller), { timeout: 30_000 })
    .toBe(true);
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          const m = await (await fetch("shell-manifest.json")).json();
          const cache = await caches.open("plumbline-v1");
          for (const f of m.files)
            if (!(await cache.match(new URL(f, location.href).href, { ignoreVary: true }))) return f;
          return null;
        }),
      { timeout: 60_000, message: "the shell was never fully stored, so the deploy below proves nothing" },
    )
    .toBe(null);
}

/** The stored document under each key a navigation can land on, the scripts each
 *  one names, and which of those cannot be served. Read through the page, so it
 *  is the real Cache API and not a model of it. */
async function storedShell(page: Page) {
  return page.evaluate(async () => {
    const cache = await caches.open("plumbline-v1");
    const scriptsOf = async (rel: string) => {
      const hit = await cache.match(new URL(rel, location.href).href, { ignoreVary: true });
      if (!hit) return null;
      const html = await hit.text();
      return [...html.matchAll(/<script[^>]+src="([^"]+)"/g)].map((m) => m[1]);
    };
    const base = await scriptsOf("./");
    const index = await scriptsOf("./index.html");
    const missing: string[] = [];
    for (const src of [...new Set([...(base ?? []), ...(index ?? [])])]) {
      const hit = await cache.match(new URL(src, location.href).href, { ignoreVary: true });
      if (!hit || !hit.ok) missing.push(src);
    }
    return { base, index, missing };
  });
}

test.setTimeout(240_000);

// MUTATION 1 — the service worker's half. In `sw.js`, restore
// `&& req.mode !== "navigate"` on the shell-path refusal in `mayCache`. RED:
// "the two stored documents disagree" — `./index.html` becomes the ghost build
// while `./` keeps the original.
//
// MUTATION 2 — the precache's half. In `precache.ts`, put the document back
// inside the asset loop (or guard its write with `depotHas`). RED: the bare-base
// key never advances to the new build, which is the half that made the breakage
// permanent instead of transient.
test("a deploy cannot leave the two stored documents disagreeing", async ({ page }) => {
  const origin = await deployableOrigin();
  let dead = false;
  try {
    await firstVisit(page, origin.url);
    const before = await storedShell(page);
    expect(before.base, "the bare-base key was never stored, so this proves nothing").not.toBeNull();
    expect(before.index, "the index.html key was never stored, so this proves nothing").not.toBeNull();

    // THE DEPLOY. A coherent build B: the main bundle is renamed everywhere that
    // names it, so `shell-manifest.json` moves with the document and `pruneToPin`
    // starts keeping B and reclaiming A. That is what makes a stale document
    // dangerous rather than merely old.
    const bundle = await page.evaluate(async () => {
      const m = await (await fetch("shell-manifest.json")).json();
      return m.files.find((f: string) => /^assets\/index-.*\.js$/.test(f)) as string;
    });
    expect(bundle, "no main bundle in the shell manifest — this test needs updating").toBeTruthy();
    origin.deploy([[bundle, "assets/index-DEPLOYB.js"]]);

    // The reader opens the app once, online. This is the entire window.
    await page.goto(origin.url).catch(() => {});
    await page.waitForTimeout(1_500);

    const after = await storedShell(page);

    // THE INVARIANT. Whatever the two keys hold, they must hold the SAME thing.
    // An installed PWA opens `./` and a browser tab opens `./index.html`; a
    // reader must not get a different app depending on which.
    expect(
      after.index,
      `the two stored documents disagree — "./" names ${JSON.stringify(after.base)} while ` +
        `"./index.html" names ${JSON.stringify(after.index)}. An installed PWA opens "./", so ` +
        `whichever of the two is stale is the one the reader gets on a plane.`,
    ).toEqual(after.base);

    // ...and neither may name a script that cannot be served. That is the white
    // screen itself: document from cache, bundle absent, nothing mounts.
    expect(
      after.missing,
      `the stored shell names scripts that are not cached, which offline is a blank page: ${JSON.stringify(after.missing)}`,
    ).toEqual([]);

    // Then the consequence rather than the bookkeeping: on a plane, text.
    await origin.close();
    dead = true;
    await page.reload({ timeout: 45_000 });
    await expect(
      page.locator(".pane canvas").first(),
      "offline after a deploy the reader half-received, the app never reached the text",
    ).toBeVisible({ timeout: 45_000 });
  } finally {
    if (!dead) await origin.close();
  }
});

// THE WINDOW. The first test lets the precache finish, which is the happy path of
// a deploy. This one is the reader who opens the app after a release and puts the
// phone away seconds later — before the idle precache has run.
//
// MUTATION: in `sw.js`, restore `&& req.mode !== "navigate"` on the shell-path
// refusal in `mayCache`. RED: "the two stored documents disagree" — the service
// worker has written the new document under `./index.html` while `./` still holds
// the old one, and neither is paired with a full set of assets.
test("a deploy caught before the precache runs cannot split the two keys", async ({ page }) => {
  const origin = await deployableOrigin();
  let dead = false;
  try {
    await firstVisit(page, origin.url);
    const bundle = await page.evaluate(async () => {
      const m = await (await fetch("shell-manifest.json")).json();
      return m.files.find((f: string) => /^assets\/index-.*\.js$/.test(f)) as string;
    });
    origin.deploy([[bundle, "assets/index-WINDOW.js"]]);

    // Open it once and go straight offline: no waiting for idle work.
    await page.goto(origin.url).catch(() => {});
    await origin.close();
    dead = true;

    const after = await storedShell(page);
    expect(
      after.index,
      `the two stored documents disagree after a deploy the reader did not stay for — ` +
        `"./" names ${JSON.stringify(after.base)} and "./index.html" names ${JSON.stringify(after.index)}. ` +
        `An installed PWA opens "./" and a tab opens "./index.html", so one of them is a blank page.`,
    ).toEqual(after.base);
    expect(
      after.missing,
      `the stored shell names scripts that are not cached: ${JSON.stringify(after.missing)}`,
    ).toEqual([]);

    await page.reload({ timeout: 45_000 });
    await expect(
      page.locator(".pane canvas").first(),
      "offline after a deploy the reader did not stay for, the app never reached the text",
    ).toBeVisible({ timeout: 45_000 });
  } finally {
    if (!dead) await origin.close();
  }
});

// A DEPLOY THIS DEVICE CANNOT FULLY RECEIVE — a flaky asset, a CDN mid-rollout.
// The shell is incomplete, so the right answer is to change nothing: keep the old
// document AND the old assets, and reclaim on some later launch that succeeds.
//
// MUTATION: in `precache.ts`, change `if (!complete || !manifest) return [];` to
// return `manifest?.files ?? []`. RED: the prune runs against the NEW build's
// keep-set while the stored document is still the old one, so the bundle that
// document names is deleted — "the stored shell names scripts that are not
// cached". That is strictly worse than the original bug, and it is the state this
// fix produced before promotion and reclamation were made one decision.
test("a deploy this device cannot fully receive changes nothing", async ({ page }) => {
  const origin = await deployableOrigin();
  let dead = false;
  try {
    await firstVisit(page, origin.url);
    const bundle = await page.evaluate(async () => {
      const m = await (await fetch("shell-manifest.json")).json();
      return m.files.find((f: string) => /^assets\/index-.*\.js$/.test(f)) as string;
    });
    // Build B renames the bundle AND an icon. The icon is the file this device
    // cannot get: the page boots perfectly without it, so the precache still
    // runs, but the SHELL is genuinely incomplete.
    //
    // Two earlier attempts proved nothing and are worth recording. Refusing the
    // main bundle stops the new document booting at all, so `precacheShell` never
    // runs. And refusing a file whose NAME did not change (a font) is not a
    // failure either — build A already cached it, so the shell really is complete.
    // It has to be a file that is new in B and unobtainable.
    origin.deploy([
      [bundle, "assets/index-PARTIAL.js"],
      ["icon-192.png", "icon-192-PARTIAL.png"],
    ]);
    origin.refuse("icon-192-PARTIAL.png");

    await page.goto(origin.url).catch(() => {});
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
    await page.waitForTimeout(2_500);

    const after = await storedShell(page);
    expect(after.index, "the two stored documents disagree").toEqual(after.base);
    // NOT PROMOTED. The new document must not become the offline shell while a
    // file it needs is unobtainable, so the stored document is still the old one.
    expect(
      after.base?.join(),
      "an incomplete deploy was promoted to the offline shell anyway, so the stored " +
        "document now depends on a file this device could not get",
    ).toContain(bundle.replace("assets/", ""));
    // ...and nothing was reclaimed underneath it.
    expect(
      after.missing,
      `an unreceivable deploy reclaimed assets the stored document still needs: ${JSON.stringify(after.missing)}`,
    ).toEqual([]);

    await origin.close();
    dead = true;
    await page.reload({ timeout: 45_000 });
    await expect(
      page.locator(".pane canvas").first(),
      "offline after a deploy that could not be received, the app never reached the text",
    ).toBeVisible({ timeout: 45_000 });
  } finally {
    if (!dead) await origin.close();
  }
});
