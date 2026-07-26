# Plumbline — TODO

The backlog — the only planning doc. Re-cut 2026-07-25: **two shells only —
the Android app (Compose, the UX gold standard) and the PWA** (GitHub Pages +
a signed APK on GitHub Releases). Every desktop port, store submission, and
code-signing errand is dropped, and the paid sync service is cancelled — the
product is simply free. What remains is ordered by what makes the two shells
better daily drivers.

Every data pack suggested below is public domain or CC-BY 4.0 — nothing
encumbered ships, ever (the Luther-tagging section documents why we build our
own tagging layers rather than adopt unvetted community modules).

House rules: shell parity in one change set (or a logged delta in
[docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md)), additive camelCase
wire, frozen formats untouched. Item numbers are stable IDs — new items
append, nothing renumbers; dropped items stay listed under Retired so the
history reads.

## Landed

- [x] **1–8. Tier-0 daily-driver gaps** (2026-07-22) — copy/context menu,
      history, margin notes, highlighting, dark/night themes, warm indexes,
      guide/shortcuts/about, small unifications.
- [x] **9. Android (Compose).** Shipped and iterating on-device (v0.5.x on the
      GrapheneOS foldable); now the UX gold standard the web follows.
- [x] **15. Memorization** — first-letter drills, typed recall, SM-2, coverage
      map + activity, in both shells. (Printable flashcards wait on #14.)
- [x] The PWA itself — Svelte over the wasm32-wasip1 engine, installable,
      offline after first visit, deployed to GitHub Pages on every `v*` tag.

## Now

- [ ] **28. PWA mobile performance.** The phone boot was unusable (2026-07-25):
      the web pack shipped ~16 MB of R&D artifacts Android doesn't bundle,
      `engine_open` parsed all of it eagerly, everything on the browser main
      thread, plus a concept/leitwort/SIF warm-up on *every* boot.
      - [x] **Concept warm made ~3× cheaper** (2026-07-25): the co-occurrence →
            PPMI → kNN → communities pipeline now runs in interned id space
            (890 ms → 270 ms native; benefits Android's warm too). Timing
            harness: `cargo test --release -p plumbline-ffi timing_harness --
            --ignored --nocapture`.
      - [x] **Pack split** (2026-07-25): boot fetches the core pack only
            (5.2 MB gz / 30.8 MB raw, was 8.9/46.8 — the same set the APK
            bundles); morphology + concept vectors are `rnd`-marked and
            stream in after first paint (`loadRndPack` →
            `plumbline_engine_load_rnd_data` → re-warm builds the SIF),
            triggered at idle, from the first-run machine choice, or the
            Settings toggle. Decision #3 finally applied to the web; no
            user-agent sniffing.
      - [x] **Engine off the main thread** (2026-07-26, branch
            `perf/engine-worker`). As built:
            1. `engine.worker.ts` hosts the WHOLE engine life: pack fetch,
               IDB home, wasm instantiate, open, warm, deferred R&D load,
               authoring writes + persistence. Fonts via `self.fonts` +
               OffscreenCanvas `measureText` for the layout measure callback.
               Splash progress + studyEpoch bumps arrive as messages.
            2. A postMessage RPC proxy exposes the same StudyEngine method
               names returning Promises ({id, method, args} → {id, result}).
            3. The shell keeps its synchronous `$derived` graph via a
               **read-through reactive cache**: `q(method, ...args)` returns
               the cached value (or null on first ask) and fires the async
               fill; the response invalidates a version signal so deriveds
               re-run. Panels already tolerate `?.blocks` = null.
            4. Layout: the worker returns the display-list JSON (items +
               height), cached by (book, chapter, cfg). **Hit-testing moves
               to TS** over the item rects (a simple scan) so hover/tap
               never round-trips.
            5. e2e: `window.__plumbline.engine.*` becomes async — tests
               await; the boot-responsiveness tests are the guard that the
               main thread stays free.
      - [ ] Remaining boot levers if the phone still wants more: persist the
            open-time indexes (renderings/search/occ — ~330 ms native) via the
            idxcache pattern, and/or intern the renderings build like concept.
            The worker now hands the UI over BEFORE the analytics warm, so
            the splash ends at engine-open, not warm-end.
- [ ] **29. Multilingual program** — promoted 2026-07-25 (the maintainer's
      pick): see **AI-generated Strong's tagging** below. Start with the
      afternoon spike (hand-built 10-verse Luther jsonl through
      `plumbline_engine_open_from_bytes`), then the Ruth + 1 John pilot.
- [ ] **21. Quotation/allusion detection.** 17 books still have zero weave
      endpoints — Revelation, the most allusive book in the canon, is dark.
      Ambient connectors are the crown jewel; coverage here *is* product
      quality. (See "Weave coverage for allusive books" below.)

Then the feature queue, in order — every external source PD or CC-BY:

- [ ] **16. Finish grammar search; add the power tier.** `tense:aorist` is an
      honest placeholder; the morphology is shipped and parsed. Then scope
      filters (`in:Psalms`, `ot:`/`nt:`), case-exact + **divine-name search**
      (FLAG_DIVINE), **italics search** (`added:` — translator-supplied words,
      a uniquely KJV discipline with no good tool anywhere), boolean/NEAR,
      history, saved searches. *No new data.*
- [ ] **18. Harmony mode.** Robertson's *Harmony* (1922, **PD**) as a curated
      weave pack + "follow the weave" pane lockstep by link pairs. "Read all
      four Gospels as one" — ~90% built already.
- [ ] **14. Print & PDF export.** A PDF measure/paint pass over the existing
      display list — print typeset exactly like the screen. Chapter handouts,
      large-print passages, memorization flashcards (#15). No Bible app free
      or paid prints beautifully; this architecture can, cheaply.
- [ ] **17. Interlinear-lite → original-language pack.** Phase 1 *needs no new
      data* (strongs.json + morphology.jsonl are token-keyed). Phase 2: WLC +
      Robinson's TR text packs (**both PD**; import pipeline exists) for a
      true reverse-interlinear.
- [ ] **24. Command palette (Ctrl+K).** Every action already routes through
      the URI verb table + search; this is the discoverability answer, cheap.
- [ ] **22. Reading plans — quiet ones.** M'Cheyne (1842, **PD**), canonical,
      chronological. A chip — "Day 37 · Ps 119 ▸" — no streaks, no guilt.
- [ ] **23. Read-aloud (TTS) with word-level highlighting.** Platform TTS
      driving the per-word display list — and the honest answer to
      accessibility: canvas-drawn text is invisible to screen readers today.
      PD human audio (LibriVox KJV) as an optional pack later.
- [ ] **19. People & places.** TIPNR (**CC-BY 4.0**) already ships identities;
      upstream adds unique person IDs + relationships. Six Marys, four
      Herods — nobody free does this inline. Places + offline maps
      (openbible.info geodata, **CC-BY**) as a pack after.
- [ ] **20. Corpus-wide leitwort browser.** Port overlay `Burst.hs` corpus-wide:
      a browsable index of the canon's repeated motifs. *No new data.*
- [ ] **27. Quiet update check.** Manual "check for updates" against GitHub
      releases — matters for sideload users; no auto-update, no phoning home.

## Later

- [ ] **25. Weave commons.** Export/import affordances + a community repo
      where PR review mirrors the in-app `approved` ethic. Ship more curated
      threads in-box (Messianic prophecies, the Tabernacle, prayers of the
      Bible — an afternoon each).
- [ ] **26. Docs & showing the depth.** The guide with GIFs (constellation,
      connectors, renderings lens) — motion sells what prose can't. The PWA's
      own page on GitHub Pages is the product's web presence; no separate
      website.

## Retired (2026-07-25)

Numbering preserved; none of these come back without a new decision:

- ~~**10/10b. Linux packaging + publish**~~, ~~**11. Windows distribution
  (signing/winget/Store)**~~, ~~**13. macOS shell**~~ — no desktop ports; the
  PWA covers every desktop. No stores, no signing contracts.
- ~~**12. Website**~~ — the hosted PWA is the website.
- ~~**Premium sync service**~~ — cancelled; the product stays entirely free.
  The data-model discipline it imposed (stable ids, no host-local
  assumptions, exportable single-file JSON) stays — it's just good design.
- ~~Engineering: split `apps/desktop/src/main.rs`~~ — the desktop shells were
  retired and deleted 2026-07-25; nothing to split.

---

# Engineering & data work

## AI-generated Strong's tagging (the multilingual program, #29)

*Direction approved 2026-07-15; promoted to Now 2026-07-25. Goal: produce our
own word-level Strong's tagging for non-English Bibles by LLM alignment —
license-clean, quality-measured, shippable in the data pack — instead of
adopting the encumbered community modules. German (Luther 1912) first.*

> **Reminder — first moves, before any bulk spend:**
> 1. *Afternoon spike:* hand-build a ~10-verse Luther jsonl (same header
>    schema, stamp `lut1912-tok1`) and load it through
>    `plumbline_engine_open_from_bytes` — the loader ignores `format` and accepts
>    any tokenization stamp, so German text renders in the reader today. This
>    proves the display path end to end.
> 2. *Pilot before corpus:* run Ruth + 1 John (~190 verses) through the full
>    prompt → verify → adjudicate loop and **measure the error rate** (see
>    QA protocol below) before submitting the 31k-verse batch.

### Background (why build our own)

- The existing Strong's-tagged Luther 1912 modules are of unknown provenance;
  CrossWire pulled its tagged GerLut module in 2007 over a copyright claim on
  the **tagging layer** (the text itself is public domain). Not shippable
  without vetting we can't do.
- Schlachter 2000 is © Genfer Bibelgesellschaft — can't ship at all.
- eBible.org's `deu1912` (Luther 1912) is PD but **untagged** — perfect source
  text, no tags.
- An alignment we generate from PD inputs is our own work product: clean to
  ship, and it comes with per-token confidence scores, which no community
  module has.

### The key insight: selection, not generation

We are **not** asking a model to assign Strong's numbers from scratch. Both
source texts are already fully tagged and already in our pipeline:

- OT: OSHB / WLC, every Hebrew word tagged (CC-BY 4.0) — feeds morphology.jsonl today
- NT: Robinson's Textus Receptus tagging, every Greek word tagged (PD) — same

So the per-verse task is: **map each German token to codes drawn from that
verse's source-code inventory** (a closed set of ~10–40 codes). The model
selects, never invents. Verses are pre-aligned as parallel pairs. This is what
makes it both tractable and mechanically verifiable.

### Phases

- [ ] **0. Versification map (Luther ↔ KJV)** — needed for the app regardless.
      Note: Luther follows MT numbering (psalm superscriptions are v1), so
      German ↔ WLC is near-identity in the OT; the KJV remap is the deviation.
      Align in native numbering, remap at import time.
- [ ] **1. Import the German text** — run the overlay SWORD importer
      (external `../overlay` checkout) over eBible `deu1912` → German verse
      tokens in our jsonl schema, stamped e.g. `lut1912-tok1`.
- [ ] **2. Pilot the prompt + schema** — per-verse call: German tokens
      (indexed) + source tokens (code, lemma, gloss, morph). Output via
      structured outputs (`output_config.format`, strict schema): for each
      German token index an array of source codes (possibly empty) +
      confidence + note. Codes constrained to the verse's inventory.
      Pilot on Ruth + 1 John (~190 verses), iterate until clean.
- [ ] **3. Bulk run** — 31,102 requests through the Message Batches API
      (50% off, one batch allows 100k requests; most finish within an hour).
      Shared instruction block marked `cache_control` (cache hits in batch are
      best-effort — keep the instruction prefix modest either way).
- [ ] **4. Mechanical verification suite** (cheap, no model calls):
  - Coverage: every source content-word claimed by ≥1 German token; flag
    unclaimed source words and untagged German content words.
  - Consistency matrix: German-lemma ↔ Strong's co-occurrence across the whole
    corpus; flag low-probability assignments.
  - Independent second opinion: statistical aligner (eflomal / fast_align) on
    the same verse pairs — CPU, minutes for the whole corpus. Disagreements go
    to the adjudication queue.
  - Optional third vote: pivot through the KJV (German↔English alignment
    composed with our English→Strong's tags).
- [ ] **5. Adjudication pass** — re-run flagged/disagreement verses (expect
      5–10%) on an Opus-tier model with the verifier evidence in context.
- [ ] **6. Human QA + measurement** — hand-check a random sample (~300–500
      token alignments) for an error-rate estimate; use the reader itself as
      the review surface (Ctrl+click a word → Strong's card shows the tag).
      The community-tagged module may be used as a *private eval benchmark
      only* — never shipped, never derived from.
- [ ] **7. Ship + integrate** — emit the German jsonl with inline `strongs`
      per token (existing schema handles multi-code and zero-code tokens),
      provenance-flagged as machine-generated with corrections accumulating in
      `patches/` — same ethos as the weave `approved` flag. Dovetails with the
      de-hardcoding work from the German-support assessment (corpus filename,
      `TOKENIZATION_VERSION`, book-name aliases, "Johannes 3,16" ref parsing).

### Methodology recommendations

**Prompt design (phase 2)**
- Per-verse context: indexed German tokens; source tokens each with Strong's
  code, lemma, transliteration, short gloss, and morph code; plus the tagged
  KJV rendering of the same verse as a bridge hint (we already ship it — a
  cheap, strong disambiguation signal).
- Keep the instruction block stable and modest (~1k tokens) so it prompt-caches;
  all per-verse content goes after it.
- Include 3–5 hand-aligned few-shot examples in the instructions, chosen to
  cover the hard shapes: a separable verb, a compound, an untranslated
  particle, a one-to-many rendering.
- Output: for each German token, an array of codes + a confidence (0–1) + a
  note only when uncertain. Enforce with a strict structured-output schema
  whose code values are enumerated per request from the verse's inventory —
  invention impossible by construction.

**Decision policy (phases 3–5)**
- Auto-accept where the LLM and the statistical aligner agree and
  confidence ≥ 0.8.
- Queue for Opus adjudication: LLM/aligner disagreements, confidence < 0.8,
  coverage violations, consistency-matrix outliers.
- The adjudicator sees both proposals plus the verifier evidence. If it still
  hesitates, the token keeps an `uncertain` flag and renders as unverified in
  the app until a human approves it (via `patches/`).

**Statistical verifier (phase 4)**
- eflomal in both directions (de→source, source→de), symmetrized
  (grow-diag-final-and); count only content-word links as votes.
- Consistency matrix: P(code | German lemma) over the whole corpus; flag any
  assignment below ~1% probability for lemmas with ≥5 occurrences.

**QA protocol (phase 6)**
- Stratified random sample of ~400 token alignments (OT/NT × prose/poetry ×
  short/long verses), graded blind against the sources — don't look at the
  model's confidence while grading.
- Ship bar: <2% error on content words in the auto-accepted set. The
  adjudicated set may be looser but must stay visibly flagged in-app.
- The community-tagged module may serve as a disagreement detector during
  eval only — never copy an alignment from it.

**Runbook mechanics (phase 3)**
- Message Batches API with `custom_id` = the verse ref key (`"Gen 1:1"`);
  results arrive in any order — key by `custom_id`, never by position.
- One batch covers the corpus (31k ≪ 100k cap). Keep the raw batch results on
  disk; the pipeline must be resumable from them without re-spending.
- Log every errored/refused/non-conforming response and re-queue those verses
  individually.

### Cost estimate (Batches API, prices as of 2026-06)

Assumptions: ~1.0–1.5k input tokens/verse (shared instructions + German verse
+ tagged source verse), ~150–400 output tokens/verse → ~40M in / ~8M out for
the whole corpus.

| Model | Batch $/MTok (in/out) | Full-corpus pass |
|---|---|---|
| Haiku 4.5 | $0.50 / $2.50 | ~$40 |
| Sonnet 5 (intro pricing thru 2026-08) | $1.00 / $5.00 | ~$80 |
| Sonnet 5 (standard) | $1.50 / $7.50 | ~$120 |
| Opus 4.8 | $2.50 / $12.50 | ~$200 |

Recommended: **Sonnet 5 for the bulk pass, Opus 4.8 for the adjudication
queue** → roughly **$100–150 all-in**. Even two full independent passes for an
ensemble stay under ~$300. Prompt-caching the instruction block trims input
cost further when hits land.

### Known hard spots

| Risk | Handling |
|---|---|
| Luther/TR textual divergences (readings Luther translates that aren't in Robinson's TR) | Flag, leave untagged, record a note — a few hundred spots expected |
| German separable verbs (`ging … auf` ← one Greek verb) | Both parts carry the same code; per-token `Vec<String>` schema already supports it |
| Compounds / many-to-one, one-to-many renderings | Multi-code tokens + empty-code tokens, same as KJV function words |
| Psalm superscriptions (`FLAG_TITLE` vs numbered v1) | Handled in the versification remap (phase 0) |
| Model inventing codes | Impossible by construction — strict schema constrains to the verse's inventory |

### Payoff beyond German

The pipeline is language-generic: rerun it for Spanish RV 1909, French Segond
1910, Russian Synodal, etc. It changes the requirement for supporting a
language from "a Strong's-tagged translation exists (and is license-clean)"
to "any public-domain MT/TR translation exists" — which is nearly always true.

### Dependencies

- The `../overlay` checkout (SWORD → jsonl importer) for phase 1.
- An Anthropic API key for phases 2–5; eflomal or fast_align (CPU) for phase 4.

## Weave coverage for allusive books (#21's data side)

*Observed 2026-07-15: 17 books had zero weave endpoints (Esth, Eccl, Song,
Lam, Jonah, Nah, Zeph, Col, 1-2Thess, Titus, Phlm, 1-3John, Jude, Rev; Josh/
Job/Ezek/Dan near-zero). Cause: the harvested weaves track verbatim-quotation
density, so books that allude without quoting (Revelation <-> Daniel/Ezekiel/
Zechariah above all) fall through.*

- [ ] Revive the deferred **cross-testament quotation/allusion detection**
      R&D tier with an allusion-sensitive method (n-gram + embedding hybrid),
      aimed specifically at Revelation's OT spine.
- [x] A first hand batch of widely-accepted suggestions landed in
      `weaves/suggested/` (2026-07-15); Song of Solomon and Philemon left
      empty deliberately -- Song's typological readings are tradition-specific
      rather than verse-level consensus, and Philemon has no OT parallels.
