# pure-study — TODO

## Rendering lens — seeing translation decisions

*Requested 2026-07-15. **New invention — not in `../overlay`** (first feature
with no reference implementation; design freely, but the parity contract still
applies). Goal: give readers without Greek/Hebrew a lens on where the
translators made choices. Select any tagged English word and see (a) the
underlying G/H word and (b) every other English rendering of that word across
the corpus, with counts and navigation — e.g. tap "charity" in 1 Cor 13 and
learn G26 agape is elsewhere rendered "love", "beloved", "feast of charity";
tap "love" in John 21 and see the agape/phileo distinction the English hides.
No new dataset — derived entirely from the tagged text we already ship.*

### Two directions, one corpus pass

Both indexes fall out of a single fold over the corpus, sibling to
`OccurrenceIx::build` in `crates/core/src/strongs.rs`:

- **Renderings index** (code → renderings): Strong's ref → map of normalized
  English rendering → occurrence list (VRef + token span). A *rendering* is
  the contiguous run of same-code tokens within a verse, so one-to-many
  translations ("suffereth long" ← G3114) stay one unit, exactly like the
  multi-code/zero-code handling already in the token schema.
- **Reverse index** (word → codes): normalized surface word → the codes it
  translates, with counts — the "love hides both agape and phileo" direction.
  The word-index fold in `crates/core/src/search.rs` (~line 52) already
  lowercases every token surface; hang both indexes off one pass.

### Tasks

- [ ] **Core** (`crates/core`): the two indexes above + a normalization fold
      (lowercase, letters-only). Start with exact surface forms
      ("love"/"loved"/"loveth" stay distinct); folding inflections together
      via the morphology data is a later refinement, likely a display-time
      grouping rather than an index change.
- [ ] **Wire** (`crates/ffi`): additive camelCase endpoints, e.g.
      `pure_engine_renderings_json(code)` → `[{rendering, count, refs:[…]}]`
      and `pure_engine_word_codes_json(word)` → `[{code, count}]`; regenerate
      bindings; rebuild release DLL.
- [ ] **UI — word study panel**: a **RENDERINGS** tier under each Strong's
      code (Full mode; between the occurrence count and the bridge tier):
      rendering chips with counts, the tapped word's own rendering
      highlighted; clicking a chip opens the concordance filtered to that
      code + rendering (respect OCC_SHOWN cap). When the reverse index shows
      the tapped surface word maps to >1 code, a small "also renders …" line
      makes the split visible without leaving the panel.
- [ ] **Parity**: GTK + WinUI in the same change set; log the Compose delta
      in docs/FEATURE-MANIFEST.md; add the feature's manifest section.
- [ ] **Tests**: index unit tests in `strongs.rs` style (small inline corpus
      covering contiguous-run grouping, multi-code tokens, case folding);
      FFI round-trip test in `crates/ffi/src/tests.rs`.

### Design notes

- The 1890 dictionary's `kjv_def` field lists renderings but is static,
  count-free, and occasionally wrong for our text — derive from the corpus,
  use `kjv_def` at most as a sanity cross-check in tests.
- Punctuation/casing: normalize for grouping but display the most common
  actual surface form as the chip label.
- FLAG_ADDED (italic) words carry no tags and never enter either index.
- Once the Luther 1912 tagging lands, the same indexes over the German corpus
  give cross-translation rendering comparison for free — worth keeping the
  index API corpus-parametric rather than KJV-global.

### Follow-ups (from testing 2026-07-16)

- [ ] **Reverse links must land on a Strong's study card, not a bare list.**
      Clicking an "'love' also translates G5368" link currently opens `occ:`
      (a verse list) for a code the reader doesn't understand. It should open
      the actual Strong's entry (definition / lemma / gloss + its study). Build
      ONE reusable code-study view — extract the per-code block from the
      word-study panel (WinUI `StudyPanel.ShowWordStudy` loop; GTK
      `word_study_markup` loop) behind a new `code:CODE` link verb — and point
      the reverse links at it. Reuse that one view everywhere instead of ad-hoc
      surfaces. Both shells, same change set.
- [ ] GTK window-icon wiring still pending (WinUI is wired). Install the
      bundled SVG in a hicolor layout named after `APP_ID` (`dev.purestudy.app`)
      under `apps/desktop/assets/icons/` and add the theme search path in
      `build_ui` so the window/taskbar shows the woven cross. CI-validated only.

## Authority tiers — provenance marks on evidence

*Requested 2026-07-15. Port overlay's three-tier trust model so every piece of
evidence in the study panel shows where it comes from, with a distinct icon per
tier — the reader always knows the provenance of what they're looking at.*

- overlay `Overlay/Bridge.hs`: `Tier` = `TierGod` (the text itself — TR /
  Masoretic words, and scripture-quotes-scripture, which inherits the text's
  own authority), `TierHuman` (curated scholarship — lexicons, the 1769
  translators' renderings, TSK), and a machine/analytical tier (embeddings and
  the R&D layer; the default for an unrecognized source, so nothing
  over-claims). `sourceTiers`, `sourcePriors`, `sourceLabel`.
  `Overlay/Panels.hs` draws the provenance marks (`provIcon`).
- What we already have: the trust **priors** are ported (`crates/rnd/src/
  bridge.rs` `Priors`, `data/source-priors.json`). NOT ported: the `Tier`
  classification, `sourceTiers`/`sourceLabel`, and the provenance icon marks.
- [ ] Port `Tier` + `sourceTiers`/`sourceLabel` to core (or pure-rnd) and
      expose each evidence item's tier(s) via an additive FFI field.
- [ ] Render a tier mark beside evidence in both shells' study panels (bridge
      partners, similar concepts, etc.): God-tier (a cross fits the app),
      Human-tier, Analytical-tier. Needs an icon set — identify/relicense the
      pack overlay used (named glyphs like `info-circle-muted`) or draw our own.
- [ ] Parity: both shells in one change set; log the Compose delta in
      FEATURE-MANIFEST; add a small legend so the marks are learnable.

## AI-generated Strong's tagging for Luther 1912

*Direction approved 2026-07-15. Goal: produce our own word-level Strong's
tagging for a German Bible by LLM alignment — license-clean, quality-measured,
shippable in the data pack — instead of adopting the encumbered community
modules.*

> **Reminder — first moves, before any bulk spend:**
> 1. *Afternoon spike:* hand-build a ~10-verse Luther jsonl (same header
>    schema, stamp `lut1912-tok1`) and load it through
>    `pure_engine_open_from_bytes` — the loader ignores `format` and accepts
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

## Weave coverage for allusive books

*Observed 2026-07-15: 17 books had zero weave endpoints (Esth, Eccl, Song,
Lam, Jonah, Nah, Zeph, Col, 1-2Thess, Titus, Phlm, 1-3John, Jude, Rev; Josh/
Job/Ezek/Dan near-zero). Cause: the harvested weaves track verbatim-quotation
density, so books that allude without quoting (Revelation <-> Daniel/Ezekiel/
Zechariah above all) fall through.*

- [ ] Revive the deferred **cross-testament quotation/allusion detection**
      R&D tier with an allusion-sensitive method (n-gram + embedding hybrid),
      aimed specifically at Revelation's OT spine.
- [ ] A first hand batch of widely-accepted suggestions landed in
      `weaves/suggested/` (2026-07-15); Song of Solomon and Philemon left
      empty deliberately -- Song's typological readings are tradition-specific
      rather than verse-level consensus, and Philemon has no OT parallels.
