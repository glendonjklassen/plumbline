# data-prep — the R&D data pack

pure-study's reading core (KJV + Strong's + notes) is small and ships with the
app. The **R&D tier** — concept embeddings, morphology, and the TSK topical
cross-references — is driven by larger, precomputed **artifacts**. This document
records exactly what those artifacts are, how they are produced, and where
pure-study looks for them, so the pack is reproducible and hostable.

The load-bearing fact (see the session analysis): **almost none of this is
"training."** One artifact (the concept vectors) is a lightweight CPU skip-gram;
everything else is a deterministic parse/align/download of public-domain source
texts. It is a **build-once → ship-the-files** proposition — no GPU, no
foundation model, no ongoing training, and the outputs are stable as long as the
tokenization stays frozen (`kjv1769-tok2`).

## What pure-study consumes

All under the resolved data home (`core::home`) at `<home>/data/`:

| File | Feeds | Rust consumer | Build? |
|------|-------|---------------|--------|
| `kjv.jsonl` | the reader | `core::corpus` | core (ships) |
| `strongs.json` | Strong's + the **etymology bridge** | `core::strongs`, `rnd::bridge` | core (ships); bridge needs no extra data |
| `kjv-notes.jsonl` | 1769 margin notes | `core::notes` | core (ships) |
| `cross-references.tsv` | TSK topical tier | `core::crossref` | download (no ML) |
| `concept-vectors.vec` (+ `.meta`, `.freq`) | concept neighbours + "verses like this" | `rnd::embed` | **train once** (CPU) |
| `morphology.jsonl` | per-token parse | `rnd::morph` | deterministic projection |

Every consumer degrades gracefully: a missing file means the section simply
doesn't render. The reader runs with only the three core files.

## How each artifact is produced

The generators are the existing offline Python in the reference `overlay`
checkout (`ml/` and `pipelines/`, each with its own README). pure-study does
**not** reimplement them — the runtime only reads their outputs, exactly as it
reads `kjv.jsonl`. Run them once against a hydrated home:

- **`concept-vectors.vec`** — `python3 ml/train_concept2vec.py` then
  `python3 ml/align_hg.py`. Skip-gram with negative sampling in **pure NumPy**
  (no gensim/torch, no GPU); minutes on a laptop. `align_hg.py` rotates the
  Hebrew subspace onto the Greek (orthogonal Procrustes, one SVD) so
  cross-testament neighbours become meaningful. Output is "wholly owned and
  free to relicense" (it never sees English). The `.meta` stamps the
  tokenization (pure-study refuses a mismatch) and records the alignment + the
  Greek root-alias map; `.freq` carries the training counts the SIF weights use.
- **`morphology.jsonl`** — `python3 pipelines/morph_oshb.py` +
  `pipelines/morph_tr.py`, then the overlay `--project-morph` step. A
  deterministic projection of OSHB (Westminster Leningrad Codex; lemma/morph
  CC BY 4.0, text public domain) and Robinson's parsed Textus Receptus (public
  domain) onto the KJV tokens. No ML.
- **`cross-references.tsv`** — `python3 pipelines/cross_refs.py`. A download +
  reshape of openbible.info's TSK-derived, vote-ranked references (CC
  Attribution). No ML.

The etymology **bridge** (`rnd::bridge`) needs no artifact at all — it is
derived at runtime from `strongs.json`'s own "of Hebrew origin (Hxxxx)"
derivation strings.

## Deferred (still data-gated)

These overlay tiers need additional hydrated inputs and are **not yet ported**:
the multi-source bridge fusion + calibrated trust model (`bridge/*.json`,
`source-priors.json`, `text-witness*`), and cross-testament quotation detection
(the fused LXX/Abbott-Smith/embedding lexicon). They can be added the same way —
port the Rust consumer, ship the precomputed file — when wanted.

Licensing: the concept vectors are wholly owned; TSK is CC-Attribution; OSHB is
CC BY 4.0; the TR/WLC texts and Abbott-Smith are public domain. The Louw-Nida /
SDBH semantic-domains source is CC-BY-**SA** (copyleft) and was deliberately
**excluded** from the shippable pack.

## Hosting the pack

Because pure-study resolves its data home (env → working tree → next to the
executable → per-user data dir) and reads these files by name, "shipping the R&D
pack" is just placing the files under `<home>/data/`. A downloadable pack is a
tarball of the R&D rows above, extracted there; the app picks them up on next
launch with no code change. (The C ABI's `pure_engine_open_from_bytes` can also
load core data from asset bytes for a bundled build.)

## The `pure-hydrate` tool

`crates/hydrate` is a small cross-platform CLI that places the pack into a home
and **verifies** each artifact by loading it through the same code the app uses:

```sh
# Inspect a home — which tiers will light up?
cargo run -p pure-hydrate -- check --home ~/.local/share/pure-study

# Copy the pack from a source (e.g. an overlay checkout that already has the
# artifacts) into a home, then verify.
cargo run -p pure-hydrate -- copy --from ../overlay --to ~/.local/share/pure-study
```

`check` reports verse/entry counts, TSK coverage, embedding dim/alignment, and
morphology coverage; it exits non-zero only when a **required core** file is
missing. It does not generate the artifacts — run the offline pipeline above for
that — it assembles and validates them.
