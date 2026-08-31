// Boot overlap: the engine binary, the reader faces and the scripture text download at the same
// time. `boot()` starts the engine fetch un-awaited at the top and collects it at the instantiate
// site; the worker starts `loadFonts` un-awaited and collects it just before the boot reply. The
// bug is those awaits going back in order — the ~1.7 MB engine before the text, both faces before
// the boot starts — which on a slow connection is dead time before first text.
//
// These assert the overlap (one download had started before another finished), never a wall-clock
// budget: a millisecond ceiling would be a constant a serialised boot can still fit inside, while
// the ordering separates fixed from broken however fast the machine is. `CORPUS_HOLD_MS` and
// `FONT_HOLD_MS` are fault injection, not budgets — nothing asserts on them and no value can turn
// a serialised boot green.
//
// Timed at a real origin rather than page.route(): interception bypasses service workers, and it
// would measure Playwright's scheduling instead of the browser's. This proxy records server-side
// when each request arrived and when its last byte left, and doubles as the fault injector.

import { expect, test, type Page } from "@playwright/test";
import http from "node:http";
import type { AddressInfo } from "node:net";

/** One request as the origin saw it: when it arrived, when its body finished. */
interface Hit {
  url: string;
  start: number;
  end: number;
}

/** A forwarding origin that records request timings, can refuse a path, and can hold one back.
 *  `upstream` comes from the config's baseURL rather than hardcoded: a second copy of the port is
 *  a test that silently stops testing the day the port moves. */
function recordingOrigin(upstream: string): Promise<{
  url: string;
  /** Every request so far, in arrival order. */
  hits: Hit[];
  /** Answer any request whose URL contains `match` with `status` instead of forwarding it. */
  refuse: (match: string, status: number) => void;
  /** Sit on any request whose URL contains `match` for `ms` before forwarding it. Fault injection:
   *  it makes the order of two waits observable. */
  delay: (match: string, ms: number) => void;
  close: () => Promise<void>;
}> {
  const up = new URL(upstream);
  // Node dialling Node: pin the address family. The browser's baseURL stays "localhost"; only
  // this hop goes by number.
  if (up.hostname === "localhost") up.hostname = "127.0.0.1";
  const hits: Hit[] = [];
  let refused: { match: string; status: number } | null = null;
  let delayed: { match: string; ms: number } | null = null;
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const forward = (req: http.IncomingMessage, res: http.ServerResponse): void => {
    const fwd = http.request(
      `${up.origin}${req.url}`,
      { method: req.method, headers: { ...req.headers, host: up.host } },
      (ur) => {
        res.writeHead(ur.statusCode ?? 502, ur.headers);
        ur.pipe(res);
      },
    );
    fwd.on("error", () => res.destroy());
    // Piping an already-ended stream never ends `fwd`, so the forward would hang. Reachable only
    // from the delayed path, where time passes before we get here.
    if (req.readableEnded) fwd.end();
    else req.pipe(fwd);
  };
  const server = http.createServer((req, res) => {
    const hit: Hit = { url: req.url ?? "", start: performance.now(), end: 0 };
    hits.push(hit);
    // The last byte out is the end of the download, and the anchor the overlap is measured from.
    res.on("finish", () => (hit.end = performance.now()));
    if (refused && hit.url.includes(refused.match)) {
      res.writeHead(refused.status, { "content-type": "text/plain" });
      res.end("refused by the test origin");
      return;
    }
    if (delayed && hit.url.includes(delayed.match)) {
      const t = setTimeout(() => {
        timers.delete(t);
        if (!res.destroyed) forward(req, res);
      }, delayed.ms);
      timers.add(t);
      return;
    }
    forward(req, res);
  });
  return new Promise((resolve) => {
    server.listen(0, () => {
      const { port } = server.address() as AddressInfo;
      resolve({
        url: `http://localhost:${port}/`,
        hits,
        refuse: (match, status) => (refused = { match, status }),
        delay: (match, ms) => (delayed = { match, ms }),
        // closeAllConnections first: `server.close()` waits for every open socket, and the page
        // keeps several alive (a dev server's HMR client re-polls forever), so without this the
        // origin never closes and the test times out having already passed.
        close: () =>
          new Promise((done) => {
            for (const t of timers) clearTimeout(t);
            timers.clear();
            server.closeAllConnections();
            server.close(() => done());
          }),
      });
    });
  });
}

/** A first visit, first-run dismissed, text on screen. The analysis tiers are left off: the
 *  optional analytics pack would only add noise to the requests being counted. */
async function firstVisit(page: Page, url: string): Promise<void> {
  await page.goto(url);
  const established = page.getByRole("button", { name: "Established believer" });
  const canvas = page.locator(".pane canvas").first();
  await expect(established.or(canvas)).toBeVisible({ timeout: 120_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(canvas).toBeVisible({ timeout: 120_000 });
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 120_000 });
}

/** How long the origin sits on the corpus cache before serving it. Fault injection, not a budget:
 *  nothing asserts on it, and no value can turn a serialised boot green because a serialised boot
 *  does not ask for the engine binary until the corpus read has returned. It exists because a
 *  loopback origin serves the whole 37 MB pack in single-digit milliseconds, against which the
 *  ordering would be decided by whether a Cache API probe happened to land first. */
const CORPUS_HOLD_MS = 3_000;

test("the engine binary is fetched beside the text, not after it", { tag: "@perf" }, async ({ page, baseURL }) => {
  const origin = await recordingOrigin(baseURL!);
  try {
    origin.delay("kjv.jsonl.idxcache", CORPUS_HOLD_MS);
    await firstVisit(page, origin.url);
    // Snapshot at first text: the background load and the idle cache sweep start right after, so
    // a later count answers a different question.
    const hits = [...origin.hits];

    const engine = hits.filter((h) => h.url.includes("plumbline_ffi.wasm"));
    const corpus = hits.filter((h) => h.url.includes("kjv.jsonl.idxcache"));
    console.log(
      "boot-overlap:",
      JSON.stringify({ engine: engine.map((h) => [Math.round(h.start), Math.round(h.end)]), corpus: corpus.map((h) => [Math.round(h.start), Math.round(h.end)]) }),
    );
    expect(corpus.length, "stage 1's corpus cache should be fetched once on a first visit").toBe(1);

    // Exactly one: the prefetch hands the bytes over through the depot, so instantiate()'s read is
    // a local hit. Two means the two sites disagree about the URL — a silent second 1.7 MB
    // download on every first visit.
    expect(
      engine.length,
      `the engine binary was requested ${engine.length}× — the prefetch and the ` +
        `instantiate must agree on its URL, or the overlap costs a whole extra download`,
    ).toBe(1);

    // The overlap itself: serialised, the engine fetch cannot start until the corpus read has
    // ended; overlapped, it starts before the corpus request is even made.
    expect(corpus[0].end).toBeGreaterThan(0);
    expect(
      engine[0].start,
      `the engine fetch started ${Math.round(engine[0].start - corpus[0].end)}ms AFTER the ` +
        `corpus read finished — the two boot waits are serialised again, and that ` +
        `gap is dead time before first text`,
    ).toBeLessThan(corpus[0].end);
  } finally {
    await origin.close();
  }
});

/** How long the origin sits on the two reader faces. Fault injection, not a budget: nothing
 *  asserts on it, and awaited in front of the boot the corpus read cannot begin until both faces
 *  have arrived whatever the value. Both faces, not one — `loadFonts` loads them in sequence, so
 *  holding only the first leaves a margin thin enough to be luck. */
const FONT_HOLD_MS = 8_000;

test("the reader faces load beside the boot, not in front of it", { tag: "@perf" }, async ({ page, baseURL }) => {
  const origin = await recordingOrigin(baseURL!);
  try {
    // Set before the first byte: the worker asks for these the moment it gets the boot message.
    origin.delay("EBGaramond", FONT_HOLD_MS);
    await firstVisit(page, origin.url);
    const hits = [...origin.hits];

    // Both threads request them (the document paints with them, the worker measures with them),
    // so this is several hits for two files and the earliest completion is the anchor.
    const fonts = hits.filter((h) => /EBGaramond.*\.woff2/.test(h.url) && h.end > 0);
    const corpus = hits.filter((h) => h.url.includes("kjv.jsonl.idxcache"));
    console.log(
      "font-overlap:",
      JSON.stringify({
        fonts: fonts.map((h) => [Math.round(h.start), Math.round(h.end)]),
        corpus: corpus.map((h) => [Math.round(h.start), Math.round(h.end)]),
      }),
    );
    expect(
      fonts.length,
      "no reader face was requested at all — the filename hash moved and this test is measuring nothing",
    ).toBeGreaterThan(0);
    expect(corpus.length, "stage 1's corpus cache should be fetched once on a first visit").toBe(1);

    const firstFontEnd = Math.min(...fonts.map((h) => h.end));
    expect(
      corpus[0].start,
      `the corpus read began ${Math.round(corpus[0].start - firstFontEnd)}ms AFTER the first ` +
        `reader face had finished downloading — the fonts are awaited in front of the boot ` +
        `again, so the whole font wait is dead time before the splash moves at all`,
    ).toBeLessThan(firstFontEnd);
  } finally {
    await origin.close();
  }
});

test("an engine binary that will not download reaches the reader, not an unhandled rejection", async ({
  page,
  baseURL,
}) => {
  const origin = await recordingOrigin(baseURL!);
  // `pageerror`, not a console listener: chromium reports a dedicated worker's unhandled rejection
  // to Playwright as a page error and emits no console output at all, so a version watching
  // console errors for /uncaught \(in promise\)/ passed with the routed `.catch()` deleted. A
  // console listener would also be noisy here — the failure path console.errors through
  // worker-client's #fail by design.
  const runtimeErrors: string[] = [];
  page.on("pageerror", (e) => runtimeErrors.push(String(e)));
  try {
    origin.refuse("plumbline_ffi.wasm", 503);
    await page.goto(origin.url);

    // Out of boot, out of the boot RPC, onto the splash in the browser's own words, with a Retry.
    const shown = page.locator(".splash .error");
    await expect(shown).toBeVisible({ timeout: 120_000 });
    await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();

    // The raw string sits one disclosure away: the reader gets a sentence they can act on, and
    // `<details>` keeps the machine words a bug report pastes. Both halves matter — `.error` must
    // not be the raw exception, and the raw must still be somewhere on the screen.
    expect(
      await shown.textContent(),
      "the reader was shown the raw exception instead of a sentence they can act on",
    ).not.toContain("HTTP 503");
    const raw = page.locator(".splash details pre");
    await expect(raw, "the raw string is not reachable anywhere on the error screen").toContainText(
      "plumbline_ffi.wasm",
    );
    await expect(raw).toContainText("HTTP 503");

    // Delivered once, down the path the caller controls, rather than also escaping the promise
    // nobody was awaiting yet — in a worker that is a dead thread and a splash that never moves.
    expect(
      runtimeErrors,
      `the un-awaited engine fetch left its rejection to the runtime: ${runtimeErrors.join(" | ")}`,
    ).toEqual([]);
  } finally {
    await origin.close();
  }
});
