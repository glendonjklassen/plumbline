import { expect, test, type Page } from "@playwright/test";
import http from "node:http";
import type { AddressInfo } from "node:net";

// The white screen on a plane: an installed PWA opened offline after a deploy mounted nothing.
//
// `precache.ts` stores the document under both keys a navigation can land on, the bare base `./`
// and `./index.html`. `sw.js` served navigations network-first and cached them, so after a deploy
// `./index.html` held build B while precache's `if (await depotHas(url)) return` guard kept the
// bare-base key on build A forever; `pruneToPin` then deleted A's bundle. An installed PWA opens
// `start_url: "./"`, so offline it got document A naming a bundle that had been reclaimed.
//
// These assert the inconsistency — two document keys on different builds, assets pruned to a third
// opinion — not the blank page, because the symptom is not reliably reproducible: the stale
// bare-base entry can rescue the reload, and `vite preview` answers a missing bundle with
// index.html at 200 rather than 404. A test that only watched for a blank page passed against the
// live bug.
//
// The fix makes the shell single-writer and atomic: `sw.js` refuses the document (refusal 3 in
// `mayCache`) and `precache.ts` writes it last, from one response into both keys, only once every
// other shell file is confirmed present.

// 127.0.0.1 by number, here and in `npm run preview`'s --host: this hop is Node dialling Node, and
// in a container the two runtimes can resolve `localhost` to different address families, leaving
// every proxied request answered with res.destroy(). The browser's own fallback hides it.
const UPSTREAM = "http://127.0.0.1:4173";

/** A proxy to the preview server that can stand in front of it as a new deploy.
 *
 *  `deploy(from, to)` renames one bundle across everything that names it — the document, both of
 *  its keys, and `shell-manifest.json` — serving it from the real bytes upstream. The coherence is
 *  required: a deploy that moved only the document would leave `pruneToPin` keeping the old build,
 *  so a stale document whose assets have been reclaimed could not arise at all. */
function deployableOrigin(): Promise<{
  url: string;
  deploy: (renames: Array<[from: string, to: string]>) => void;
  /** 404 anything whose path contains this: one shell file the deploy cannot deliver, which makes
   *  the shell incomplete rather than merely new. */
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
        // A renamed file: real bytes under the new name, so the build is coherent.
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

/** First visit, reader up, shell fully stored: a device legitimately offline-capable before the
 *  deploy below lands. */
async function firstVisit(page: Page, url: string): Promise<void> {
  await page.goto(url);
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

/** The stored document under each key a navigation can land on, the scripts each names, and which
 *  cannot be served. Read through the page, so it is the real Cache API and not a model of it. */
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

// Fails against either half of the bug: with `req.mode !== "navigate"` back on sw.js's shell-path
// refusal, `./index.html` becomes the new build while `./` keeps the old one; with precache.ts's
// document write back inside the depotHas-guarded asset loop, the bare-base key never advances at
// all, which is what made the breakage permanent rather than transient.
test("a deploy cannot leave the two stored documents disagreeing", async ({ page }) => {
  const origin = await deployableOrigin();
  let dead = false;
  try {
    await firstVisit(page, origin.url);
    const before = await storedShell(page);
    expect(before.base, "the bare-base key was never stored, so this proves nothing").not.toBeNull();
    expect(before.index, "the index.html key was never stored, so this proves nothing").not.toBeNull();

    // A coherent build B: the bundle is renamed everywhere, so `shell-manifest.json` moves with
    // the document and `pruneToPin` starts keeping B and reclaiming A — which is what makes a
    // stale document dangerous rather than merely old.
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

    // The invariant: an installed PWA opens `./` and a tab opens `./index.html`, so whatever the
    // two keys hold they must hold the same thing.
    expect(
      after.index,
      `the two stored documents disagree — "./" names ${JSON.stringify(after.base)} while ` +
        `"./index.html" names ${JSON.stringify(after.index)}. An installed PWA opens "./", so ` +
        `whichever of the two is stale is the one the reader gets on a plane.`,
    ).toEqual(after.base);

    // Neither may name a script that cannot be served: document from cache, bundle absent, nothing
    // mounts — the white screen itself.
    expect(
      after.missing,
      `the stored shell names scripts that are not cached, which offline is a blank page: ${JSON.stringify(after.missing)}`,
    ).toEqual([]);

    // The consequence rather than the bookkeeping: on a plane, text.
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

// The narrow window the first test does not cover: a reader who opens the app after a release and
// puts the phone away before the idle precache has run. Fails against the bug — with
// `req.mode !== "navigate"` back on sw.js's shell-path refusal the worker has written the new
// document under `./index.html` while `./` still holds the old one, neither with a full asset set.
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

// A deploy this device cannot fully receive (a flaky asset, a CDN mid-rollout). The shell is
// incomplete, so the right answer is to change nothing and reclaim on a later launch that
// succeeds. Fails against the bug: if `precache.ts`'s `if (!complete || !manifest) return [];`
// returns the new manifest's files instead, the prune runs against the new keep-set while the
// stored document is still the old one, deleting the bundle that document names.
test("a deploy this device cannot fully receive changes nothing", async ({ page }) => {
  const origin = await deployableOrigin();
  let dead = false;
  try {
    await firstVisit(page, origin.url);
    const bundle = await page.evaluate(async () => {
      const m = await (await fetch("shell-manifest.json")).json();
      return m.files.find((f: string) => /^assets\/index-.*\.js$/.test(f)) as string;
    });
    // Build B renames the bundle and an icon; the icon is the file this device cannot get. It has
    // to be a file that is both new in B and unobtainable: refusing the main bundle stops the new
    // document booting so `precacheShell` never runs, and refusing a file whose name did not
    // change leaves the shell genuinely complete from build A's cache.
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
    // Not promoted: the new document must not become the offline shell while a file it needs is
    // unobtainable, so the stored document is still the old one.
    expect(
      after.base?.join(),
      "an incomplete deploy was promoted to the offline shell anyway, so the stored " +
        "document now depends on a file this device could not get",
    ).toContain(bundle.replace("assets/", ""));
    // And nothing was reclaimed underneath it.
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
