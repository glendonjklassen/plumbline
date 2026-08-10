# Reading plans + the concept study — design spec

Decisions taken with the maintainer 2026-08-08; this document is the contract
for the build. Amend it when a decision changes — do not let the code drift
past it silently. (Amended 2026-08-09: the feature built as "speedrun" ships
as **Concept Study** — the maintainer's rename, applied through every layer
before anything froze: UI strings, the serialized `kind`, the ABI names, and
the identifiers.)

## Decisions in force

| # | Decision | Choice |
|---|----------|--------|
| 1 | Pacing | **Sequence-anchored.** "Today" is the next unfinished day of the plan. Absence accrues no backlog; a 365-day plan may take 14 months and that is fine. No streaks, no guilt — the reading map's own philosophy (the bloom is an invitation, not a debt). |
| 2 | Progress | **Derived from the reading tracker.** A plan day is done when every chapter it names stands `Read` in `core::reading` (dwell-gated, plus the existing long-press mark-read for paper reading). No second bookkeeping; reading done outside the plan counts. Prerequisite: the dwell gate tuned for fast readers (done — 500 wpm, complete at 85%). |
| 3 | Classes | **One active plan per class.** Plans carry a `class`; starting a plan in an occupied class offers to replace the running one. Classes: `wholeBible` (365 / 180 / 90, chronological), `newTestament` (NT in 90), `devotional` (Psalms + Proverbs monthly). Parallel plans across classes are the point (NT-90 + Psalms/Proverbs together). Concept studies are their own class-free kind — any number may exist, but see §Concept Study for why only one is *active in the reader* at a time. |
| 4 | Lineup v1 | **Generated set + chronological.** Word-weighted schedules generated in core off the cached `ChapterWords` (zero pack bytes, deterministic, works for the German corpus free since it shares verse addresses): Bible in 365/180/90, NT in 90, Psalms+Proverbs in a month. Chronological is a small curated table (book/chapter sequence, ~3 KB) built by a data-prep script into the pack's study stage. |
| 5 | Surfaces | **Explore ▸ Plans** (the eighth card) is the home: running plans with day cards + progress, a picker to start, stop-with-confirmation per house rules. The **passage navigator** shows the active plans' today; a **nav-strip chip** ("Day 12 · Gen 30–31", stacking "+1 more" when several plans run) rides the reader when any plan is active — tap goes to today's first unread chapter. Both shells, same change set. |
| 6 | Concept Study | A new plan **kind** with its own reader mode — see below. |

## The plan file

One file per running plan under `home/plans/` — personal study data, so it
rides in the backup zip; **add `plans/` to the backup path filter in both
shells and the zip layout doc, or restore silently drops it.**

```json
{
  "format": "plumbline-plan-v1",
  "id": "bible-365",            // stable id; custom plans get a fresh one
  "kind": "schedule",           // "schedule" | "conceptStudy"
  "class": "wholeBible",        // exclusivity group; absent on a concept study
  "generator": {"scope": "canon", "days": 365},  // regenerate, don't store, the schedule
  "table": null,                 // OR the curated table's pack id ("chronological")
  "started": "2026-08-08T12:00:00Z",
  "lang": "en",
  "done": [1, 2, 5],            // days completed OUT OF ORDER stay recorded
  "tag": null                    // concept study only: the preset tag name
}
```

- `generator` parameters, not the materialized schedule: the word-weighted walk
  is deterministic given the corpus, far smaller, and a custom-plan builder
  later stores the same shape.
- `done` is a cache of *derived* completion (decision #2): recomputing a day
  from the reading store must agree; the list exists so a day completed under a
  since-cleared reading record stays honoured. Sequence-anchoring means "today"
  = the lowest day index not in `done`.
- Timestamps and `lang` follow the provenance rules (I18N.md): stamped at
  create, never on re-save.

## Concept Study — a concept sweep with its own reader mode

The use case, verbatim: *"sometimes I want to study the entire Bible for a
concept and will skim passages. I then want to tag passages that are applicable
and study them together."* Tags → weave already exist; the concept study is
the sweep that feeds them.

- **Its own mode, clicked into.** Starting (or resuming) a concept study puts
  the reader in concept-study mode — a visible banner/chip names the run and
  its tag, and the mode persists until the reader switches back. Only one
  concept study is active in the reader at a time (the mode changes tap semantics; two at once
  would make a tap ambiguous).
- **The tracker is OFF while in the mode.** Skimming is not reading: the
  shell's `ReadingTracker` is suspended, so the reading map, and every
  schedule-plan derived from it, ignores concept-study time entirely.
- **Generous self-progress, no dwell.** A chapter counts as swept when the
  reader has been through it — high-water reaching the last verse, no dwell
  gate — or marks it swept by hand. Progress is chapters-swept over the run's
  scope (whole canon by default), painted with the same canon-dispersion
  language the coverage surfaces already use. **Non-linear by design**: there
  is no day sequence; sweep Revelation before Genesis.
- **Tap-to-tag.** In concept-study mode a verse tap tags the verse with the run's
  preset tag after a fast confirmation (the confirm button names the act —
  "Tag 'grace'" — per §Ask before destroying anything's naming rule, though
  this one creates rather than destroys). Word study is still reachable
  (long-press context menu), but the tap is the sweep's tool.
- **Exit and return.** Switching back to normal mode restores tap = word
  study and resumes the tracker. The run keeps its coverage; re-entering
  resumes where the map shows gaps.
- **Done.** A concept study has no finish line to enforce — the reader ends it from
  the Plans screen (confirmation; the tag and its members are untouched — the
  whole point is what was gathered). The tag card's existing ⇔ make-weave
  path is the handoff to study.

## ABI sketch (all additive; bindings regenerated per the house rule)

- `plumbline_engine_plans_json()` — available plan definitions (generated +
  curated) and the running plans with state.
- `plumbline_engine_plan_start(id_or_generator_json)` / `_stop(id)` — author
  endpoints, null on success.
- `plumbline_engine_plan_today_json(id)` — the day card: day index, chapter
  refs, per-chapter standing, done flag.
- `plumbline_engine_concept_study_start(tag, now)` / `_sweep(id, book,
  chapter)` — as built, the mode lives in the config (`conceptStudy`, the
  active run's id) rather than in engine state: the config is already the one
  thing every pane and both shells read, so a tap cannot mean two things.
- Blocks endpoints for the Plans screen cards, same producer pattern as the
  rest of the panel (`plumbline_core::panel` over `PanelSource`).

## Order of work

1. ~~Dwell tuning (500 wpm / 0.85)~~ — done with this spec.
2. `plumbline_core::plan`: generator, classes, plan file store, derived
   completion, the concept-study record. Unit tests over a toy corpus.
3. Chronological table: data-prep script + pack entry (study stage) + loader.
4. FFI endpoints + bindgen + the golden ABI tests.
5. Web shell: Plans screen, navigator card, nav chip, concept-study mode +
   tap-to-tag; e2e incl. the tracker-suspension assertion.
6. Android shell: same surfaces (`PlansScreen`, BookNav card, chip, mode);
   CI-built APK for on-device UAT per house rules.
7. FEATURE-MANIFEST section + backup filter updates in the same change set.
