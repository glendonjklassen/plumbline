import { expect, test, type Page } from "@playwright/test";

// The pasteable bug report must not depend on PERF, which must ship off. The report's
// header (release tag, build id, engine version, data pack version, device) is a
// normal part of Settings; PERF only decides whether MEASUREMENTS are appended. These
// two tests hold both halves: the report is there with the flag off, and it is off.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  // The analysis tiers are left off deliberately: nothing in a report header comes
  // from the analysis pack, so these tests need not wait for that download.
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

function settings(page: Page) {
  return page.locator('[data-surface="settings"]');
}

async function openSettings(page: Page): Promise<void> {
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(settings(page)).toBeVisible();
  // The report lives behind the Advanced disclosure; every path to it goes here.
  const adv = settings(page).locator("details.advanced");
  if (!(await adv.evaluate((d) => (d as HTMLDetailsElement).open))) {
    await adv.locator("> summary").click();
  }
}

/** The report as the reader sees it, once the async device survey has landed.
 *
 *  Scoped to the Settings surface and asserted to be the only one: `pre.report`
 *  belongs to this dialog alone today, but a class is not a `data-surface`, and
 *  the repo has already had one sweep measure the passage navigator while
 *  reporting on Settings.
 *
 *  Read with `textContent`, not `innerText`: the block is styled `pre-wrap`, so
 *  innerText is at the mercy of where the lines happen to wrap in the viewport. */
async function reportText(page: Page): Promise<string> {
  const report = settings(page).locator("pre.report");
  await expect(report, "there should be exactly one bug report on screen").toHaveCount(1);
  // Idempotent: a blind click on the summary would CLOSE the disclosure on a
  // second call and then read an element that is only in the DOM by luck.
  // `.diag`, not bare `details:has(...)`: the report nests inside the
  // Advanced disclosure, which that selector would also match.
  const disclosure = settings(page).locator("details.diag:has(pre.report)");
  if (!(await disclosure.evaluate((d) => (d as HTMLDetailsElement).open))) {
    await disclosure.locator("summary").click();
  }
  await expect(report).toBeVisible();
  // `storage used` / `pack files` come from surveyOffline(), which is dispatched
  // when the dialog opens — so poll for the last of those lines rather than
  // catching the report half-built and calling it a missing field.
  await expect
    .poll(async () => (await report.textContent()) ?? "", { timeout: 60_000, intervals: [500] })
    .toMatch(/pack files\s+\d+ · missing \d+/);
  return (await report.textContent()) ?? "";
}

test("the bug report names the build, the pack and the device, in a build that measures nothing", async ({
  page,
}) => {
  // MUTATION (either recreates D-20; both verified only as descriptions, not run):
  //   * apps/web/src/shell/SettingsDialog.svelte — add `if (!PERF) return [];` as
  //     the first line of `reportHeader()`. The report collapses to the one line
  //     saying this build is not measuring itself, and this test goes red on
  //     "the report must open with the build stamp".
  //   * apps/web/src/shell/SettingsDialog.svelte — wrap the template section from
  //     `<p class="label">Report a problem</p>` through its `</details>` in
  //     `{#if PERF}` … `{/if}`. Red on the "Copy bug report" button, which is not
  //     rendered at all.
  await boot(page);
  await openSettings(page);

  // The button is the reader's whole path to this: there is no other way to get
  // the text out of a phone.
  await expect(settings(page).getByRole("button", { name: "Copy bug report" })).toBeVisible();

  const lines = (await reportText(page)).split("\n");
  expect(lines[0], "the report must open with the build stamp").toMatch(
    /^Plumbline \S+ · build \S+ · engine \S+/,
  );
  // The DATA version, which moves independently of the code — and the one field
  // that used to be read out of the (PERF-shaped) diagnostics round trip, so a
  // literal "?" here is the regression, not a missing line.
  expect(lines[1], "the report must name the data pack this session booted on").toMatch(
    /^data pack [^?\s]{4,}/,
  );

  const text = lines.join("\n");
  expect(text, "a report with no device facts cannot be acted on").toContain("DEVICE");
  // Each field asserted to carry a VALUE, not just a label: every one of them
  // prints "?" when whatever it reads is absent, so a label-only check would pass
  // on a report that says nothing. Column widths are not pinned — they are
  // cosmetic, and a test that fails when a label is realigned is noise.
  for (const field of [
    /\n {2}ua\s+Mozilla\S/, // a real user-agent string, not "?"
    /\n {2}cpu threads\s+\d+/,
    /\n {2}screen\s+\d+x\d+ @\d/,
    /\n {2}storage used\s+\d/,
  ]) {
    expect(text, `the device section is missing a value for ${field}`).toMatch(field);
  }
});

test("a release build ships with self-measurement off", async ({ page }) => {
  // PERF is a plain constant the bundler folds away, so from out here the only
  // observable is what it renders: with it on, Settings grows the boot-diagnostics
  // tables and the report grows a stall meter.
  //
  // MUTATION: apps/web/src/engine/perf.ts — `export const PERF = false` →
  // `= true`. THIS test goes red (both assertions), and the test above must stay
  // GREEN. That pair is the decoupling: the flag decides whether numbers are
  // measured, never whether a reader can file an actionable report. Flipping it
  // on for local perf work therefore reds this one test on purpose.
  await boot(page);
  await openSettings(page);

  await expect(
    settings(page).getByText("Boot diagnostics"),
    "a build that measures itself on every reader's phone is about to ship",
  ).toHaveCount(0);

  const text = await reportText(page);
  // Said out loud rather than left blank: a stall meter that was never started
  // reports 0 ms across 0 stalls, which reads as a device that never stalled.
  expect(text, "the report must say why it carries no numbers").toContain(
    "this build is not measuring itself",
  );
  expect(text, "a zeroed stall meter is worse than none").not.toContain("ENGINE THREAD UNAVAILABLE");
});
