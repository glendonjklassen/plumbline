# Bibliography — data sources & licensing

What Plumbline ships, where each piece comes from, and under what license.
Everything is freely licensed or public domain; per the project's stance, every
analysis layer is a tool for studying the text, never an authority over it.

## The text and its apparatus

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `data/kjv.jsonl` | KJV 1769 text, Strong's-tagged, tokenized (`kjv1769-tok2`) | eBible.org SWORD module `engKJV2006eb`, converted by the overlay import pipeline | KJV text: public domain (Crown patent applies in the UK) |
| `data/strongs.json` | Strong's Hebrew + Greek dictionaries (14,197 entries) | [openscriptures/strongs](https://github.com/openscriptures/strongs) | CC-BY-SA |
| `data/kjv-notes.jsonl` | The 1769 translators' margin notes | same import pipeline | public domain |
| `data/cross-references.tsv` | ~343k Treasury of Scripture Knowledge references with votes | [openbible.info](https://www.openbible.info/labs/cross-references/) | CC Attribution; TSK itself public domain |

## Morphology

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `data/morphology.jsonl` (Hebrew side) | OSHM parses projected onto the KJV tokens | [openscriptures/morphhb](https://github.com/openscriptures/morphhb) (OSHB/WLC) | CC-BY 4.0 |
| `data/morphology.jsonl` (Greek side) | Robinson parsing codes, Textus Receptus | [byztxt/greektext-textus-receptus](https://github.com/byztxt/greektext-textus-receptus) (Dr. M. A. Robinson) | public domain |

## The cross-testament bridge

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `bridge/lxx-alignment.json` | Hebrew↔Greek links from LXX↔WLC statistical alignment | [eliranwong/LXX-Swete-1930](https://github.com/eliranwong/LXX-Swete-1930) (Swete's LXX, public domain text) + CLTK lemmata | see repos; Swete text public domain |
| `bridge/abbott-smith.json` | Abbott-Smith's Manual Greek Lexicon OT references | [translatable-exegetical-tools/Abbott-Smith](https://github.com/translatable-exegetical-tools) | public domain text; repo CC-BY-SA |
| `bridge/stepbible-tipnr.json` | Proper-name Hebrew↔Greek identities (TIPNR) | [STEPBible/STEPBible-Data](https://github.com/STEPBible/STEPBible-Data) | CC-BY 4.0 |
| `data/source-priors.json`, `data/quotation-pairs.json`, `data/text-witness.json` | fitted trust priors; harvested quotation pairs; the graded text-witness | produced by the offline pipeline over the sources above | derived works of the above |

## Self-trained artifacts

| Shipped file | Contents | Provenance |
|---|---|---|
| `data/concept-vectors.vec` (+ `.meta`, `.freq`) | 7,426 concept embeddings over Strong's sequences, Procrustes-aligned across testaments | trained offline in pure NumPy on the tagged corpus; it never sees the English surface, so the output is wholly owned and freely licensable |

No modern pretrained encoder touches the text anywhere: contextual models
misread Early Modern English, so every artifact is either curated (above) or
self-trained on Strong's numbers. The methods papers behind each layer
(word2vec/SIF/Procrustes/scan statistics/label propagation) are catalogued in
the overlay project's BIBLIOGRAPHY; the Rust ports implement the same recipes
(see the module docs in `crates/rnd/`).

## Study content

`weaves/` (including the suggested queue), `threads/`, and `patches/` are
authored study data. The shipped weaves began life as **AI-generated study
aids**; each carries an `approved` flag surfaced in the reader, and the
suggested queue is exactly that — suggestions, kept out of the approved
library until a human blesses them.

## Type

Scripture renders in **EB Garamond** (SIL Open Font License), bundled with the
web shell under `apps/web/public/fonts/`; the Compose shell asks for the same
family from `assets/fonts/` and falls back to the platform serif.
