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

## Shipped

**Punjabi and Hindi are in.** `data/pan-fbi.jsonl` and `data/hin-fbi.jsonl`,
built by `data-prep/indic/build-indic.py`, proved by `check-indic.py`, both at
KJV addresses with all twenty TR discriminators present. Rows in
`crates/core/src/i18n.rs`, 868 catalogue keys each, Noto Serif Gurmukhi and
Noto Serif Devanagari bundled, `apps/web/e2e/indic.spec.ts` green.

The Punjabi text is **not** the 1945 one this file used to name. See §1.

## Verified

`\v` markers counted in the downloaded USFM. KJV Acts: **40 verses in ch8, 1007 in the book.**

| Language | Text | Licence | Acts ch8 | Acts total | 8:37 |
|---|---|---|---|---|---|
| Punjabi | **`FreeBiblesIndia/Punjabi_Bible` — SHIPPED** | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Punjabi | Pavitr Bible 1945 — `tfbf/Bible-Punjabi-Pavitr-Bible-1945` | PD / CC-BY-SA (conflict) | 40 | 1007 | ⚠ spliced |
| Hindi | **`FreeBiblesIndia/Hindi_Bible` — SHIPPED** | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Tamil | `FreeBiblesIndia/Tamil_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Gujarati | `FreeBiblesIndia/Gujarati_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Urdu-Devanagari | `FreeBiblesIndia/Urdu_Devanagari_Bible` | CC-BY-SA 4.0 | 40 | 1007 | ✅ |
| Malayalam | `FreeBiblesIndia/Malayalam_Bible` | CC-BY-SA 4.0 | 40 | **1004** | ✅ |
| Malayalam | Sathyavedapusthakam 1910 — `tfbf/…-1910` | PD | 40 | **1005** | ✅ |
| French | Ostervald — `open-bibles/fra-ostervald.osis.xml` | PD | — | — | ✅ |
| Chinese | CUV 1919 — `open-bibles/chi-cuv{,-simp}.usfx.xml` | PD | — | — | ✅ |

**1 John 5:6–8 is renumbered in every Indian-language text found**, the two
shipped ones included: the KJV's 5:6b sits at 5:7, the Comma Johanneum is
absent, 5:8 realigns. Counts still match the KJV everywhere, so it is a content
misalignment at one address rather than a structural one — a note or weave
anchored at `1John 5:7` shows the Comma in English and "the Spirit is truth"
here. `NumberingSpec` is the wrong shape for it (nothing is renumbered) and both
registry rows say so.

Two splits, in both texts and in opposite directions: 3 John 15 is the tail of
the KJV's v14 and is appended; Rev 12:18 is the **head** of 13:1 and is
prepended. At Rev 13:1a they read "he stood" where the TR has "I stood" — the
one critical reading that shows through the twenty discriminators.

## 0. The checker — done for these two

`data-prep/indic/check-indic.py` verifies all 66 books, every chapter count and
every last-verse number against the KJV, the twenty TR discriminators, that
tokens reassemble, that no word begins inside a grapheme cluster, that NFC and
NFD are both no-ops, and — with the source in hand — that every verse's letters
are the source's.

It also carries the **splice guard** (§1), and the Comma is treated as
informative rather than disqualifying, as agreed.

Still open: **both Malayalam texts are short in Acts** — 1004 and 1005 against
1007, with ch8 complete — so there are gaps elsewhere in the book. Run
`check-indic.py` against them before either is considered; it is the same USFM
shape and the script takes a language code.

A textual finding counts only if a script produced it from a local file.

## 1. Punjabi — done, and not the text this file named

- **Shipped:** `FreeBiblesIndia/Punjabi_Bible`, CC-BY-SA 4.0. Uniform apparatus
  in every book, all five of the publisher's checking stages complete.
- **`tfbf/Bible-Punjabi-Pavitr-Bible-1945` was rejected on the evidence.** Eight
  whole books of it are a different modern translation spliced in — Titus, John,
  James, 1 Peter, 1–2 Thessalonians, 2 Peter, 1 Corinthians, 1,772 verses — plus
  ~217 scattered verses elsewhere. **Acts 8:37 is one of the splices**, so the
  verse the text was accepted for is the one verse in it that cannot be
  attributed to it. Its own `STATUS.md` says the files "are not ready to be used
  in a real project".
  The tell is punctuation: the 1945 keyboarding types the danda as an ASCII `|`
  in 19,306 verses and the spliced material uses a real `।` U+0964. Every other
  check passes on that file, which is why `check-indic.py` now carries the test
  as a standing claim — **a sentence terminator over 1% of a corpus's must be
  used by 90% of its books.** The next Indian-language corpus offered here is
  likely to have been assembled the same way.
- The PD-vs-CC-BY-SA conflict in that repo is moot now; the shipped text's
  licence is unambiguous.
- **Gurmukhi is not normalised**, and the checker asserts it: NFC decomposes the
  precomposed nukta letters, so "normalising" rewrites letters.

## 2. Hindi — done

- **Shipped:** `FreeBiblesIndia/Hindi_Bible`, CC-BY-SA 4.0, the traditional
  पवित्र बाइबल with modernised spelling.
- Devanagari also buys Marathi and Urdu-Devanagari later: the font, the search
  rules and `Script::Devanagari` are all per-script, not per-language, so a
  second Devanagari language is a registry row and a catalogue.

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

**Per script, not per language** — and that is now a column rather than a
convention. `core::i18n::Script` and `core::font::Font::script` are what the
font picker and `readerFace` compare; direction is derived from the script.
Adding a Devanagari language (Marathi, Urdu-Devanagari, Nepali) costs a row and
a catalogue and no font work. Tamil, Gujarati, Malayalam and Chinese are each a
new script, a new face and a new `Script` variant. French adds nothing.

**The Arabic release did not de-risk the Indic scripts as much as expected.**
What ported was the subsetting and the bundle-for-everyone fallback. What did
not: `rtl` was standing in for "script" everywhere, because Arabic was the only
language where the two questions had one answer. Chinese will need the same
audit — its problem is line-breaking and segmentation, not shaping.

**Grapheme clusters — audited, and two were real.** `blank_out` masked on
`is_alphanumeric` and passed everything else through, so a masked Devanagari
word kept its viramas and a masked Arabic one kept its tashkeel, hanging off the
underscores; and `normalize_word` deleted every virama from the search index,
because `is_alphanumeric` is false for one. Both fixed. `first_letters` was
fine — it takes the first alphanumeric char, which is a base consonant.
Underscore counts still run one per codepoint rather than per cluster, which
overstates a masked word's length; exact needs UAX #29 and a dependency this
crate does not take.

**No Strong's tags exist for any of these texts.** Punjabi and Hindi ship
reader-only, as Arabic does — `lexicon: None` on both rows. Every language here
ships reader-only:
reading, search, notes, tags, weaves, memorization, plans and the reading map all work, and
every word tap answers `study.noStrongs`. Word-level Greek alignment for Indian languages
does exist — `tfbf/irv_ugnt_alignment`, CC-BY-SA — but it is aligned to the IRV, a different
text. Unresolved for French too. The Arabic row set the precedent for refusing
machine-guessed codes.

**i18n catalogue, 868 keys per language.** `every_shipped_string_is_translated`
blocks a partial catalogue, so it is all-or-nothing. Book names come free: the
`\h` field of each USFM file is the publisher's own name for the book, though
the long forms ("ਕੁਰਿੰਥੀਆਂ ਨੂੰ ਪਹਿਲੀ ਪੱਤ੍ਰੀ") need shortening to what a picker
can hold.

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
