#!/usr/bin/env node
// Compile the chronological plan's curated order
// (data-prep/chronological/order.json) into data/chronological.json — the
// table `plumbline_core::plan::load_table` reads (READING-PLANS.md decision
// #4). Chapter counts come from the CORPUS, not a table here, so the source
// can name whole books and a count can never drift from the shipped text.
//
// The one thing this script exists to guarantee: the expanded sequence covers
// every canon chapter EXACTLY once. A curated order with a hole or a repeat
// fails the build loudly instead of shipping a plan that silently skips
// scripture (a repeat would also double-bill a day's reading).
//
//   node scripts/build-chronological.mjs

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const src = JSON.parse(readFileSync(join(repo, "data-prep/chronological/order.json"), "utf8"));

// Chapter counts per book, from the corpus itself (line 1 is the header).
const counts = new Map();
for (const line of readFileSync(join(repo, "data/kjv.jsonl"), "utf8").split("\n").slice(1)) {
  if (!line) continue;
  const v = JSON.parse(line);
  if ((counts.get(v.b) ?? 0) < v.c) counts.set(v.b, v.c);
}

// Expand the curated entries into [book, first, last] segments.
const segments = [];
for (const entry of src.order) {
  const [book, first, last] = typeof entry === "string" ? [entry, 1, counts.get(entry)] : entry;
  if (!counts.has(book)) throw new Error(`unknown book: ${book}`);
  if (!(first >= 1 && last >= first && last <= counts.get(book)))
    throw new Error(`bad span: ${book} ${first}-${last} (book has ${counts.get(book)} chapters)`);
  segments.push([book, first, last]);
}

// Exactly-once coverage of the whole canon.
const seen = new Set();
for (const [book, first, last] of segments) {
  for (let c = first; c <= last; c++) {
    const key = `${book} ${c}`;
    if (seen.has(key)) throw new Error(`chapter appears twice: ${key}`);
    seen.add(key);
  }
}
let total = 0;
for (const [book, n] of counts) {
  for (let c = 1; c <= n; c++) {
    total++;
    if (!seen.has(`${book} ${c}`)) throw new Error(`chapter missing from the order: ${book} ${c}`);
  }
}
if (seen.size !== total) throw new Error(`coverage mismatch: ${seen.size} ordered vs ${total} in canon`);

const out = { format: "plumbline-plan-table-v1", id: "chronological", days: src.days, segments };
const path = join(repo, "data/chronological.json");
writeFileSync(path, JSON.stringify(out));
console.log(`chronological: ${segments.length} segments, ${seen.size} chapters, ${src.days} days → ${path}`);
