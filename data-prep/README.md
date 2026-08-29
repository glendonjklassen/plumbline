# data-prep — the R&D data pack

**The whole pack — reading core and R&D tier alike — is committed to this
repo** (`data/`, `bridge/`), so a clone is a hydrated home and none of this has
to be rebuilt to use the app. This document records what the artifacts are and
how each was *produced*, so the pack stays reproducible (and re-buildable if
the frozen tokenization ever changes). Provenance and licensing:
[BIBLIOGRAPHY.md](../BIBLIOGRAPHY.md).

The load-bearing fact (see the session analysis): **almost none of this is
"training."** One artifact (the concept vectors) is a lightweight CPU skip-gram;
everything else is a deterministic parse/align/download of public-domain source
texts. It is a **build-once → ship-the-files** proposition — no GPU, no
foundation model, no ongoing training, and the outputs are stable as long as the
tokenization stays frozen (`kjv1769-tok2`).

## What Plumbline consumes

All under the resolved data home (`core::home`) at `<home>/data/`:

| File | Feeds | Rust consumer | Build? |
|------|-------|---------------|--------|
| `kjv.jsonl` | the reader | `core::corpus` | committed (SWORD import) |
| `strongs.json` | Strong's + the **etymology bridge** | `core::strongs`, `rnd::bridge` | committed; bridge needs no extra data |
| `kjv-notes.jsonl` | 1769 margin notes | `core::notes` | committed (import) |
| `akjv.jsonl` | the plain-English overlay | `core::akjv` | deterministic align (no ML) |
| `cross-references.tsv` | TSK topical tier | `core::crossref` | download (no ML) |
| `concept-vectors.vec` (+ `.meta`, `.freq`) | **no longer shipped** (2026-07-30): its three readers were removed as noise. Still produced here; nothing consumes it | `rnd::embed` | **train once** (CPU) |
| `morphology.jsonl` | per-token parse | `rnd::morph` | deterministic projection |
| `chronological.json` | the chronological reading plan's curated order | `core::plan::load_table` | `scripts/build-chronological.mjs` from [chronological/order.json](chronological/order.json) (no ML; exactly-once canon coverage verified against the corpus at build) |
| `bridge/*.json` (LXX, Abbott-Smith, TIPNR) | fused cross-testament witnesses | `rnd::bridge` | committed / align (no ML) |
| `source-priors.json` | per-source trust weight | `rnd::bridge` | deterministic calibration |

Every consumer degrades gracefully: a missing file means the section simply
doesn't render. The reader runs with only the three core files.

## How each artifact is produced

The generators are the existing offline Python in the reference `overlay`
checkout (`ml/` and `pipelines/`, each with its own README). Plumbline does
**not** reimplement them — the runtime only reads their outputs, exactly as it
reads `kjv.jsonl`. Run them once against a hydrated home:

- **`concept-vectors.vec`** — `python3 ml/train_concept2vec.py` then
  `python3 ml/align_hg.py`. Skip-gram with negative sampling in **pure NumPy**
  (no gensim/torch, no GPU); minutes on a laptop. `align_hg.py` rotates the
  Hebrew subspace onto the Greek (orthogonal Procrustes, one SVD) so
  cross-testament neighbours become meaningful. Output is "wholly owned and
  free to relicense" (it never sees English). The `.meta` stamps the
  tokenization (Plumbline refuses a mismatch) and records the alignment + the
  Greek root-alias map; `.freq` carries the training counts the SIF weights use.
- **`morphology.jsonl`** — `python3 pipelines/morph_oshb.py` +
  `pipelines/morph_tr.py`, then the overlay `--project-morph` step. A
  deterministic projection of OSHB (Westminster Leningrad Codex; lemma/morph
  CC BY 4.0, text public domain) and Robinson's parsed Textus Receptus (public
  domain) onto the KJV tokens. No ML.
- **`cross-references.tsv`** — `python3 pipelines/cross_refs.py`. A download +
  reshape of openbible.info's TSK-derived, vote-ranked references (CC
  Attribution). No ML.

The **bridge** (`rnd::bridge`) works with no artifact at all — the etymology
layer is derived at runtime from `strongs.json`'s own "of Hebrew origin (Hxxxx)"
derivations — and, when the committed `bridge/*.json` witnesses and
`source-priors.json` are present, **fuses** them: each cross-testament partner
is tagged with the sources that assert it (etymology / lxx / abbott-smith /
tipnr / …) and ranked by the fitted trust prior.

- **`bridge/*.json`** — `python3 pipelines/lxx_bridge.py`,
  `pipelines/abbott_smith.py`, `pipelines/stepbible_tipnr.py`. Public-domain /
  CC-BY inputs; the artifacts carry their attributions. LXX uses IBM Model 2
  word alignment (pure Python EM, deterministic) — statistics, not training.
- **`source-priors.json`** — `python3 ml/calibrate_source_priors.py`. A
  deterministic calibration of each source's precision against the Abbott-Smith
  gold; a tiny JSON of weights.

## Deferred (still data-gated)

Not yet ported: cross-testament **quotation detection** (the fused
LXX/Abbott-Smith/embedding lexicon + CL-ASA run alignment) and the
`text-witness*` adversarial audit. Same recipe when wanted — port the Rust
consumer, ship the precomputed file.

Licensing: the concept vectors are wholly owned; TSK is CC-Attribution; OSHB is
CC BY 4.0; the TR/WLC texts and Abbott-Smith are public domain. The Louw-Nida /
SDBH semantic-domains source is CC-BY-**SA** (copyleft) and was deliberately
**excluded** from the shippable pack.

## Hosting the pack

Because Plumbline resolves its data home (env → working tree → next to the
executable → per-user data dir) and reads these files by name, "shipping the R&D
pack" is just placing the files under `<home>/data/`. A downloadable pack is a
tarball of the R&D rows above, extracted there; the app picks them up on next
launch with no code change. (The C ABI's `plumbline_engine_open_from_bytes` can also
load core data from asset bytes for a bundled build.)

## The `plumbline-hydrate` tool

`crates/hydrate` is a small cross-platform CLI that places the pack into a home
and **verifies** each artifact by loading it through the same code the app uses:

```sh
# Inspect a home — which tiers will light up?
cargo run -p plumbline-hydrate -- check --home ~/.local/share/plumbline

# Copy the pack from a source (e.g. an overlay checkout that already has the
# artifacts) into a home, then verify.
cargo run -p plumbline-hydrate -- copy --from ../overlay --to ~/.local/share/plumbline
```

`check` reports verse/entry counts, TSK coverage, embedding dim/alignment, and
morphology coverage; it exits non-zero only when a **required core** file is
missing. It does not generate the artifacts — run the offline pipeline above for
that — it assembles and validates them.


## `akjv.jsonl` — the plain-English overlay

    node scripts/build-akjv-delta.mjs --akjv <AKJV.json>

Source: the **American King James Version** (Michael Peter Engelbrite, 1999),
public domain, taken from `formats/json/AKJV.json` in
[scrollmapper/bible_databases]. It is a modernisation of the same text — same
31,102 verses, same versification — so no verse mapping is involved.

**It is a delta, not a second corpus.** For each verse the aligner word-diffs
the AKJV against the KJV's frozen tokens (LCS over case- and
punctuation-normalised words) and emits only the runs that differ, as
`[startTok, endTok, replacement]`. That keeps `kjv.jsonl` and the frozen
`kjv1769-tok2` stamp untouched, lets the reader swap words at layout time,
leaves every Strong's code attached to the KJV token that owns it, and makes
"show me the word this replaced" free — the original is still in the corpus.

Rendering rule for a span `[a,b]`: `pre(a) + replacement + post(b)`. The
interior punctuation of the consumed tokens is dropped, because the
replacement carries whatever the AKJV put between its own words (KJV
"Verily, verily" → AKJV "Truly, truly").

Two things the aligner is careful about, both found by looking at the output:

- The AKJV text carries paragraph pilcrows as standalone words. The KJV corpus
  carries paragraphs as a token *flag*, so a stray `¶` would read as an
  inserted word and land a no-op delta on the last token of a verse.
- A repeated word lets the LCS anchor on the other occurrence, pairing two
  identical words as a "replacement" (`day` → `day`). Those are dropped, or the
  reader would see a mark under a word that never changed.

Measured over the whole Bible: **6.9% of tokens** re-rendered, in 66.7% of
verses; 46,185 spans over 2,944 distinct replacements, dominated by
`unto`→`to` (7,375), `thy`→`your` (3,986), `ye`→`you` (3,562), `thee`→`you`,
`upon`→`on`, `hath`→`has`, `saith`→`says`. 1.35 MB raw, 210 KB gzipped.

[scrollmapper/bible_databases]: https://github.com/scrollmapper/bible_databases

## Luther 1912 (German corpus)

`data-prep/luther/build-luther.py` turns The Unbound Bible's public-domain
`luther_1912` export into `data/luther1912.jsonl`, in `kjv.jsonl`'s frozen shape
with the stamp `luther1912-tok1`.

**The reason a German Bible is tractable at all:** that source has already been
mapped to KJV versification. All 66 books, every chapter count and every
last-verse number match `data/kjv.jsonl` exactly — 31,102 verses. German
tradition numbers about 350 verses differently, and rather than renumber, the
Unbound editors moved the text to the KJV address and left the German number in
the verse as a `3:19 ` prefix. So **`refKey` means the same verse in both
corpora**, no versification map is needed, and nothing a reader has written needs
migrating.

The prefixes are stripped (they are an editorial artifact, not scripture) and
kept in `german-numbering.tsv`, which is the exact data a future "show German
verse numbers" feature would want. Nothing reads it today.

`check-luther.py` is the proof, and it takes the source file as an optional
argument to do its most important check. Six claims, each against evidence:
the addresses are the KJV's; the tokens reassemble into the verse; no artifact
survived; words are whole (`pre`/`post` are punctuation, never letters); the
divine name is marked on caps HERR/HERRN/HERRE and nowhere else; and every
letter of every verse is the source's.

Run both after any change to either script:

```sh
python3 data-prep/luther/build-luther.py luther_1912.json
python3 data-prep/luther/check-luther.py luther_1912.json
```

The fourth claim is there because mutation-testing the checker found it missing:
a tokenizer that peels a letter off the end of a word reassembles perfectly, has
identical letters, and silently breaks every tap target.

### Strong's tags for the German corpus

`data-prep/luther/merge-strongs.py` fills the tokens' empty Strong's slots from
the Zefania XML *Luther 1912 mit Strongs* (toledot.info; its header declares the
text public domain — see BIBLIOGRAPHY.md). The tokens themselves never change:
`luther1912-tok1` is frozen the way `kjv1769-tok2` is.

Verse addressing cannot bridge the two editions (the Zefania file's German
versification shifts in more places than `german-numbering.tsv` records, and its
orthography is modernized — `dass` for `daß`), so the merge aligns **each book
as one token stream** in reading order, with an orthography fold and handling
for joined words and contractions. ~98.3% of the source's ~350k tags transfer;
words the alignment cannot confidently pair stay untagged — a missing tag is
honest, a guessed one is not. `check-luther.py`'s seventh claim guards the
result: every code resolves in `data/strongs.json`, Hebrew only in the OT and
Greek only in the NT, and the count is high enough to prove the merge ran.

A corpus rebuild loses the tags, so the full sequence is:

```sh
python3 data-prep/luther/build-luther.py luther_1912.json
python3 data-prep/luther/merge-strongs.py "SF_2022-02-27_GER_LUTH1912_(LUTHER_1912_mit_Strongs).xml"
python3 data-prep/luther/check-luther.py
```

## The Spanish Bible (`data/rv1909.jsonl`)

`data-prep/rv1909/build-rv1909.py` turns the eBible.org USFX edition of the
Reina-Valera 1909 (public domain, via `seven1m/open-bibles`) into
`data/rv1909.jsonl`, in `kjv.jsonl`'s frozen shape with the stamp `rv1909-tok1`.

The source arrives better equipped than the German one did: it already sits at
KJV verse addresses (66 books, 1,189 chapters, 31,102 verses, every count
identical), it is already Strong's-tagged inline, and it marks
translator-supplied words with `<add>` — which is what the KJV's italics are, so
`FLAG_ADDED` means the same thing in both. No alignment pass and no numbering
table: Reina-Valera keeps the KJV's chapter and verse breaks throughout.

The one adjustment is that the source tags PHRASES where `kjv.jsonl` tags head
words; the build follows the KJV, because the renderings are derived by counting
the words under each code and `occurrence_count` counts tagged tokens. The
script's header has the argument.

`check-rv1909.py` is the proof and takes the source as an optional argument; with
it, it checks that every verse's letters are the source's.

```sh
curl -LO https://raw.githubusercontent.com/seven1m/open-bibles/master/spa-rv1909.usfx.xml
python3 data-prep/rv1909/build-rv1909.py spa-rv1909.usfx.xml
python3 data-prep/rv1909/check-rv1909.py spa-rv1909.usfx.xml
```

## The Arabic Bible (`data/svd1865.jsonl`)

`data-prep/svd/build-svd.py` turns eBible.org's USFM edition of the 1865 Smith &
Van Dyck (`arb-vd`, public domain) into `data/svd1865.jsonl`, stamped
`svd1865-tok1`.

The Arabic Bible with the KJV's standing, and then some: Masoretic Old
Testament, Textus Receptus New Testament, and since 2008 the shared pulpit text
of the Orthodox, Catholic and Evangelical churches in Egypt at once.

What it gives that the other two corpora did not: full vocalization (every word
carries its tashkeel), real paragraph divisions (15% of verses open one, against
the KJV's own 10% — Reina-Valera's source had one per chapter and the build
refused them), and the 120 psalm superscriptions, folded into verse 1 and
flagged the way `kjv.jsonl` folds them.

What it does not give: Strong's tags, and none are invented. Word alignments for
this text exist (BibleAquifer/ArabicVanDyckBible, CC0) but are LLM-generated, and
Arabic would have been the only corpus here whose codes were machine-guessed
rather than a publisher's claim about its own words. `ar`'s registry row carries
`lexicon: None`. No translator-supplied-word markup either — the KJV's italics
have no Van Dyck counterpart.

Two verses are MERGED rather than carried. The SVD prints 31,104 verses to the
KJV's 31,102, and both extras are splits: its 1 Tim 6:22 is the KJV's 6:21b, its
3 John 15 the KJV's 14b. No text moves either way, and every refKey in this app
is frozen, so the build folds them back to the KJV address. There is no
`numbering` row for it — that column annotates a DIFFERING number, and in both
cases the number is the same.

`check-svd.py` is the proof and takes the source as an optional argument; with
it, every verse's letters AND MARKS are checked against the source. It earned
that: it caught verse text riding on `\p` lines (50 verses losing a clause),
`\s1` section headings being absorbed into the verse above them, superscriptions
that run to two `\d` lines, and one stray combining mark that is a typo in the
1865 text itself.

```sh
curl -LO https://ebible.org/Scriptures/arb-vd_usfm.zip
python3 data-prep/svd/build-svd.py arb-vd_usfm.zip
python3 data-prep/svd/check-svd.py arb-vd_usfm.zip
```

## The Punjabi and Hindi Bibles (`data/pan-fbi.jsonl`, `data/hin-fbi.jsonl`)

`data-prep/indic/build-indic.py` turns Free Bibles India's USFM editions into
`data/pan-fbi.jsonl` and `data/hin-fbi.jsonl`, stamped `pan-fbi-tok1` and
`hin-fbi-tok1`. **One script for two languages** — unlike the three corpora
above, which each have their own — because these are the same publisher's export
in the same conventions, down to the markers used and the two verse splits. Two
files here would be one file and a copy of it, and the copy is where they drift.

Both are the traditional Protestant Bible of their language, ਪਵਿੱਤਰ ਬਾਈਬਲ and
पवित्र बाइबल, with modernised spelling and an editorial apparatus the build
discards. Both sit at KJV addresses: 66 books, 1,189 chapters, and 31,102 of
their 31,104 verses at the same address as `data/kjv.jsonl`. Both are Textus
Receptus — `check-indic.py` proves twenty readings a critical text omits or
brackets, Acts 8:37 among them. Neither carries Strong's tags, so both registry
rows are `lexicon: None`, as Arabic's is.

**The Punjabi text is not the obvious one, and that is the interesting part.**
The obvious candidate was `tfbf/Bible-Punjabi-Pavitr-Bible-1945`, a volunteer
digitisation of a 1945 print that is public domain outright. It was rejected on
the evidence: eight whole books of it — Titus, John, James, 1 Peter, 1–2
Thessalonians, 2 Peter, 1 Corinthians, 1,772 verses — are a different modern
translation spliced in, plus ~217 scattered verses elsewhere. **Acts 8:37 is one
of the splices.** The tell is punctuation: the 1945 keyboarding types the danda
as an ASCII `|` in 19,306 verses, the spliced material uses a real `।` U+0964,
and no book uses both. Its own `STATUS.md` says the files "are not ready to be
used in a real project".

Every other check passes on that file — 66 books, KJV addresses, all twenty TR
readings present, tokens that reassemble — so `check-indic.py` carries the test
that caught it as a standing claim: **a sentence terminator accounting for more
than 1% of the corpus's terminators must be used by at least 90% of its books.**
On the 1945 file `।` is 11.9% of the terminators and appears in 18 of 66 books.
The next Indian-language corpus offered to this app is likely to have been
assembled the same way.

Two more things worth writing down:

- **The text is never normalised.** The Punjabi source repo warns in capitals
  that Unicode normalisation must not be applied to Gurmukhi: the precomposed
  nukta letters are on Unicode's composition exclusion list, so NFC *decomposes*
  them and "normalising" silently rewrites letters. Devanagari is on the same
  list. Both files are already stable under NFC and NFD — measured, not assumed
  — so passing them through unchanged costs nothing, and the check asserts it so
  the one line a later maintainer might add by habit fails the build.
- **The two splits merge in opposite directions.** 3 John 15 is the tail of the
  KJV's v14 and is appended; Rev 12:18 is the *head* of the KJV's 13:1 and is
  prepended. `build-svd.py` never had to say this because both of its merges ran
  the same way. A build that appended both keeps every word, passes the letter
  comparison, and prints Revelation 13:1 with its first clause last.

```sh
curl -sLo pa.zip https://github.com/FreeBiblesIndia/Punjabi_Bible/archive/refs/heads/master.zip
curl -sLo hi.zip https://github.com/FreeBiblesIndia/Hindi_Bible/archive/refs/heads/master.zip
python3 data-prep/indic/build-indic.py pa pa.zip && python3 data-prep/indic/check-indic.py pa pa.zip
python3 data-prep/indic/build-indic.py hi hi.zip && python3 data-prep/indic/check-indic.py hi hi.zip
```

## The French Bible (`data/ost1996.jsonl`)

`data-prep/ostervald/build-ostervald.py` turns `fra-ostervald.osis.xml` (the
seven1m/open-bibles rendering of the J.F. Ostervald translation, **1996
revision** — the OSIS header says `ostv1996` and the About screen says 1996
too) into `data/ost1996.jsonl`, stamped `ost1996-tok1`. Public domain per the
open-bibles catalogue. Ostervald is the French TR line — the Geneva/Olivétan
tradition — where Louis Segond is eclectic and was rejected; Acts 8:37 and the
other nineteen discriminators all carry text.

**This is the first corpus since Luther whose source does not sit at KJV
addresses** — and Luther arrived pre-aligned by the Unbound editors, so it is
the first this pipeline aligns itself. The source prints French/Hebrew-style
numbering: 31,172 verses, 91 chapters breaking differently (psalm titles
numbered as verse 1, Job 38–41 recut, a dozen boundary shifts from Numbers to
Revelation). The build walks the KJV's addresses while consuming the source's
verses in canon order, which lands everything with only three primitive kinds
of directive — 62 psalms' title verses folded into verse 1 as `FLAG_TITLE`
(five span two verses), six merges (including Rev 12:18, *"Et je me tins
debout"*, the TR's "I stood", prepended into 13:1 exactly as `build-indic.py`
does), and three splits at asserted sentence boundaries (Luke 10:41, Acts
19:40, 2 Cor 13:12). What a printed Ostervald calls each of the 1,263 moved
addresses lands in `crates/core/src/versification/ostervald-numbering.tsv`,
which the French row's `NumberingSpec` annotates from.

Points a later maintainer should not rediscover:

- **Elision lives in `pre`** — "l'homme" is `l'` + `homme`, off a closed
  thirteen-prefix list, so the search index holds the word a reader types.
  `check-ostervald.py` therefore *allows* letters in `pre` (exactly the
  whitelist) where every other checker forbids them.
- **The letter-stream proof is per book and alignment-independent**: every
  directive preserves source order, so each book's letters, concatenated, must
  equal the source's — no letter lost, invented or reordered, however the
  addresses moved.
- **The source has its own typos and they ship** ("de tout ton cour" at the
  gate verse itself, œ lost); shipping the source's letters is the policy, as
  it was for the Indic texts.

```sh
curl -LO https://raw.githubusercontent.com/seven1m/open-bibles/master/bibles/fra-ostervald.osis.xml
python3 data-prep/ostervald/build-ostervald.py fra-ostervald.osis.xml
python3 data-prep/ostervald/check-ostervald.py fra-ostervald.osis.xml
```

## The Chinese Bibles (`data/cuv1919t.jsonl`, `data/cuv1919s.jsonl`)

`data-prep/cuv/build-cuv.py` turns the seven1m/open-bibles USFX editions of
the 1919 Chinese Union Version (和合本) — `chi-cuv.usfx.xml` traditional,
`chi-cuv-simp.usfx.xml` simplified — into two corpora, stamped `cuv1919t-tok1`
and `cuv1919s-tok1`. One script for both editions, for `build-indic.py`'s
reason, and `check-cuv.py` additionally proves the two **parallel
token-for-token** (same addresses, same token counts, same flags — every verse
letter-count-identical across editions). Public domain: 1919 clears the US
95-year term outright; the Revised CUV (2010) is in copyright and is *not*
this text, which is why the About screen names the year.

**The corpora tokenize one character per token**, and that is the load-bearing
decision of the whole Chinese row:

- **Search**: `run_search`'s query splitter breaks a Han run into
  per-character words, so the existing phrase tier — consecutive-token
  confirmation — becomes exact substring search, which is what a Chinese
  reader expects. Dictionary segmentation was rejected: a reader's word
  boundaries and a segmenter's disagree (transliterated names worst), and
  every disagreement is a search that finds nothing.
- **Layout**: break opportunities become exactly token boundaries, so the
  greedy breaker in `crates/layout` needed no intra-token work — and gluing
  punctuation into `pre`/`post` *is* the kinsoku rule (a closing 。」 can
  never open a line; an opening 「 can never end one). The FFI zeroes
  `space_width` for a Han corpus the same way it derives `rtl`: from the open
  corpus's tokenization stamp, with no ABI change.

The remap is small — the CUV nearly sits at KJV addresses already: John 7:53
split back out of the head of 8:1, 1 Chr 22 shifted one verse (the printed
21:31 is the KJV's 22:1), 3 John 15 merged as always. The printed CUV combines
ranged verses and prints **併於上節** ("combined with the previous verse") at
the second number — 69 such placeholder verses ship verbatim, because they are
what the printed page shows, and the build constructs the same placeholder at
the two range tails the file left unemitted (Deut 13:18, Ps 116:19 — both
interleave their clauses inside the merged verse, so no honest cut exists).
All 22 disagreeing addresses land in
`crates/core/src/versification/cuv-numbering.tsv`, shared by both rows.

One transformation touches letters, and only in the simplified edition: two
2013 规范字 codepoints in CJK Extension B (𫈟, 𫗪 — twelve occurrences) that
Source Han Serif and virtually every other font lack ship as their
traditional forms 蔯/餵 instead — the convention simplified text uses for an
unencodable rare form, counted exactly so upstream drift is loud, and
`check-cuv.py` proves the whole corpus sits in the renderable repertoire.

The face is derived, not declared: `apps/web/scripts/subset-fonts.mjs`
subsets Source Han Serif TC (fetched sha256-pinned, 24 MB, gitignored) to the
exact codepoints of the two corpora and two catalogues — ~1.0 MB of woff2 —
and a scripture codepoint the font lacks fails the build.

```sh
curl -LO https://raw.githubusercontent.com/seven1m/open-bibles/master/bibles/chi-cuv.usfx.xml
curl -LO https://raw.githubusercontent.com/seven1m/open-bibles/master/bibles/chi-cuv-simp.usfx.xml
python3 data-prep/cuv/build-cuv.py t chi-cuv.usfx.xml && python3 data-prep/cuv/check-cuv.py t chi-cuv.usfx.xml
python3 data-prep/cuv/build-cuv.py s chi-cuv-simp.usfx.xml && python3 data-prep/cuv/check-cuv.py s chi-cuv-simp.usfx.xml
```

## Localized Strong's dictionaries (`data/strongs-<code>.json`)

Two scripts in `data-prep/strongs-lang/`, both taking a language code and reading
`plumbline-hydrate languages` for which corpus and which output file that code
means:

- `translate.py` — machine-translates `strongs_def` and `derivation` of every
  `data/strongs.json` entry over the Batch API (needs `ANTHROPIC_API_KEY`;
  idempotent and resumable; writes the committed intermediate
  `translations.<code>.json`). The app labels these definitions as
  machine-translated and points readers at the repo's issues for corrections.
- `build-strongs.py` — assembles the shipped file: language-neutral fields
  copied, translated prose folded in (English fallback where a translation is
  missing), and the `kjv_def` slot filled with **that language's own renderings**
  derived from its tagged corpus — the words that actually stand under each code,
  most frequent first. Derived data, not AI output.

It prints what `machine_translated` should be on that language's registry row.
Set it: the study card's AI caveat reads off that flag, and a dictionary whose
renderings are localized while its prose is still English must not claim to be a
translation.

```sh
python3 data-prep/strongs-lang/build-strongs.py de
ANTHROPIC_API_KEY=sk-… python3 data-prep/strongs-lang/translate.py es
python3 data-prep/strongs-lang/build-strongs.py es
```

**How the two shipped intermediates were actually made.** The German
`translations.de.json` came out of `translate.py` over the Batch API. The
Spanish `translations.es.json` was translated by a fleet of Claude Sonnet
subagents instead (2026-08-16) — same system prompt, same fixed renderings for
Strong's formulaic vocabulary (properly → propiamente, compare → compárese, …),
the dictionary sliced into ≤200-entry files and each slice translated by one
agent. Every slice was validated before merging: it parses, its key set matches
its input exactly, every translated field is non-empty with none invented, and
every cross-reference code (H7225, G26) survives verbatim; slices that dropped
even one entry were rejected and re-run. `translate.py` remains the
reproducible path for anyone with an API key — the two routes produce the same
kind of artifact, and the committed intermediate is the source of truth either
way.
