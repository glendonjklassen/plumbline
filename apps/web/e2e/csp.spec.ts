import { expect, test, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

// The Content-Security-Policy the site serves lives in ops/hetzner/cloud-init.yaml
// (the box's Caddyfile) and nowhere else: vite.config.ts reads it from there and
// `vite preview` serves it, so every test in this suite already runs under it.
// This file holds the two things that suite cannot see by accident:
//
//   1. The policy names TWO inline scripts by hash: index.html's theme script,
//      which must be inline (it runs before first paint; see the comment on it),
//      and 404.html's redirect, which sends a deep link on a lost path back to
//      the reader. Edit either and its hash is stale, and the browser drops the
//      script SILENTLY: a dark-theme reader gets the cream flash back, a shared
//      link lands on the 404 page, and nothing else fails. So the hashes are
//      re-derived from the build here, the way splash.spec.ts re-derives the
//      palette from theme.rs. (The 404 one was found by this test: the first
//      draft of the policy carried one hash, and the second run of the test
//      reported the other.)
//      Can it fail? Change one character of either inline script, rebuild,
//      leave the Caddyfile alone: the computed set differs from the policy's.
//      Add a third inline script anywhere: three hashes against two.
//
//   2. The app boots to scripture under the policy with zero violations. The
//      engine is a module worker that instantiates wasm and fetches the pack;
//      take 'wasm-unsafe-eval' or worker-src or connect-src out of the policy and
//      the worker never answers, so the verse never lands and the test times
//      out — while a violation anywhere else is caught by the event listener.
//      The header is asserted on the response first, so a preview that is not
//      serving the policy (reuseExistingServer keeps an old `npm run preview`
//      alive across config edits) cannot pass vacuously.

function policy(): string {
  const src = readFileSync(new URL("../../../ops/hetzner/cloud-init.yaml", import.meta.url), "utf8");
  const line = /^\s*Content-Security-Policy "([^"]+)"$/m.exec(src);
  if (!line) throw new Error("ops/hetzner/cloud-init.yaml has no Content-Security-Policy line");
  return line[1];
}

function directive(name: string): string[] {
  const d = policy()
    .split(";")
    .map((s) => s.trim())
    .find((s) => s.startsWith(`${name} `));
  if (!d) throw new Error(`the policy has no ${name} directive`);
  return d.split(/\s+/).slice(1);
}

/** Every inline script in a built page, as the CSP hash that would allow it. */
function inlineScriptHashes(file: string): string[] {
  const html = readFileSync(new URL(`../dist/${file}`, import.meta.url), "utf8");
  return [...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
    (m) => `'sha256-${createHash("sha256").update(m[1]).digest("base64")}'`,
  );
}

test("the policy's script hashes are exactly the build's inline scripts", () => {
  const allowed = directive("script-src")
    .filter((t) => t.startsWith("'sha256-"))
    .sort();
  const built = [...inlineScriptHashes("index.html"), ...inlineScriptHashes("404.html")].sort();
  expect(allowed, "the policy's hashes are not the build's inline scripts — update cloud-init.yaml").toEqual(built);
});

test("nothing in the policy reaches off the origin, and nothing can frame it", () => {
  // Every source list is 'self' or a keyword: the app has no third-party
  // origin, and a host added here is a host every reader's browser would trust.
  for (const d of policy().split(";").map((s) => s.trim())) {
    const tokens = d.split(/\s+/).slice(1);
    for (const t of tokens) expect(t, `${d}: ${t} is not 'self' or a keyword`).toMatch(/^'/);
  }
  expect(directive("frame-ancestors")).toEqual(["'none'"]);
  expect(directive("object-src")).toEqual(["'none'"]);
});

async function violationsDuringBoot(page: Page): Promise<string[]> {
  const fromConsole: string[] = [];
  const note = (text: string) => {
    if (/Content.Security.Policy/i.test(text)) fromConsole.push(text);
  };
  page.on("console", (msg) => note(msg.text()));
  page.on("worker", (w) => w.on("console", (msg) => note(`[worker] ${msg.text()}`)));
  await page.addInitScript(() => {
    const log: string[] = ((window as unknown as { __csp?: string[] }).__csp ??= []);
    document.addEventListener("securitypolicyviolation", (e) => {
      log.push(`${e.violatedDirective} blocked ${e.blockedURI} (${e.sourceFile}:${e.lineNumber})`);
    });
  });
  const response = await page.goto("/");
  expect(
    response?.headers()["content-security-policy"],
    "the preview is not serving the production policy — restart `npm run preview`",
  ).toBe(policy());
  // A fresh profile opens at John 3; the mirror is built from the display list,
  // so a verse there means the worker booted, instantiated the engine and laid
  // the chapter out — the whole path the policy could have blocked.
  await expect(page.locator('.pane .mirror p[data-verse="16"]')).toHaveCount(1, { timeout: 90_000 });
  const inPage = await page.evaluate(() => (window as unknown as { __csp?: string[] }).__csp ?? []);
  return [...fromConsole, ...inPage];
}

test("the app boots to scripture under the production policy without a violation", async ({ page }) => {
  expect(await violationsDuringBoot(page)).toEqual([]);
});
