# Bibliography — data sources & licensing

What Plumbline ships, where each piece comes from, and under what license.
Everything is freely licensed or public domain; per the project's stance, every
analysis layer is a tool for studying the text, never an authority over it.

## The text and its apparatus

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `data/kjv.jsonl` | KJV 1769 text, Strong's-tagged, tokenized (`kjv1769-tok2`) | eBible.org SWORD module `engKJV2006eb` 14.3, converted by the overlay import pipeline (see below) | KJV text: public domain (Crown patent applies in the UK) |
| `data/strongs.json` | Strong's Hebrew + Greek dictionaries (14,197 entries) | [openscriptures/strongs](https://github.com/openscriptures/strongs) | CC-BY-SA |
| `data/kjv-notes.jsonl` | The 1769 translators' margin notes — 6,959 of them, Genesis to Malachi | the `<note>` markup carried inside that same `engKJV2006eb` 14.3 dump, lifted in the same pass (see below) | public domain |
| `data/akjv.jsonl` | Plain-English overlay: where the AKJV words a verse differently, keyed to KJV token spans (6.9% of tokens) | American King James Version, Michael Peter Engelbrite 1999, via [scrollmapper/bible_databases](https://github.com/scrollmapper/bible_databases); aligned by `scripts/build-akjv-delta.mjs` | **public domain** — released 1999-11-08, "copy it, sell it, modify it" |
| `data/cross-references.tsv` | ~343k Treasury of Scripture Knowledge references with votes | [openbible.info](https://www.openbible.info/labs/cross-references/) | CC Attribution; TSK itself public domain |

**The KJV module, named exactly.** Both KJV rows come out of one dump of one
module: **`engKJV2006eb`, version 14.3** — eBible.org's Strong's-tagged KJV,
documented by the import pipeline as the 1769 standardised text — distributed
through CrossWire's install manager. The reference `overlay` checkout's
hydration script installs it (`installmgr -ri eBible.org engKJV2006eb`), dumps
it to plain text (`mod2imp`), and the importer walks that dump once: verse text
becomes `kjv.jsonl`'s tokens, and the `<note>` elements sitting inside those
verses become `kjv-notes.jsonl`. The margin notes are therefore not a separate
edition to credit — they are the apparatus shipped inside the same module — and
`data/kjv.jsonl`'s header line records the identity for both:
`"source":"engKJV2006eb 14.3 (CrossWire/eBible.org, public domain)"`. The notes
run Genesis to Malachi, 6,959 rows over the 39 Old Testament books; the module
carries none for the New Testament.

eBible.org's catalogue lists that KJV today as **`ENGKJV` / `eng-kjv2006`**
([entry](https://ebible.org/find/details.php?id=engKJV), public domain); the
module id `engKJV2006eb` itself no longer resolves there (checked 2026-07-29).
`data/` and `bridge/` are committed to this repo, so nothing re-fetches it.

## The German text and its apparatus

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `data/luther1912.jsonl` (text) | Luther 1912 at KJV addresses, tokenized (`luther1912-tok1`) | The Unbound Bible (Biola University) `luther_1912` module, via [kesaranb/luther1912](https://github.com/kesaranb/luther1912); built by `data-prep/luther/build-luther.py` | public domain (module metadata: "This Bible is in the Public Domain") |
| `data/luther1912.jsonl` (Strong's tags) | ~344k Strong's tags merged onto the tokens | Zefania XML *Luther 1912 mit Strongs* (`SF_2022-02-27_GER_LUTH1912_Strongs_xml_220227.zip`, [zefania-sharp on SourceForge](https://sourceforge.net/projects/zefania-sharp/files/Bibles/GER/Lutherbibel/Luther%201912/)), creator/publisher www.toledot.info; merged by `data-prep/luther/merge-strongs.py` (book-level token alignment, ~98.3% of the source's tags; unmatched words stay untagged) | public domain (the file's own header: "This Text is in the Public Domain") |
| `data/strongs-de.json` | German Strong's dictionary: machine-translated definitions + Luther renderings derived from the tagged corpus | translation of `data/strongs.json` by `data-prep/strongs-lang/translate.py` (Claude, Batch API; labelled as machine-translated in the app) + renderings computed by `build-strongs.py` | **CC-BY-SA** — a derivative of openscriptures/strongs, share-alike carries over |
| `data/rv1909.jsonl` (text + Strong's tags) | Reina-Valera 1909 at KJV addresses, tokenized (`rv1909-tok1`), with the source's own inline Strong's tags on each phrase's head word | eBible.org USFX edition via [seven1m/open-bibles](https://github.com/seven1m/open-bibles) (`spa-rv1909.usfx.xml`); built by `data-prep/rv1909/build-rv1909.py`, proved by `check-rv1909.py` | public domain (the 1909 revision; the 1960 is not) |
| `data/strongs-es.json` | Spanish Strong's dictionary: machine-translated definitions + Reina-Valera renderings derived from the tagged corpus | translation of `data/strongs.json` by a Claude Sonnet subagent fleet, 2026-08-16 (same prompt and validation as `data-prep/strongs-lang/translate.py`, which remains the reproducible path; labelled as machine-translated in the app) + renderings computed by `build-strongs.py` from `data/rv1909.jsonl` | **CC-BY-SA** — a derivative of openscriptures/strongs, share-alike carries over |
| `data/svd1865.jsonl` | Smith & Van Dyck 1865 at KJV addresses, tokenized (`svd1865-tok1`), fully vocalized; two split verses merged back to the KJV address | eBible.org USFM edition `arb-vd`; built by `data-prep/svd/build-svd.py`, proved by `check-svd.py` | public domain (the 1865 text; eBible's `copr.htm` and catalogue both say so) |
| `data/pan-fbi.jsonl` | ਪਵਿੱਤਰ ਬਾਈਬਲ (Punjabi) at KJV addresses, tokenized (`pan-fbi-tok1`); two split verses merged back to the KJV address | [FreeBiblesIndia/Punjabi_Bible](https://github.com/FreeBiblesIndia/Punjabi_Bible), original work available at <http://freebiblesindia.in>; built by `data-prep/indic/build-indic.py`, proved by `check-indic.py`. **Modified**: the text is tokenized into the frozen `kjv.jsonl` shape, its two split verses are merged to the KJV address, and the publisher's footnotes, section headings and book introductions are not carried. | **CC BY-SA 4.0** — attribution above, share-alike carries to the tokenized corpus |
| `data/hin-fbi.jsonl` | पवित्र बाइबल (Hindi) at KJV addresses, tokenized (`hin-fbi-tok1`); two split verses merged back to the KJV address | [FreeBiblesIndia/Hindi_Bible](https://github.com/FreeBiblesIndia/Hindi_Bible), original work available at <http://freebiblesindia.in>; built by `data-prep/indic/build-indic.py`, proved by `check-indic.py`. **Modified**: as above, and the source's inline cross-reference apparatus (`\bdit`, 2,513 spans) is dropped with the footnotes. | **CC BY-SA 4.0** — attribution above, share-alike carries to the tokenized corpus |

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

**No longer shipped, as of 2026-07-30.** The one entry in this section left the
product that day: nothing in the app reads it any more, so it is not in the APK
and not in the web data pack. The provenance record stays here because the file
is still an output of the offline pipeline and still lives in `data/`.

| File | Contents | Provenance |
|---|---|---|
| `data/concept-vectors.vec` (+ `.meta`, `.freq`) — **not shipped since 2026-07-30** | 7,426 concept embeddings over Strong's sequences, Procrustes-aligned across testaments | trained offline in pure NumPy on the tagged corpus; it never sees the English surface, so the output is wholly owned and freely licensable |

The three features that read it were removed the same day, on the maintainer's
call that they were machine-generated noise: SIMILAR CONCEPTS, "verses like
this", and the concept map.

No modern pretrained encoder touches the text anywhere: contextual models
misread Early Modern English, so every artifact is either curated (above) or
self-trained on Strong's numbers. The methods papers behind each layer
(word2vec/SIF/Procrustes/scan statistics/label propagation) are catalogued in
the overlay project's BIBLIOGRAPHY; the Rust ports implement the same recipes
(see the module docs in `crates/rnd/`).

## Study content

The bundled study set is `apps/android/app/src/main/assets/stock/`, seeded once
into the reader's own files, after which their copies rule: **28 weaves** in the
library (386 links, every one marked `approved`), **194 more under
`weaves/suggested/`** (none approved), and one thread — the Romans Road. The
weaves began life as **AI-generated study aids**, and their notes say so
(`notesSource: "generated"`, which is also the default when the field is
absent). `approved` is surfaced in the reader, and approving a suggestion is
what promotes it out of the queue into the library.

## Type

Scripture renders in **EB Garamond** (SIL Open Font License), bundled with the
web shell under `apps/web/public/fonts/`; the Compose shell asks for the same
family from `assets/fonts/` and falls back to the platform serif. The picker
offers four more faces, all bundled the same way and all OFL: **Literata**
(Type Together, for Google Fonts), **Inter** (Rasmus Andersson), **Fira Code**
(Nikita Prokopov et al.), and **Atkinson Hyperlegible** (Braille Institute of
America — the low-vision face).

Three further faces are bundled for everyone and offered to nobody who cannot
read them, because none of the seven above contains a single glyph of their
scripts: **Amiri** (Khaled Hosny — the naskh face that carries the Van Dyck,
and the one that positions its tashkeel properly), **Noto Serif Gurmukhi** and
**Noto Serif Devanagari** (Google, for ਪਵਿੱਤਰ ਬਾਈਬਲ and पवित्र बाइबल). All
three are OFL, and each licence travels with its source at
`apps/web/fonts-src/OFL-*.txt`. Which face a reader is offered follows the
SCRIPT of their language and not its direction — `core::font::Font::offered_for`
is the one rule, and `core::i18n::Script` is what it reads.
