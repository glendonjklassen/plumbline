import { expect, test, type Page } from "@playwright/test";

// A running schedule plan rides the reader as a nav-strip chip ("Day 1 · Matt 1–2",
// tap → today's first unread chapter), and the passage navigator opens with a today
// card whose chapters are the buttons. Dies if Shell stops mounting the chip, if
// `todayPlans` stops finding the running schedule, or if BookNav loses the card.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

test("a running plan rides the reader: the chip goes to today, the navigator leads with it", async ({ page }) => {
  await boot(page);

  // Start the schedule through the engine; the picker UI has its own coverage.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "nt-90", new Date().toISOString());
  });

  // The chip is icon-only, so its label is the aria-label; the expected target is
  // pinned from the same wire the chip reads.
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  await expect(chip).toBeVisible({ timeout: 10_000 });
  await expect(chip).toHaveAttribute("aria-label", /Day 1 · /);

  const first = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const plans = await s.fetchQ("plans", "");
    return plans.running.find((p: any) => p.id === "nt-90").today.chapters[0];
  });

  await chip.click();
  expect(await page.evaluate(() => {
    const p = (window as any).__plumbline.panes[0];
    return { book: p.book, chapter: p.chapter };
  })).toEqual({ book: first.book, chapter: first.chapter });

  await page.locator(".pane .nav .passage").first().click();
  const card = page.locator('[data-surface="plan-today"]');
  await expect(card).toBeVisible();
  await expect(card).toContainText("Day 1");
  await expect(card).toContainText("The New Testament in 90 days");
  await card.getByRole("button", { name: first.display, exact: false }).first().click();
  await expect(card).toHaveCount(0); // the dialog closed with the navigation
  expect(await page.evaluate(() => (window as any).__plumbline.panes[0].chapter)).toBe(first.chapter);

  // In concept-study mode the chip stands down: the tracker is suspended there, so
  // reading in the mode would earn the schedule no credit.
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("grace");
  });
  await expect(page.locator(".plan-chip-row")).toHaveCount(0);
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  await expect(chip).toBeVisible();
});

// The chronological plan's curated table through the whole stack. Dies if the table
// falls out of the pack (the row disappears), if the loader breaks (a start errors),
// or if the walk's head stops being Gen 1 (a scrambled table).
test("the chronological plan is offered, starts, and day 1 begins at Genesis 1", async ({ page }) => {
  await boot(page);

  // The row is only offered when the shipped pack's table loads: the engine filters
  // unbuildable table plans out.
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  const row = page.getByRole("button", { name: /The Bible in chronological order/ });
  await expect(row).toBeVisible();
  await row.click();

  const readToday = () =>
    page.evaluate(async () => {
      const s = (window as any).__plumbline;
      const plans = await s.fetchQ("plans", "");
      return plans.running.find((p: any) => p.id === "chronological")?.today ?? null;
    });
  // The click's start is async; poll until the run reports its day-1 card.
  await expect.poll(async () => (await readToday())?.day ?? 0, { timeout: 10_000 }).toBe(1);
  expect((await readToday()).chapters[0]).toMatchObject({ book: "Gen", chapter: 1 });

  // The chip lives on the Read screen, and Plans replaces the reader, so go back
  // before looking for it.
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(page.locator(".plan-chip-row .plan-chip").first()).toHaveAttribute("aria-label", /Day 1 · /);
});

// While a plan runs, the picker drops the other plans of its class: with them on
// offer, the only thing a tap could mean is "throw away the plan I'm on".
test("a running plan takes its class off the picker", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "bible-365", new Date().toISOString());
  });
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));

  await expect(page.locator(".plan-card", { hasText: "The whole Bible in a year" })).toBeVisible();
  for (const rival of ["The whole Bible in 180 days", "The whole Bible in 90 days", "The Bible in chronological order"]) {
    await expect(page.locator(".plan-builtin", { hasText: rival })).toHaveCount(0);
  }
  // Another class is still on offer: the filter is per class, not blanket.
  await expect(page.locator(".plan-builtin", { hasText: "The New Testament in 90 days" })).toBeVisible();
});

test("finishing a chapter advances the chip and its label", async ({ page }) => {
  await boot(page);
  const first = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "bible-365", new Date().toISOString());
    const plans = await s.fetchQ("plans", "");
    return plans.running.find((p: any) => p.id === "bible-365").today.chapters[0];
  });
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  // Icon-only chip: its label is the aria-label, not its text.
  await expect(chip).toHaveAttribute("aria-label", new RegExp(first.display.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  const labelBefore = await chip.getAttribute("aria-label");

  // Travel the dwell path, not an authoring write: authoring always invalidates the
  // plans cache, so only the dwell path can catch a stale one.
  await page.evaluate(async (c) => {
    const s = (window as any).__plumbline;
    const t0 = Date.now();
    for (let i = 0; i < 1200; i++) {
      const out = await s.rpc.call(
        "readingTick", c.book, c.chapter, 31, 1, true, new Date(t0 + i * 1000).toISOString());
      if (out?.completed) { await s.rpc.call("readingTick", null, 0, 0, 0, false, new Date(t0 + (i + 1) * 1000).toISOString()); return; }
    }
    throw new Error("dwell never completed the chapter");
  }, first);

  // The chip names what is left to read: a label that never moves is the bug.
  await expect(chip).not.toHaveAttribute("aria-label", labelBefore!);
  const landed = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    document.querySelector<HTMLButtonElement>(".plan-chip-row .plan-chip")!.click();
    await new Promise((r) => setTimeout(r, 300));
    const p = s.panes[0];
    return { book: p.book, chapter: p.chapter };
  });
  expect(landed, "the chip still sent the reader back to the chapter they finished").not.toEqual({
    book: first.book,
    chapter: first.chapter,
  });
});

// Finishing the day's worth advances the chip to the next day's portion rather than
// standing it down, so a reader can work ahead. `doneToday` stays on the wire (the
// Study hub's band uses it), so this also dies if it falls off.
test("reading the day's worth advances the chip to the next day", async ({ page }) => {
  await boot(page);
  const today = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "nt-90", new Date().toISOString());
    const plans = await s.fetchQ("plans", "");
    return plans.running.find((p: any) => p.id === "nt-90").today.chapters;
  });
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  await expect(chip).toBeVisible({ timeout: 10_000 });

  // 999 verses / 3600s: past the last verse and long enough to count, i.e. the
  // completed pass the tracker would have banked.
  await page.evaluate(async (chapters) => {
    const s = (window as any).__plumbline;
    for (const c of chapters) {
      const out = await s.rpc.call("readingRecord", c.book, c.chapter, 999, 3600, new Date().toISOString());
      if (!out?.completed) throw new Error(`chapter did not complete: ${c.book} ${c.chapter}`);
    }
  }, today);

  await expect
    .poll(async () =>
      page.evaluate(async () => {
        const plans = await (window as any).__plumbline.fetchQ("plans", "");
        return plans.running.find((p: any) => p.id === "nt-90").doneToday;
      }),
    )
    .toBe(true);
  await expect(chip).toBeVisible();
  await expect(chip).toHaveAttribute("aria-label", /Day 2 · /);
});

// Pause sets a plan aside whole and Resume brings it back with nothing lost. Dies
// if the endpoint drops, if todayPlans stops filtering `paused`, or if the Plans
// card loses the Pause/Resume controls.
test("a paused plan asks nothing until it is resumed", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "bible-365", new Date().toISOString());
  });
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  await expect(chip).toBeVisible({ timeout: 10_000 });

  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  const card = page.locator(".plan-card", { hasText: "The whole Bible in a year" });
  await card.getByRole("button", { name: "Pause" }).click();
  await expect(card).toContainText("paused");
  await expect(card).toContainText(/Started /); // the run's identity: name + start day
  await expect(card.getByRole("button", { name: "Pause" })).toHaveCount(0);

  // The plan chip goes, but the row itself may stand: it is also the bookmarks
  // strip, which is not the plan's to take down.
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(page.locator(".plan-chip-row .plan-chip")).toHaveCount(0);

  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  await card.getByRole("button", { name: "Resume" }).click();
  await expect(card.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(page.locator(".plan-chip-row .plan-chip").first()).toHaveAttribute("aria-label", /Day 1 · /);
});

test("no Explore card spills its text past its border, at any text scale", async ({ page }) => {
  // Cards spill because the global 44px tap floor replaces the automatic minimum
  // size that stops a grid item being shorter than its own text. Phone width only:
  // a desktop is wide enough for the text to fit even squashed, hiding the bug.
  await page.setViewportSize({ width: 360, height: 740 });
  await boot(page);
  for (const scale of [1, 1.4]) {
    await page.evaluate((z) => {
      (window as any).__plumbline.config.uiScale = z;
      document.documentElement.style.setProperty("--uiScale", String(z));
    }, scale);
    // Mount Explore directly; the navigation path has its own coverage.
    await page.evaluate(() => (((window as any).__plumbline as any).screen = "explore"));
    await expect(page.locator(".ex-card").first()).toBeVisible();
    const spills = await page.evaluate(() =>
      [...document.querySelectorAll<HTMLButtonElement>(".ex-card")]
        .filter((b) => b.scrollHeight > b.clientHeight + 1)
        .map((b) => ({ text: (b.textContent || "").replace(/\s+/g, " ").slice(0, 40), client: b.clientHeight, scroll: b.scrollHeight })),
    );
    expect(spills, `cards overflow at uiScale ${scale}`).toEqual([]);
  }
});
