import { expect, test, type Page } from "@playwright/test";

// Decision #5's reader-side plan surfaces (docs/READING-PLANS.md): when a
// schedule plan is running, its "today" rides the reader as a nav-strip chip
// ("Day 1 · Matt 1–2", tap → today's first unread chapter), and the passage
// navigator opens with a today card whose chapters are the buttons. Each
// assertion dies on the obvious break: the chip if Shell stops mounting it or
// `todayPlans` stops finding the running schedule; the navigation if the tap
// stops going to the first unread chapter; the card if BookNav loses it; the
// concept-study assertion if the chip forgets the mode suspends the tracker.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

test("a running plan rides the reader: the chip goes to today, the navigator leads with it", async ({ page }) => {
  await boot(page);

  // Start the NT-in-90 schedule straight through the engine (the picker UI has
  // its own coverage); the chip and card are the subject here.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "nt-90", new Date().toISOString());
  });

  // The chip appears with day 1. Its label (aria-label — the chip is icon-only)
  // is the plan's own answer, so pin the expected target from the same wire
  // the chip reads.
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  await expect(chip).toBeVisible({ timeout: 10_000 });
  await expect(chip).toHaveAttribute("aria-label", /Day 1 · /);

  const first = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const plans = await s.fetchQ("plans", "");
    return plans.running.find((p: any) => p.id === "nt-90").today.chapters[0];
  });

  // Tap → today's first unread chapter, in the reader.
  await chip.click();
  expect(await page.evaluate(() => {
    const p = (window as any).__plumbline.panes[0];
    return { book: p.book, chapter: p.chapter };
  })).toEqual({ book: first.book, chapter: first.chapter });

  // The passage navigator leads with the today card; its chapter buttons GO.
  await page.locator(".pane .nav .passage").first().click();
  const card = page.locator('[data-surface="plan-today"]');
  await expect(card).toBeVisible();
  await expect(card).toContainText("Day 1");
  await expect(card).toContainText("The New Testament in 90 days");
  await card.getByRole("button", { name: first.display, exact: false }).first().click();
  await expect(card).toHaveCount(0); // the dialog closed with the navigation
  expect(await page.evaluate(() => (window as any).__plumbline.panes[0].chapter)).toBe(first.chapter);

  // In concept-study mode the chip stands down: the tracker is suspended, so
  // schedule reading in the mode would earn no credit.
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("grace");
  });
  await expect(page.locator(".plan-chip-row")).toHaveCount(0);
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  await expect(chip).toBeVisible();
});

// The chronological plan rides its curated table (decision #4) through the
// whole stack: the table ships in the pack, the engine offers the row only
// because it loads, and day 1 of the walk starts at Genesis 1. Dies if the
// table falls out of the pack (the row disappears), if the loader breaks (a
// start errors), or if the walk's head stops being Gen 1 (a scrambled table).
test("the chronological plan is offered, starts, and day 1 begins at Genesis 1", async ({ page }) => {
  await boot(page);

  // The picker OFFERS the row — which it only does when the shipped pack's
  // table actually loads (the engine filters unbuildable table plans out).
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

  // And it rides the reader like any schedule: the chip names day 1. The chip
  // lives on the READ screen, and Plans is a destination that replaces the
  // reader — so go back the way the ‹ does before looking for it.
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(page.locator(".plan-chip-row .plan-chip").first()).toHaveAttribute("aria-label", /Day 1 · /);
});

// The UAT round (2026-08-11). Three separate ways the plans surfaces misled a
// reader, each with its own cause:
//
//   1. The picker kept offering the other whole-Bible plans while one ran, so
//      the only thing a tap could mean was "throw away the plan I'm on".
//   2. The chip's label named the day's WHOLE span all evening ("Gen 1–4"
//      after Genesis 1 was finished), so it looked like nothing had counted —
//      the label now names what is left (`remaining()` in planToday.ts).
//   3. Explore's cards spilled their second line past the border at every text
//      scale, because the global 44px tap floor replaces the automatic minimum
//      size that stops a grid item being shorter than its own text.
test("a running plan takes its class off the picker", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "bible-365", new Date().toISOString());
  });
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));

  // The running plan's own card is there…
  await expect(page.locator(".plan-card", { hasText: "The whole Bible in a year" })).toBeVisible();
  // …and its rivals are gone from the picker entirely, not merely disabled.
  for (const rival of ["The whole Bible in 180 days", "The whole Bible in 90 days", "The Bible in chronological order"]) {
    await expect(page.locator(".plan-builtin", { hasText: rival })).toHaveCount(0);
  }
  // Another class is still on offer — this is a class filter, not a blanket one.
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

  // Read the chapter the way a phone does: dwell ticks into the core's tracker,
  // whose banked report lands as `readingWrote` — NOT an authoring write. The
  // authoring path always invalidated the plans cache; the dwell path is the
  // one the UAT caught stale, so it is the one this test must travel.
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

  // The chip now names what is LEFT — a label that never moves all evening is
  // the UAT bug — and sends the reader to the next chapter.
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

// Finishing the day's worth ADVANCES the chip to the next day's portion — it
// does not stand down. (It used to: maintainer UAT 2026-08-12 asked for the
// disappearing chip, and UAT 2026-08-18 reversed it — "it should show you the
// next X verses to read, people want to be able to work ahead".) `doneToday`
// stays on the wire — the Study hub's band uses it for its "done" line — so
// this also dies if it falls off.
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

  // Read every chapter of the day (reached past the last verse, dwell enough
  // for the words — the completed pass the tracker would have banked).
  await page.evaluate(async (chapters) => {
    const s = (window as any).__plumbline;
    for (const c of chapters) {
      const out = await s.rpc.call("readingRecord", c.book, c.chapter, 999, 3600, new Date().toISOString());
      if (!out?.completed) throw new Error(`chapter did not complete: ${c.book} ${c.chapter}`);
    }
  }, today);

  // The wire says the day was done today…
  await expect
    .poll(async () =>
      page.evaluate(async () => {
        const plans = await (window as any).__plumbline.fetchQ("plans", "");
        return plans.running.find((p: any) => p.id === "nt-90").doneToday;
      }),
    )
    .toBe(true);
  // …and the chip shows DAY 2 — the next portion, ready to be read ahead —
  // rather than disappearing until midnight.
  await expect(chip).toBeVisible();
  await expect(chip).toHaveAttribute("aria-label", /Day 2 · /);
});

// Pause sets a plan aside WHOLE: the chip stands down, the card says when the
// run began (name + start day is its identity), and Resume brings it back with
// nothing lost. Dies if the endpoint drops, if todayPlans stops filtering
// `paused`, or if the Plans card loses the Pause/Resume controls.
test("a paused plan asks nothing until it is resumed", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "bible-365", new Date().toISOString());
  });
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  await expect(chip).toBeVisible({ timeout: 10_000 });

  // Pause from the Plans screen, where the plan lives.
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  const card = page.locator(".plan-card", { hasText: "The whole Bible in a year" });
  await card.getByRole("button", { name: "Pause" }).click();
  await expect(card).toContainText("paused");
  await expect(card).toContainText(/Started /); // the run's identity: name + start day
  await expect(card.getByRole("button", { name: "Pause" })).toHaveCount(0);

  // Back in the reader, the PLAN chip is gone — a paused plan asks nothing.
  // The row itself may stand: it is the bookmarks strip now (2026-08-24), and
  // the seating tiles are not the plan's to take down.
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(page.locator(".plan-chip-row .plan-chip")).toHaveCount(0);

  // Resume: the chip returns, same day, nothing lost.
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  await card.getByRole("button", { name: "Resume" }).click();
  await expect(card.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(page.locator(".plan-chip-row .plan-chip").first()).toHaveAttribute("aria-label", /Day 1 · /);
});

test("no Explore card spills its text past its border, at any text scale", async ({ page }) => {
  // A phone's width is where the UAT saw it — desktop is wide enough for the
  // card text to fit even squashed, which would let the test pass over the bug.
  await page.setViewportSize({ width: 360, height: 740 });
  await boot(page);
  for (const scale of [1, 1.4]) {
    await page.evaluate((z) => {
      (window as any).__plumbline.config.uiScale = z;
      document.documentElement.style.setProperty("--uiScale", String(z));
    }, scale);
    // Mount the Explore screen the way Shell's Study destination does — the
    // navigation path has its own coverage; the subject here is the cards.
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
