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
