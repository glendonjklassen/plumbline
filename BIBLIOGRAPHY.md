# Bibliography — data sources & licensing

What Plumbline ships, where each piece comes from, and under what license.
Everything is freely licensed or public domain.

## The text and its apparatus

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `data/kjv.jsonl` | KJV 1769 text, Strong's-tagged, tokenized (`kjv1769-tok2`) | eBible.org SWORD module `engKJV2006eb` 14.3, converted by the overlay import pipeline (see below) | KJV text: public domain (Crown patent applies in the UK) |
| `data/strongs.json` | Strong's Hebrew + Greek dictionaries (14,197 entries) | [openscriptures/strongs](https://github.com/openscriptures/strongs) | CC-BY-SA |
| `data/derivation-families.json` | One parent list per Strong's number — the explicit, unhedged derivations the dictionary states — behind the word study's "Same root (Strong's)" row | derived from `data/strongs.json` by `data-prep/strongs/derivations.py`, hand decisions in `data-prep/strongs/derivation-overrides.json` | as Strong's (public domain) |
| `data/kjv-notes.jsonl` | The 1769 translators' margin notes — 6,959 of them, Genesis to Malachi | the `<note>` markup carried inside that same `engKJV2006eb` 14.3 dump, lifted in the same pass (see below) | public domain |
| `data/akjv.jsonl` | Plain-English overlay: where the AKJV words a verse differently, keyed to KJV token spans (6.9% of tokens) | American King James Version, Michael Peter Engelbrite 1999, via [scrollmapper/bible_databases](https://github.com/scrollmapper/bible_databases); aligned by `scripts/build-akjv-delta.mjs` | **public domain** — released 1999-11-08, "copy it, sell it, modify it" |

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
`data/` is committed to this repo, so nothing re-fetches it.

## The German text and its apparatus

| Shipped file | Contents | Source | License |
|---|---|---|---|
| `data/luther1912.jsonl` (text) | Luther 1912 at KJV addresses, tokenized (`luther1912-tok1`) | The Unbound Bible (Biola University) `luther_1912` module, via [kesaranb/luther1912](https://github.com/kesaranb/luther1912); built by `data-prep/luther/build-luther.py` | public domain (module metadata: "This Bible is in the Public Domain") |
| `data/luther1912.jsonl` (Strong's tags) | ~344k Strong's tags merged onto the tokens | Zefania XML *Luther 1912 mit Strongs* (`SF_2022-02-27_GER_LUTH1912_Strongs_xml_220227.zip`, [zefania-sharp on SourceForge](https://sourceforge.net/projects/zefania-sharp/files/Bibles/GER/Lutherbibel/Luther%201912/)), creator/publisher www.toledot.info; merged by `data-prep/luther/merge-strongs.py` (book-level token alignment, ~98.3% of the source's tags; unmatched words stay untagged, and a tag the KJV verse does not carry is dropped) | public domain (the file's own header: "This Text is in the Public Domain") |
| `data/strongs-de.json` | German Strong's dictionary: machine-translated definitions + Luther renderings derived from the tagged corpus | translation of `data/strongs.json` by `data-prep/strongs-lang/translate.py` (Claude, Batch API; labelled as machine-translated in the app) + renderings computed by `build-strongs.py` | **CC-BY-SA** — a derivative of openscriptures/strongs, share-alike carries over |
| `data/rv1909.jsonl` (text + Strong's tags) | Reina-Valera 1909 at KJV addresses, tokenized (`rv1909-tok1`), with the source's own inline Strong's tags on each phrase's head word | eBible.org USFX edition via [seven1m/open-bibles](https://github.com/seven1m/open-bibles) (`spa-rv1909.usfx.xml`); built by `data-prep/rv1909/build-rv1909.py`, proved by `check-rv1909.py` | public domain (the 1909 revision; the 1960 is not) |
| `data/strongs-es.json` | Spanish Strong's dictionary: machine-translated definitions + Reina-Valera renderings derived from the tagged corpus | translation of `data/strongs.json` by a Claude Sonnet subagent fleet, 2026-08-16 (same prompt and validation as `data-prep/strongs-lang/translate.py`, which remains the reproducible path; labelled as machine-translated in the app) + renderings computed by `build-strongs.py` from `data/rv1909.jsonl` | **CC-BY-SA** — a derivative of openscriptures/strongs, share-alike carries over |
| `data/svd1865.jsonl` | Smith & Van Dyck 1865 at KJV addresses, tokenized (`svd1865-tok1`), fully vocalized; two split verses merged back to the KJV address | eBible.org USFM edition `arb-vd`; built by `data-prep/svd/build-svd.py`, proved by `check-svd.py` | public domain (the 1865 text; eBible's `copr.htm` and catalogue both say so) |
| `data/pan-fbi.jsonl` | ਪਵਿੱਤਰ ਬਾਈਬਲ (Punjabi) at KJV addresses, tokenized (`pan-fbi-tok1`); two split verses merged back to the KJV address | [FreeBiblesIndia/Punjabi_Bible](https://github.com/FreeBiblesIndia/Punjabi_Bible), original work available at <http://freebiblesindia.in>; built by `data-prep/indic/build-indic.py`, proved by `check-indic.py`. **Modified**: the text is tokenized into the frozen `kjv.jsonl` shape, its two split verses are merged to the KJV address, and the publisher's footnotes, section headings and book introductions are not carried. | **CC BY-SA 4.0** — attribution above, share-alike carries to the tokenized corpus |
| `data/hin-fbi.jsonl` | पवित्र बाइबल (Hindi) at KJV addresses, tokenized (`hin-fbi-tok1`); two split verses merged back to the KJV address | [FreeBiblesIndia/Hindi_Bible](https://github.com/FreeBiblesIndia/Hindi_Bible), original work available at <http://freebiblesindia.in>; built by `data-prep/indic/build-indic.py`, proved by `check-indic.py`. **Modified**: as above, and the source's inline cross-reference apparatus (`\bdit`, 2,513 spans) is dropped with the footnotes. | **CC BY-SA 4.0** — attribution above, share-alike carries to the tokenized corpus |
| `data/ost1996.jsonl` | La Bible Ostervald, 1996 revision, moved onto KJV addresses, tokenized (`ost1996-tok1`); 62 psalm superscriptions folded into verse 1 as title tokens, six merges, three splits, and the 1,263 addresses whose printed French number differs recorded in `crates/core/src/versification/ostervald-numbering.tsv` | [seven1m/open-bibles](https://github.com/seven1m/open-bibles) OSIS edition (`fra-ostervald.osis.xml`); built by `data-prep/ostervald/build-ostervald.py`, proved by `check-ostervald.py` (every book's letter stream identical to the source's) | public domain (per the open-bibles catalogue; the edition is the 1996 revision and the app says so) |
| `data/cuv1919t.jsonl` | 和合本 1919 (Chinese Union Version, traditional) at KJV addresses, tokenized one character per token (`cuv1919t-tok1`); John 7:53 split back out of 8:1, 1 Chr 22 realigned, 3 John 15 merged; the printed edition's 69 ranged-verse placeholders (併於上節) carried verbatim, two constructed at range tails, all 22 differing printed numbers in `crates/core/src/versification/cuv-numbering.tsv` | [seven1m/open-bibles](https://github.com/seven1m/open-bibles) USFX edition (`chi-cuv.usfx.xml`); built by `data-prep/cuv/build-cuv.py`, proved by `check-cuv.py` | public domain (the 1919 text — the 2010 Revised CUV is in copyright and is not this text) |
| `data/cuv1919s.jsonl` | 和合本 1919, simplified-character edition, as above (`cuv1919s-tok1`); proved parallel to the traditional edition token-for-token. **Modified**: two 2013 规范字 codepoints in CJK Extension B that virtually no font carries (𫈟, 𫗪; twelve occurrences) ship as their traditional forms 蔯/餵 | as above (`chi-cuv-simp.usfx.xml`); same build and check scripts | public domain (the 1919 text) |

## Removed data

Removed from the repository and the product on 2026-11-22, with the analysis
tiers that read them (git history has the files and their provenance tables):
the OSHB and Robinson morphology (`data/morphology.jsonl`), the Treasury of
Scripture Knowledge cross-references (`data/cross-references.tsv`), the
cross-testament bridge witnesses (`bridge/`: the LXX alignment, Abbott-Smith,
STEPBible TIPNR), the fitted priors and witness files, and the self-trained
concept embedding (`data/concept-vectors.vec`).

## Study content

The bundled study set is `stock/`, seeded once
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

Four further faces are bundled for everyone and offered to nobody who cannot
read them, because none of the seven above contains a single glyph of their
scripts: **Amiri** (Khaled Hosny — the naskh face that carries the Van Dyck,
and the one that positions its tashkeel properly), **Noto Serif Gurmukhi** and
**Noto Serif Devanagari** (Google, for ਪਵਿੱਤਰ ਬਾਈਬਲ and पवित्र बाइबल), and
**Noto Serif TC** (Google/Adobe — Source Han Serif's traditional cut, carrying
both editions of the 和合本; subset to the exact characters of the two corpora
and two catalogues, ~1 MB, by `apps/web/scripts/subset-fonts.mjs`, which
fetches the 24 MB upstream OTF sha256-pinned rather than committing it). All
four are OFL, and each licence travels at `apps/web/fonts-src/OFL-*.txt`.
Which face a reader is offered follows the SCRIPT of their language and not
its direction — `core::font::Font::offered_for` is the one rule, and
`core::i18n::Script` is what it reads.
