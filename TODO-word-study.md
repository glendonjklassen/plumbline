# TODO — word study

Goal: give a reader tools to study a word from its **usage in the corpus** — where it
occurs, what company it keeps, how it distributes, what it translates — and retire
Strong's as the front door of word study. Retire the *dictionary as the study surface*,
not the tags: the codes in `kjv.jsonl` tokens are a frozen format and stay forever as
alignment keys. What goes is the pattern where tapping a word answers with a
nineteenth-century gloss first and the text's own evidence second.

## Why word-first

Everything downstream of a tap today is keyed by Strong's code — the panel producers
(`crates/core/src/panel.rs`), the occurrence index, the FFI endpoints, the web panel
kinds. Two consequences:

1. **Untagged corpora get nothing.** Every recent language ships `lexicon: None`, and a
   word tap answers `study.noStrongs`. That list is growing, not shrinking.
2. **The dictionary frames the study.** The reader meets a code and a compressed gloss
   before they meet a single occurrence.

A usage study keyed on the **surface word** needs no tags at all: occurrences, context
lines, distribution, and collocates fall out of the corpus itself, in any language.
Where tags exist they become an *enrichment* — sense-splitting, other renderings,
cross-language bridge — instead of a prerequisite. That is the whole retirement path in
one sentence: word-first tools on every corpus; Strong's demoted from front door to
appendix; the definitional prose eventually an optional pack.

## 1. `WordIx` — the missing index

New core index: **folded surface word → occurrences as (verse, token span)**, per
corpus.

- Nothing public does this today. `SearchIx::word_idxs` is private, `run_search` is
  capped at 200 with tier semantics baked in, `OccurrenceIx` is code-keyed and
  verse-granular (no token offsets), and `Renderings` only covers tagged runs.
- Reuse `fold_word` / `normalize_word` from `search.rs` (the grapheme-cluster fixes are
  already in there). Corpus-parametric like `Renderings::build`, so one implementation
  serves every language.
- Built incrementally in warm slices like `OccurrenceIxBuilder` / `RenderingsBuilder` —
  the boot-responsiveness rule applies, the engine lives in one worker thread.
- Token spans matter: they are what turns an occurrence list into concordance *lines*
  (context around the word, the word emphasized) instead of bare references.

## 2. The usage card — the new tap answer

A rebuilt word-study producer keyed by **(corpus, word)**, codes optional. Roughly in
order:

- **Headline:** the word and its occurrence count. Never a code.
- **Distribution:** counts by book and testament split. `concept.rs` already computes
  this per code (`ConceptStat`, `top_books`, `testament_split`); per word it is the same
  counting over `WordIx`, and it is cheap enough to be ungated.
- **Concordance in context:** KWIC lines from the token spans, with scope chips reusing
  `SearchScope` (`All / Book / Chapter / OT / NT`) — search already has this scoping
  vocabulary and the concordance producers currently take no scope at all. Add a paging
  verb to `PanelLink` while here: `OCCURRENCE_CAP` (500) and `PANEL_OCC_CAP` (300) end
  in "… N more" with no way to continue.
- **Senses** (tagged corpora only): the word's occurrences grouped by underlying code —
  `word_codes` plus `Renderings` inverted. "Mercy" splits into H2617 and G1656; each
  sense links to its own concordance and to the code's *other* renderings
  (lovingkindness, kindness…). This is the single strongest "study context" tool the
  data supports, and every piece of it already exists.
- **Collocates:** which words keep company with this one. `concept.rs` already does
  PPMI + mutual-kNN per code; generalize to folded surface words. Open question:
  stopword filtering — `stopwords.rs` is code-keyed, so surface-word collocation needs
  a per-language stoplist or a frequency-derived one.
- **Concentration:** `burst.rs` per word — "34 of its 41 uses cluster in Jeremiah 1–20"
  is exactly the kind of context fact this feature is for. Generalizes the same way as
  collocates.
- **Dictionary, last:** where a lexicon exists, the gloss and the Strong's entry
  collapse into an expandable section at the bottom. The evidence leads; the gloss
  comments.

Gating: occurrences, distribution, and KWIC should be **ungated** — they are the
reader's own text rearranged, not analysis. Senses, collocates, and bursts keep the
existing `Gates` split (renderings/morph under human, concept/leitwort under machine).

## 3. Wiring the code already asks for

- **Unstub form search.** `form_search_scoped` in `search.rs` parses
  `pos/stem/conj/voice/mood/…` predicates and then returns "form search needs the
  morphology layer" — while `plumbline-rnd::morph` can answer it. Feature-gated, this
  gives "every imperative of this verb" for free. It is the most concrete word-study
  wiring already sketched in the tree.
- **Scope the existing concordances.** `panel::concordance` and
  `strongs_occurrences_json` take no scope; thread `SearchScope` through.
- Each new view follows the standing pattern: producer in `panel.rs` → `PanelLink`
  verb → FFI endpoint (regenerate bindings) → `StudyEngine` method → `PanelView` kind
  in the web shell.

## 4. What "retire Strong's" means precisely

- **Keep:** the codes in `kjv.jsonl` tokens (frozen), refKey, code-keyed bridge/witness
  data. Codes become invisible plumbing — alignment keys the reader never has to see.
- **Demote:** `StrongsEntry` prose (`strongs_def`, `kjv_def`, `derivation`) out of the
  front door, per §2.
- **Replace:** the only fields the usage card still needs from `strongs.json` are
  **lemma and xlit** — to label a sense "חֶסֶד chesed" instead of "H2617". Plan a slim
  lexeme table (code → lemma, xlit, pron) so the 14k definitional entries can move to
  an optional download later; `strongs-de` / `strongs-es` follow the same path.
- **Exit criteria:** a word tap never renders a code as a headline; every panel string
  currently sourced from `StrongsEntry` has a usage-derived or lexeme-table
  replacement; the engine boots and every producer works with `strongs.json` absent.

## Sequencing

1. `WordIx` + ungated usage card (headline, distribution, KWIC with scope + paging) —
   ships value to **every** language immediately and replaces the `study.noStrongs`
   dead end.
2. Senses + renderings integration on tagged corpora; scoped concordances; paging on
   the existing code-keyed views.
3. Word-level collocates and bursts (feature-gated); unstub form search.
4. Lexeme table; demote the dictionary; move definitional prose to an optional pack.

## Constraints

- **Do not touch i18n now.** The usage card needs new catalogue keys, and
  `every_shipped_string_is_translated` makes strings all-or-nothing across every
  shipped language — while the language push is in flight, that surface is theirs.
  Build core + FFI + producers first; land strings after the languages settle, or
  hand the key list to whoever owns the catalogue at that point.
- Web shell only, per the standing scope decision.
- `WordIx` memory and warm-time cost should be measured against the existing search
  index before committing to always-on; slice sizes per the existing `WARM_SLICE`
  pattern.

## Open questions

- Does the usage card *replace* `word_study_gated` outright, or become the default
  with the current code-first card reachable behind a link during a transition?
- Collocate stoplists for untagged languages: hand-written per language, or derived
  from corpus frequency (top-N folded words)?
- Is the slim lexeme table worth minting as a new frozen format now, or does
  `strongs.json` stay bundled indefinitely with only the UI demotion (§2) shipped?
