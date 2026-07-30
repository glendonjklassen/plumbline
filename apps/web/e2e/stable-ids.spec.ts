import { expect, test, type Page } from "@playwright/test";

// STABLE IDS, PROVEN IN A BROWSER — docs/STABLE-IDS.md.
//
// Threads, tags and weaves gained two additive fields: `id` (32 hex chars,
// minted once, never derived from the name) and `updated` (the UTC stamp of the
// last mutating save). The core's own tests pin the rules; this pins the two
// things only a real browser can answer.
//
//  1. `updated` has to come from somewhere for the mutations the shells send no
//     stamp with — setting a thread's notes, dropping a tag member. The engine
//     reads its own clock for those (`now_stamp` in crates/ffi), and in the
//     browser that clock is the WASI shim answering `clock_time_get`. If the
//     shim did not implement it, `SystemTime::now()` would return an error, and
//     the code maps an error to the epoch — so the failure is SILENT: every save
//     would be stamped 1970-01-01 and last-writer-wins would be decided by a
//     coin toss for ever. Compiling for wasm32-wasip1 proves nothing about this;
//     only running it does.
//
//  2. `id` needs 128 random bits, which come from std's `RandomState`, i.e.
//     `random_get` under the same shim.
//
// Both are read back out of the file the engine actually wrote — through
// `exportUserData`, which is the same list the backup zip is built from — rather
// than off the wire, because the wire does not carry ids yet (the ordinals stay
// until the codegen work in TODO §H).

test.setTimeout(120_000);

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** One authored file's text, straight out of the tree the backup is built from. */
async function authoredFile(page: Page, path: string): Promise<string> {
  const text = await page.evaluate(async (want) => {
    const files = new Map<string, Uint8Array>(await (window as any).__plumbline.rpc.exportUserData());
    const bytes = files.get(want);
    return bytes ? new TextDecoder().decode(bytes) : null;
  }, path);
  expect(text, `${path} is not in the exported tree`).not.toBeNull();
  return text as string;
}

const HEX32 = /^[0-9a-f]{32}$/;

test("an authored tag carries an id and a stamp the browser's own clock agrees with", async ({ page }) => {
  await boot(page);

  // The shell sends a stamp with a tag ADD, so this half would pass even with a
  // dead clock. It is here for the id, and to give the next step something to
  // mutate without one.
  const sent = "2026-08-01T00:00:00Z";
  const err = await page.evaluate(
    async (added) => await (window as any).__plumbline.engine.tagAdd("Clockwork", "verse", "John 3:16", null, added),
    sent,
  );
  expect(err, "tagAdd failed").toBeNull();

  const tag = JSON.parse(await authoredFile(page, "tags/clockwork.json"));
  expect(tag.id, `id is not 32 lowercase hex: ${tag.id}`).toMatch(HEX32);
  expect(tag.updated).toBe(sent);
  expect(tag.created).toBe(sent);

  // NOW the part that needs the engine's own clock: removing a member sends no
  // stamp at all. Take the browser's time either side of the call, so the
  // assertion is that the engine's clock agrees with the page's rather than
  // against a constant that goes stale.
  const before = await page.evaluate(() => new Date().toISOString());
  const removed = await page.evaluate(
    async () => await (window as any).__plumbline.engine.tagRemove("Clockwork", "verse", "John 3:16"),
  );
  expect(removed, "tagRemove failed").toBeNull();
  const after = await page.evaluate(() => new Date().toISOString());

  const mutated = JSON.parse(await authoredFile(page, "tags/clockwork.json"));
  expect(mutated.members, "the removal itself did not land").toEqual([]);
  expect(mutated.id, "the identity changed under a mutation").toBe(tag.id);

  // Second-granularity stamps, so compare truncated to the second and allow the
  // boundary either side; the failure this is here to catch is "1970", not a
  // rounded second.
  const sec = (iso: string) => iso.slice(0, 19);
  expect(
    mutated.updated,
    `updated ${mutated.updated} is outside the window ${before}…${after} — a dead clock reads 1970-01-01T00:00:00Z`,
  ).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
  expect(sec(mutated.updated) >= sec(before)).toBe(true);
  expect(sec(mutated.updated) <= sec(after)).toBe(true);
});

test("two objects authored in one session get different ids", async ({ page }) => {
  await boot(page);

  const stamp = "2026-08-02T00:00:00Z";
  const errs = await page.evaluate(async (added) => {
    const e = (window as any).__plumbline.engine;
    return [
      await e.tagAdd("Alpha ids", "verse", "John 1:1", null, added),
      await e.tagAdd("Omega ids", "verse", "Rev 22:13", null, added),
      await e.threadAdd("Thread ids", "Rom 3:23", null, added),
    ];
  }, stamp);
  expect(errs, "an authoring call failed").toEqual([null, null, null]);

  const ids: string[] = [];
  for (const path of ["tags/alpha-ids.json", "tags/omega-ids.json", "threads/thread-ids.json"]) {
    const id = JSON.parse(await authoredFile(page, path)).id;
    expect(id, `${path}: id is not 32 lowercase hex: ${id}`).toMatch(HEX32);
    ids.push(id);
  }
  expect(new Set(ids).size, `ids collided within one session: ${ids.join(", ")}`).toBe(3);
});
