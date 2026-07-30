// Boot overlap: the engine binary, the reader faces and the scripture text all
// download AT THE SAME TIME (TODO F, 2026-07-29).
//
// None of the three needs another until the moment it is used, and until this
// change they were awaited in order — the ~1.7 MB engine binary before the text
// (`boot()`), and both font files before the boot even started (the worker's boot
// op). On a slow connection that was dead time before first text. `boot()` now
// starts the engine fetch un-awaited at the top and collects it at the
// instantiate site; the worker starts `loadFonts` un-awaited and collects it just
// before the boot reply, which is still before any layout op can be answered.
//
// WHAT THIS ASSERTS, AND WHAT IT DELIBERATELY DOES NOT. It asserts the OVERLAP —
// that one download had started before another finished — and never a wall-clock
// budget. A millisecond ceiling here would be a constant, and CLAUDE.md records
// two tests that passed against the very bugs they described for exactly that
// reason. The overlap is a structural fact: with the awaits back in order the
// engine request cannot begin until the corpus read has ended, and the corpus
// read cannot begin until both faces have arrived — so the ordering separates
// fixed from broken however fast or slow the machine is.
//
// The two holds below (`CORPUS_HOLD_MS`, `FONT_HOLD_MS`) are FAULT INJECTION, not
// budgets. Nothing asserts on either number and no value for either can turn a
// serialised boot green; they exist because a loopback origin serves the whole
// pack in single-digit milliseconds, and against that an ordering fact would be
// decided by whether a local storage probe happened to land first.
//
// AND IT IS OBSERVED FROM A REAL ORIGIN, not page.route(): interception bypasses
// service workers, and more to the point it would be measuring Playwright's own
// scheduling rather than the browser's. This proxy records, server-side, when
// each request arrived and when its last byte went out — the requests actually
// made, timed by something neither the app nor the test can talk its way past.
// It doubles as the fault injector: it holds the corpus and the fonts back, and
// it refuses the engine binary outright for the last test, where the failure has
// to reach the reader instead of the runtime.

import { expect, test, type Page } from "@playwright/test";
import http from "node:http";
import type { AddressInfo } from "node:net";

/** One request as the origin saw it: when it arrived, when its body finished. */
interface Hit {
  url: string;
  start: number;
  end: number;
}

/** A forwarding origin that records request timings, can refuse a path, and can
 *  hold one back.
 *
 *  `upstream` is the suite's own server (preview in CI, a dev server locally),
 *  taken from the config's baseURL rather than hardcoded — a second copy of the
 *  port is a test that silently stops testing the day the port moves. */
function recordingOrigin(upstream: string): Promise<{
  url: string;
  /** Every request so far, in arrival order. */
  hits: Hit[];
  /** Answer any request whose URL contains `match` with `status` instead of
   *  forwarding it. */
  refuse: (match: string, status: number) => void;
  /** Sit on any request whose URL contains `match` for `ms` before forwarding
   *  it. Fault injection: it makes the ORDER of two waits observable. */
  delay: (match: string, ms: number) => void;
  close: () => Promise<void>;
}> {
  const up = new URL(upstream);
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
    // A request whose body already ended has nothing left to pipe, and piping an
    // ended stream never ends `fwd` — so the forward would hang. Reachable only
    // from the delayed path, where time passes before we get here.
    if (req.readableEnded) fwd.end();
    else req.pipe(fwd);
  };
  const server = http.createServer((req, res) => {
    const hit: Hit = { url: req.url ?? "", start: performance.now(), end: 0 };
    hits.push(hit);
    // The moment the last byte left for the browser — the honest end of a
    // download, and the anchor the overlap is measured against.
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
        // closeAllConnections FIRST, and it is not tidiness: `server.close()`
        // waits for every open socket, and the page it just served keeps several
        // alive (a dev server's HMR client re-polls forever). Without this the
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

/** A first visit, first-run dismissed, text on screen.
 *
 *  The analysis tiers are left OFF: this is a boot-path test, and ~4 MB of
 *  optional analytics would only add noise to the requests being counted. */
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

/** How long the origin sits on the corpus cache before serving it.
 *
 *  FAULT INJECTION, NOT A BUDGET — nothing below asserts on this number, and no
 *  value for it can turn a serialised boot green, because a serialised boot does
 *  not ask for the engine binary until the corpus read has already returned.
 *
 *  It is here because a loopback origin serves the whole 37 MB pack in single-
 *  digit milliseconds, and against that the ordering would be decided by whether
 *  a Cache API probe happens to land first — a coin toss, and the failure would
 *  land on the fixed code. This restores the proportion the item is about: a
 *  connection where the text takes longer to arrive than a local lookup takes to
 *  answer, which is every phone. */
const CORPUS_HOLD_MS = 3_000;

test("the engine binary is fetched beside the text, not after it", async ({ page, baseURL }) => {
  const origin = await recordingOrigin(baseURL!);
  try {
    origin.delay("kjv.jsonl.idxcache", CORPUS_HOLD_MS);
    await firstVisit(page, origin.url);
    // Snapshot at first text: the background load and the idle cache sweep start
    // right after, and a count taken later is a count of a different question.
    const hits = [...origin.hits];

    const engine = hits.filter((h) => h.url.includes("plumbline_ffi.wasm"));
    const corpus = hits.filter((h) => h.url.includes("kjv.jsonl.idxcache"));
    console.log(
      "boot-overlap:",
      JSON.stringify({ engine: engine.map((h) => [Math.round(h.start), Math.round(h.end)]), corpus: corpus.map((h) => [Math.round(h.start), Math.round(h.end)]) }),
    );
    expect(corpus.length, "stage 1's corpus cache should be fetched once on a first visit").toBe(1);

    // EXACTLY ONE. The prefetch hands the bytes over through the depot, so the
    // read inside instantiate() is a local hit. Two requests here means the two
    // sites disagree about the URL — a silent second 1.7 MB download on every
    // first visit, which is the opposite of the point.
    expect(
      engine.length,
      `the engine binary was requested ${engine.length}× — the prefetch and the ` +
        `instantiate must agree on its URL, or the overlap costs a whole extra download`,
    ).toBe(1);

    // THE OVERLAP ITSELF. Serialised, the engine fetch cannot start until the
    // corpus read has ended; overlapped, it starts before it — in fact before the
    // corpus request is even made.
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

/** How long the origin sits on the two reader faces in the test below.
 *
 *  FAULT INJECTION, NOT A BUDGET — nothing is asserted about this number. It
 *  makes the order of two independent waits observable with the same margin in
 *  both directions: awaited in front of the boot, the corpus read cannot begin
 *  until both faces have arrived, and no value here can change that. Both faces,
 *  not one: `loadFonts` loads them in sequence, so holding only the first leaves
 *  a serialised boot starting the corpus read milliseconds after that face
 *  finished — a margin thin enough to be luck. */
const FONT_HOLD_MS = 8_000;

test("the reader faces load beside the boot, not in front of it", async ({ page, baseURL }) => {
  const origin = await recordingOrigin(baseURL!);
  try {
    // Set before the first byte: the worker asks for these the moment it gets the
    // boot message, which is the wait this test is about.
    origin.delay("EBGaramond", FONT_HOLD_MS);
    await firstVisit(page, origin.url);
    const hits = [...origin.hits];

    // Requested by BOTH threads — the document paints with them, the worker
    // measures with them — so this is several hits for two files, and the
    // earliest completion is the anchor.
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
  // `pageerror`, and NOT a console listener. Chromium reports a DEDICATED
  // WORKER's unhandled rejection to Playwright as a page error, not as a console
  // message — measured 2026-07-30 on a throwaway origin whose worker did nothing
  // but `Promise.reject(new Error("BOOM-PROBE"))` with a handler attached three
  // seconds later (the exact shape boot.ts has): chromium emitted `pageerror
  // "Error: BOOM-PROBE"` and no console output at all. The first version of this
  // test watched console errors for /uncaught \(in promise\)/ and PASSED with the
  // routed `.catch()` deleted — worthless, and the third such test this repo has
  // caught. A console listener would also be noisy here for the wrong reason: the
  // failure path deliberately console.errors through worker-client's #fail.
  const runtimeErrors: string[] = [];
  page.on("pageerror", (e) => runtimeErrors.push(String(e)));
  try {
    origin.refuse("plumbline_ffi.wasm", 503);
    await page.goto(origin.url);

    // The same failure path an awaited instantiate() used: out of boot, out of the
    // boot RPC, onto the splash in the browser's own words, with a Retry.
    const shown = page.locator(".splash .error");
    await expect(shown).toBeVisible({ timeout: 120_000 });
    await expect(shown).toContainText("plumbline_ffi.wasm");
    await expect(shown).toContainText("HTTP 503");
    await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();

    // Nothing may have reached the runtime unhandled. The splash above proves the
    // error was DELIVERED; this proves it was delivered ONCE, down the path the
    // caller controls, rather than also escaping the promise nobody was awaiting
    // yet — which in a worker is a console error at best and a dead thread at
    // worst, with the reader left on a splash that never moves.
    expect(
      runtimeErrors,
      `the un-awaited engine fetch left its rejection to the runtime: ${runtimeErrors.join(" | ")}`,
    ).toEqual([]);
  } finally {
    await origin.close();
  }
});
