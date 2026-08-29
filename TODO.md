# TODO — languages

Order: **Punjabi → Hindi → French → Chinese → the rest.** Every one has a qualifying text
today.

## The rule

A corpus is a candidate only if its New Testament is **Textus Receptus** and its Old
Testament **Masoretic** — the KJV, Reina-Valera, Luther and Van Dyck line — and its licence
permits redistribution inside an offline app.

**The gate is Acts 8:37.** Present as in the KJV or the text is rejected. This is stricter
than "TR or Majority Text": Acts 8:37 is a TR reading that the Byzantine majority does
*not* carry, so a Majority Text Bible fails this test, and that is intended.

Masoretic is not a live question — every candidate here is a Protestant translation and
they all use it.

## Verified

`\v` markers counted in the downloaded USFM. KJV Acts: **40 verses in ch8, 1007 in the book.**

| Language | Text | Licence | Acts ch8 | Acts total | 8:37 |
|---|---|---|---|---|---|
| Punjabi | Pavitr Bible 1945 — `tfbf/punjabi_bible_1945` | PD / CC-BY-SA (conflict) | 40 | 1007 | ✅ |
| Punjabi | `FreeBiblesIndia/Punjabi_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Hindi | `FreeBiblesIndia/Hindi_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Tamil | `FreeBiblesIndia/Tamil_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Gujarati | `FreeBiblesIndia/Gujarati_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Urdu-Devanagari | `FreeBiblesIndia/Urdu_Devanagari_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Malayalam | `FreeBiblesIndia/Malayalam_Bible` | CC-BY-SA 4.0 | 40 | **1004** | ✅ |
| Malayalam | Sathyavedapusthakam 1910 — `tfbf/…-1910` | PD | 40 | **1005** | ✅ |
| French | Ostervald — `open-bibles/fra-ostervald.osis.xml` | PD | — | — | ✅ |
| Chinese | CUV 1919 — `open-bibles/chi-cuv{,-simp}.usfx.xml` | PD | — | — | ✅ |

TR marker sweep run against Punjabi 1945 and Hindi FBI — **all present in both**: Mt 17:21,
Mt 18:11, Mt 23:14, Mk 16:20, Lk 17:36, Rom 16:24, Acts 8:37. Both are Textus Receptus.

## 0. The checker — first

Port `data-prep/rv1909/check-rv1909.py` into a general tool. Given a corpus, verify:

- all 66 books, every chapter count and every last-verse number against the KJV;
- the TR discriminators — Acts 8:37, Mt 17:21 / 18:11 / 23:14, Mk 7:16 / 9:44 / 9:46 /
  11:26 / 15:28 / 16:9–20, Lk 17:36 / 23:17, Jn 5:4 / 7:53–8:11, Acts 15:34 / 24:7 / 28:29,
  Rom 16:24, and 1 Jn 5:7 (the Comma — TR-only, informative rather than disqualifying).

The table above is single-verse spot checks; nothing is confirmed across 66 books until
this runs. One question already waiting for it: **both Malayalam texts are short in Acts**
— 1004 and 1005 against 1007, with ch8 complete — so there are gaps elsewhere in the book.
That is a hole at a valid KJV address, which weaves, the Treasury cross-references and the
stock study set can all link into.

A textual finding counts only if a script produced it from a local file.

## 1. Punjabi

- **Text:** `tfbf/punjabi_bible_1945`, USFM. Passes the full marker sweep.
- **Settle the licence first.** The repo's `LICENSE.md` asserts public domain under Indian
  copyright law; the USFM headers carry CC-BY-SA 4.0. Either ships — they differ only in
  whether attribution and share-alike are owed.
- `FreeBiblesIndia/Punjabi_Bible` is a revision of the same base, passes equally, and has an
  unambiguous CC-BY-SA 4.0. Choose on readability.
- **Gurmukhi: do not Unicode-normalise.** Composition exclusions; normalising corrupts the
  text. Flagged by the source repo.
- New font subset; shares nothing with the Devanagari family.

## 2. Hindi

- **Text:** `FreeBiblesIndia/Hindi_Bible`, USFM, CC-BY-SA 4.0. Passes the full marker sweep.
- Devanagari — also buys Marathi and Urdu-Devanagari later.

## 3. French

- **Text:** `open-bibles/fra-ostervald.osis.xml`, public domain.
- **OSIS XML**, not USFX like `spa-rv1909.usfx.xml` — different parse.
- Gate: Acts 8:37 present — *"Et Philippe lui dit: Si tu crois de tout ton cœur, cela t'est
  permis."* Ostervald is the French TR line, the Geneva/Olivétan tradition.
- Not Louis Segond; Segond is eclectic and critical-leaning.
- Latin script: no shaping, no new font, no layout work. Cheapest row on the list.

## 4. Chinese — Traditional and Simplified

- **Text:** CUV 1919 (和合本) — `open-bibles/chi-cuv.usfx.xml`, `chi-cuv-simp.usfx.xml`,
  public domain. 1919 clears the US 95-year term outright; corroborated by `getbible/v2`
  (`cut`, `cus`, `chiunl`).
- **USFX, the same format as `spa-rv1909.usfx.xml`** — `build-rv1909.py` is the template.
- **Take the 1919 text, not a modern edition of it.** The Revised CUV (2010) is in
  copyright, and modern re-punctuated or character-converted editions can carry a fresh
  claim on the derivative. The About screen names the text, so this is a truthfulness
  problem before it is a legal one.
- One translation reaches Mandarin and Cantonese readers both.
- **Layout work is the cost of this row.** No spaces, so break opportunities stop being the
  same list as tokens: line breaks fall between almost any two characters (kinsoku rules),
  while Strong's attachment and tapping want multi-character words. Prototype against
  `crates/layout` before committing. Also needs word segmentation and a CJK font subset —
  1–2 MB, bounded by the CUV's character set.

## 5. Other Languages

- **Tamil, Gujarati, Urdu-Devanagari** — pass with a complete Acts under CC-BY-SA 4.0.
  Public-domain alternatives worth checking, since PD carries no obligations:
  `tfbf/Bible-Tamil-Sathiyavedam-1957`, `tfbf/Bible-Gujarati-Pavitr-1908` (MIT).
- **Malayalam** — passes 8:37 but is short elsewhere in Acts. Run the checker and compare
  the FBI text against `tfbf/Bible-Malayalam-Sathyavedapusthakam-1910` before committing.
- **Unchecked tfbf public-domain texts**, pre-1960: Bengali 1909, Telugu 1929 (MIT),
  Kannada 1951, Assamese 1951, Odia 1958 (MIT).

## Cross-cutting

**Per script, not per language.** Only Devanagari amortises (Hindi, Marathi,
Urdu-Devanagari, Nepali). Gurmukhi, Tamil, Gujarati, Malayalam and Chinese are each their
own script and font. French adds nothing.

**Re-cost the Indic scripts against the Arabic release.** RTL, complex-script shaping and
non-Latin font subsetting are no longer greenfield if Arabic shipped. Chinese is unaffected
— its problem is line-breaking and segmentation, not shaping.

**Grapheme clusters.** Indic scripts break any code that treats one Rust `char` as one
visible character. Audit the memorize drill first — blanking or revealing by "first letter"
means first *cluster*, and getting it wrong renders broken glyphs rather than a hint. Then
verse-preview truncation and search highlighting.

**No Strong's tags exist for any of these texts.** Every language here ships reader-only:
reading, search, notes, tags, weaves, memorization, plans and the reading map all work, and
every word tap answers `study.noStrongs`. Word-level Greek alignment for Indian languages
does exist — `tfbf/irv_ugnt_alignment`, CC-BY-SA — but it is aligned to the IRV, a different
text. Unresolved for French too. The Arabic row set the precedent for refusing
machine-guessed codes.

**i18n catalogue, ~600 keys per language.** `every_shipped_string_is_translated` blocks a
partial catalogue, so it is all-or-nothing per language.

**First-run prose.** Already gated: every language ships with the welcome and curious paths
closed until someone inside that culture writes them, and the devotional booklet is offered
only where it has been translated. No work needed — just do not write those words on
anyone's behalf.

**Licence obligations.** CC-BY-SA rows need attribution in `BIBLIOGRAPHY.md`, a statement
that the text was modified (we tokenize), and the derived corpus shared under the same
licence — no cost beyond what `strongs.json` already imposes. Public-domain rows carry no
obligation, which is why they are preferred wherever they pass the gate.

**If a corpus is ever OCR'd here**, no native-reader sign-off blocks shipping. The release
gate is structural — the checker's invariants plus a diff against a parallel text. What
ships instead of a sign-off is honesty: an up-front notice on that translation saying it was
digitised from page scans and errors are possible, with a request that readers report them
on the repo. The readership is the review pass, and reported fixes flow upstream. The
mechanism to build it on already exists: a registry column like
`LexiconSpec.machine_translated`, surfaced when the reader switches to that corpus. Nothing
above needs it.

## Sources

- The Free Bible Foundation — <https://github.com/tfbf>
- Free Bibles India — <https://github.com/FreeBiblesIndia>
- open-bibles — <https://github.com/seven1m/open-bibles>
- getbible translations index — <https://github.com/getbible/v2>
