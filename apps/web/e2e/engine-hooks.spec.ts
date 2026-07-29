// One owner per engine hook. `StudyEngine.onAuthored` and `onReadingWrite` are
// single-slot properties, not listener lists — a second assignment silently
// wins and the loser becomes dead code that reads as live. boot.ts carried
// exactly that: a 50 ms persistence debounce on `onAuthored` that could never
// fire, because the only caller of `boot()` (engine.worker.ts) reassigns the
// hook on the next statement after the await resolves. Tuning the debounce in
// boot.ts changed nothing, and the copy that did run also had to postMessage
// `authored` to the shell, which boot's copy did not.
//
// A SOURCE guard, deliberately, and the only honest one: a dead duplicate is by
// definition invisible at runtime, so every behavioural test would have passed
// while the bug was live — which the working rules forbid. What this pins is
// the invariant instead of a symptom: the wiring stays in one place.
//
// Scope is src/engine, where the StudyEngine's hooks get wired. The shell-side
// hooks of the same name on WorkerClient are assigned from src/state and are a
// different object's slots.
//
// Mutation-tested 2026-07-29 (break the fix, watch it fail, restore): with the
// deleted boot.ts handler pasted back in, both assignment sites are listed and
// the test goes red — "engine.onAuthored is a single slot […] assigned at
// boot.ts:124, engine.worker.ts:620". Removed again, green.

import { expect, test } from "@playwright/test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ENGINE_DIR = fileURLToPath(new URL("../src/engine", import.meta.url));

/** Single-slot engine hook → the one module allowed to wire it. */
const OWNERS: Record<string, string> = {
  onAuthored: "engine.worker.ts",
  onReadingWrite: "engine.worker.ts",
};

/** Every `<expr>.<hook> =` in src/engine, as `file:line`. The class-field
 *  declarations on StudyEngine have no dot before the name, so they do not
 *  count; neither does prose in a comment that happens to quote the wiring. */
function assignmentSites(hook: string): string[] {
  const assigns = new RegExp(`\\.${hook}\\s*=(?!=)`);
  const sites: string[] = [];
  for (const name of readdirSync(ENGINE_DIR).sort()) {
    if (!name.endsWith(".ts")) continue;
    readFileSync(join(ENGINE_DIR, name), "utf8")
      .split("\n")
      .forEach((line, i) => {
        const code = line.trim();
        if (code.startsWith("//") || code.startsWith("*") || code.startsWith("/*")) return;
        if (assigns.test(code)) sites.push(`${name}:${i + 1}`);
      });
  }
  return sites;
}

for (const [hook, owner] of Object.entries(OWNERS)) {
  test(`engine.${hook} is wired in exactly one place`, () => {
    const sites = assignmentSites(hook);
    const where = sites.length ? `assigned at ${sites.join(", ")}` : "assigned nowhere";
    expect(
      sites,
      `engine.${hook} is a single slot: the last assignment wins and every earlier one is dead code — ${where}. Only ${owner} may wire it.`,
    ).toHaveLength(1);
    expect(sites[0]?.split(":")[0], `engine.${hook} must be wired in ${owner}`).toBe(owner);
  });
}
