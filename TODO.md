# pure-study — TODO

The approved feature roadmap from the product review of 2026-07-18 (first
half), followed by the in-flight engineering and data work (second half). The
product stays free — what God has given us in the KJV is far better than a
Porsche, and it is free — with a paid sync service as the only premium piece
(the workman's wages, never a feature gate).

House rules apply to every item here: shell parity in one change set (or a
logged delta in [docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md)),
additive camelCase wire, frozen formats untouched. Platforms: Linux, Windows,
macOS, Android — ARM and x86; no iOS.

## Tier 0 — daily-driver gaps (small, additive, do first)

- [ ] **1. Copy & context menu.** There is no clipboard anywhere in either
      shell. Left-click is taken (weave pinning), so: right-click a verse →
      context menu — copy verse · copy with reference (`…text — John 3:16
      (KJV)`) · copy chapter · tag · add to thread · note. Copy affordances on
      panel cards too. Formats: plain, ref-suffixed, markdown.
- [ ] **2. Back/forward history.** Study is a chain of jumps (concordance →
      verse → xref → …) with no way back. Per-pane history stack, Alt+←/→,
      mouse buttons 4/5. Everything already funnels through the core link
      router (`pure_core::panel::parse_link` / `pure_route_link_json`), so the
      push points exist today.
- [ ] **3. Personal margin notes.** The 1769 translators' notes are
      first-class; the user's aren't (notes exist only on
      threads/entries/tags/weaves — [tag.rs:66](crates/core/src/tag.rs#L66)).
      Per-verse notes in a `notes/` dir sibling to `tags/` (same atomic store,
      refKey-keyed, additive endpoints), a gutter mark beside the weave dots,
      a "your notes" panel section. Also the anchor of future sync value.
- [ ] **4. Highlighting — reuse tags, add color.** Additive `color` field on
      tags; members render as a soft wash behind the verse's line rects (the
      highlight-band mechanism already computes these). A fixed, muted palette
      of 5–6 tones tuned to the paper and the dark theme. The tags browser
      doubles as the highlight browser for free.
- [ ] **5. Dark + night themes, crafted like the light one.** `ForceLight`
      today ([main.rs:414](apps/desktop/src/main.rs#L414)). A candlelight-warm
      dark paper and a true-black night mode (OLED Android), designed with the
      same care as `#fcf9f4` — not an inverted afterthought. Define the palette
      tokens once (a small table/JSON every shell embeds at build) so shells ×
      themes can't drift. Follow-system + manual toggle, persisted in config.
- [ ] **6. Kill the first-study-click pause.**
      [GUIDE.md:23-24](docs/GUIDE.md#L23-L24) documents it honestly. When
      config says Full mode, warm the analytics indexes on a background thread
      at startup; consider extending the `*.idxcache` to cover more of it.
- [ ] **7. In-app guide, shortcuts overlay, About.** GUIDE.md and
      BIBLIOGRAPHY.md are excellent and invisible — no Help affordance in
      either header. `?`/F1 shortcut overlay; the guide rendered in-app; an
      About page stating edition, provenance, and the "yours forever, no
      account, no telemetry" covenant.
- [ ] **8. Small unifications.** Cross-book header stepping (WinUI has it,
      GTK clamps — manifest §Multi-pane): unify on cross-book. Paint **all**
      search hits in the visible chapter, not just the goto verse. Modifier-
      click a `go:` link → open in the *other* pane.

## Tier 1 — reach

- [ ] **9. Android (Compose).** The single biggest lever; already planned.
      The pack (~48 MB `data/` + `bridge/`) fits a Play asset pack;
      Simple-first onboarding matters more on phones; Kotlin binding gaps are
      manifest-tracked. Typography survives intact (core layout +
      `TextMeasurer`).
- [ ] **10. Linux packaging.** Flathub (Linux discoverability, full stop),
      AUR, AppImage — plus **ARM64 Linux** builds: a Raspberry Pi is a $70
      offline study machine (missions, low-connectivity, church labs).
- [ ] **11. Windows distribution.** `release.yml` already builds
      self-contained arm64/x64/x86 apps. Missing: **code signing** (SmartScreen
      scares exactly the non-technical people this serves — Azure Trusted
      Signing or SignPath OSS), a **winget** manifest, a **Microsoft Store**
      listing. Native ARM64 Windows is already a differentiator — say so.
- [ ] **12. A one-page website + quiet update check.** Screenshots (the
      constellation sells itself), downloads, the ethos statement. In-app: a
      manual "check for updates" against GitHub releases — no auto-update, no
      phoning home.
- [ ] **13. macOS shell.** The portable crates already build on macOS and the
      data home already resolves `~/Library/Application Support/pure-study`
      ([GUIDE.md:127-131](docs/GUIDE.md#L127-L131)) — only the shell is
      missing. SwiftUI/AppKit over the same C ABI with a CoreText-backed
      measure callback; the view-model consolidation has landed, so shell #4
      is paint-and-route. Developer ID signing + notarization, universal
      binary, notarized DMG + Homebrew cask (Mac App Store later if ever — its
      sandbox containerizes the file-based data home). Add a macos CI runner
      for the portable crates and a macOS delta section to the manifest.
- [ ] **14. Print & PDF export.** The core lays out via a measure callback; a
      PDF measure/paint pass gives print output typeset exactly like the
      screen, with no shell involved. Chapter handouts, large-print passages,
      memorization flashcards (#15). No Bible app, free or paid, prints
      beautifully — this architecture can, cheaply.

## Tier 2 — study differentiators

- [ ] **15. Memorization — first-letter mode + spaced repetition.**
      *Flagged top priority of the differentiators (2026-07-18).* Source = any
      tag or thread ("memorize this thread"). First-letter prompts,
      progressive blank-out, typed recall, SM-2 scheduling; printable
      flashcards once #14 lands. Include a **coverage map**: the canon
      strip/dispersion visual language reused to paint what you've spent time
      with — cells shaded by verses memorized and review depth/recency, the
      OT/NT divide marked, so a glance shows where your memory work has
      reached and where it hasn't. The scheduler's per-verse review history
      provides the data by construction. The KJV is *the* memorization text
      (homeschool, AWANA, Bible bees) and that world has no quality free tool —
      possibly the largest untapped audience. Progress lives in the "yours"
      dirs (→ sync later).
- [ ] **16. Finish grammar search; add the power tier.** `tense:aorist` is an
      honest placeholder ([README.md](README.md) §Limitations); the morphology
      is shipped and parsed. Then, all in core so every shell gets it at once:
      scope filters (`in:Psalms`, `ot:`/`nt:`, ranges); case-exact and
      **divine-name search** (FLAG_DIVINE is in the tokens); **italics search**
      (`added:` — translator-supplied words; FLAG_ADDED is in the tokens — a
      uniquely KJV study discipline with no good tool anywhere);
      boolean/NEAR; search history; saved searches.
- [ ] **17. Interlinear-lite → original-language pack.** Phase 1 needs no new
      data: an under-word toggle showing lemma/xlit/parse (strongs.json +
      morphology.jsonl are already keyed to tokens). Phase 2: optional WLC/TR
      text pack (both PD; import pipeline exists in overlay/data-prep) for a
      true reverse-interlinear — PLAN.md already notes cosmic-text gives RTL
      Hebrew for free. Offline and beautiful where the web tools are neither.
- [ ] **18. Harmony mode.** A curated Gospel-harmony weave pack (Robertson's
      *Harmony*, PD, importable) plus "follow the weave": panes align
      pericope-by-pericope as you scroll (Shift-lockstep exists; harmony mode
      locks by link pairs instead of pixels). "Read all four Gospels as one" —
      a headline feature that is ~90% built already.
- [ ] **19. People & places.**
      [bridge/stepbible-tipnr.json](bridge/stepbible-tipnr.json) already ships
      TIPNR identities; upstream TIPNR carries unique person IDs +
      relationships. A People browser and a chip on name-words: "Herod
      Antipas, tetrarch of Galilee — distinct from Herod the Great." Six
      Marys, four Herods, thirty Zechariahs — nobody free does this inline.
      Genealogies from TIPNR relations later; places + offline maps
      (openbible.info geodata, CC-BY) as a pack after that.
- [ ] **20. Corpus-wide leitwort browser.** Port overlay `Burst.hs` corpus-wide
      (PLAN.md marks it "later"): the per-word LEITWORT tier answers "does
      *this* word cluster?"; Burst answers it for every word at once — a
      browsable index of the canon's repeated motifs. Discovery, not just
      display.
- [ ] **21. Quotation/allusion detection — raise its priority.** Already
      tracked below (weave coverage for allusive books). Ambient
      connectors are the crown jewel and 17 books have zero weave endpoints —
      Revelation, the most allusive book in the canon, is dark. Coverage here
      *is* product quality.
- [ ] **22. Reading plans — quiet ones.** M'Cheyne, Horner, canonical,
      chronological (all PD). A chip — "Day 37 · Ps 119 ▸" — no streaks, no
      badges, no guilt mechanics. Plans live in the "yours" dirs (→ sync).
- [ ] **23. Read-aloud (TTS) with word-level highlighting.** Platform TTS
      (SAPI/OneCore, Android TTS, macOS AVSpeechSynthesizer; optional Piper
      voices on Linux) driving the existing per-word display list —
      karaoke-style highlight, chapter autoplay. Zero licensing risk, and it
      doubles as the honest answer to accessibility: canvas-drawn text is
      invisible to screen readers today. PD human audio (LibriVox KJV) as an
      optional pack later.
- [ ] **24. Command palette (Ctrl+K).** The discoverability answer: the depth
      is hidden behind Ctrl+click and small header buttons, and every action
      already routes through the URI verb table (manifest §Link routing) +
      search. Cheap now that the router consolidation has landed.

## Tier 3 — ecosystem & content

- [ ] **25. Weave commons.** Weaves/threads are already portable JSON. Add
      export/import affordances + a `pure-study-commons` community repo where
      PR review mirrors the in-app `approved` ethic. Ship more curated content
      in-box: 29 approved weaves and one thread (`romans-road`) today; a dozen
      excellent threads (Messianic prophecies, the Tabernacle, prayers of the
      Bible) cost an afternoon each and make first-run Full mode feel
      inhabited.
- [ ] **26. Docs & showing the depth.** The guide as a small site with GIFs
      (constellation, connectors, renderings lens) — the features are
      unphotographable in prose; motion sells them.

## The premium sync service (the only paid piece)

Scope = exactly the "yours" list ([README.md](README.md) §Your data): weaves,
threads, tags, patches — plus personal notes (#3), plans/memorization
progress, config, reading position.

- **E2EE by default** — study notes are pastoral and private; zero-knowledge
  server.
- **Per-file version history** ("restore my library to last Tuesday") — the
  atomic single-file JSON store makes this nearly free server-side.
- **Continuity** — pane state / last position across devices (sync sells best
  once Android exists; sequence accordingly).
- **Shared libraries** — family / class / congregation spaces: a pastor
  publishes a weave library; members subscribe read-only into their
  *Suggested* queue and approve into their own library — the existing
  approval flow *is* the sharing UX, already built.
- **Read-only web publish** of a weave/thread — the invitation surface, and
  the only "web app" this product ever needs.
- **The covenant, stated on the pricing page**: local files remain canonical
  and exportable forever; sync never gates a local feature. Convenience, not
  captivity.

## Suggested sequence

1. **Tier 0 (1–8)** now — small, additive, and they compound into every
   future screenshot and review.
2. **Android (9)** + packaging/signing/website (10–12); the macOS shell (13)
   follows — the view-model consolidation already makes a fourth shell
   paint-and-route cheap.
3. Differentiators: **memorization (15) first**, then allusion coverage (21) →
   power search (16) → harmony mode (18) → print/PDF (14, which also unlocks
   15's flashcards) → interlinear-lite (17) → command palette (24) → reading
   plans (22) → TTS/a11y (23) → people (19) → leitwort browser (20).
4. **Commons (25) + the sync service** after Android ships (continuity is the
   sync product's best demo).

---

# Engineering & data work

## Split `apps/desktop/src/main.rs` into modules

*The one remaining item from the 2026-07-16 architecture review (all of P0–P2
landed: view-models, popup + panel content models, and the link router moved
into the core; the manifest now describes endpoints instead of
re-implementations).*

- [ ] `main.rs` is ~3.6k lines — over the CLAUDE.md "no 3k-line files" rule.
      Mechanical module split (`reader` / `panes` / `popups` / `panel` /
      `links` / `chrome`) with **no logic change**; mostly `pub(crate)`
      visibility adjustments that want iterative `cargo check`, so best done
      in an environment where GTK compiles (not the Windows ARM64 box —
      validate via the `desktop-gtk` CI job).
- [ ] Nicety left from the review's P2.6: a compile-checked block-kind enum
      *shared into the shells* (today an unknown kind gracefully renders as
      nothing), so a shell that forgets a kind fails to build.

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
- [x] A first hand batch of widely-accepted suggestions landed in
      `weaves/suggested/` (2026-07-15); Song of Solomon and Philemon left
      empty deliberately -- Song's typological readings are tradition-specific
      rather than verse-level consensus, and Philemon has no OT parallels.
