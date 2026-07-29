import { test } from "@playwright/test";

// Registers the backup-zip cases from zip-cases.ts with the Playwright runner
// so CI covers them. They need no browser — zipRead is a pure function, and it
// is tested as one; the split keeps the cases runnable by a bare `node` when
// mutation-testing a guard.
import { zipCases } from "./zip-cases";

for (const c of zipCases) test(`zip: ${c.name}`, c.run);
