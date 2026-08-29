# TODO — languages

Order: **Punjabi → Hindi → French → Chinese → the rest.** The first four are
in; every remaining candidate has a qualifying text today.

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
`crates/core/src/i18n.rs`, full catalogues, Noto Serif Gurmukhi and
Noto Serif Devanagari bundled, `apps/web/e2e/indic.spec.ts` green.

**French is in.** `data/ost1996.jsonl` — the Ostervald, 1996 revision, from
`open-bibles/fra-ostervald.osis.xml` as planned, all twenty discriminators
present (Acts 8:37 carries the source's own `cour`-for-`cœur` typo, shipped
as-is). The cheap-row prediction held for fonts and layout and not for
addressing: the source prints French/Hebrew numbering (91 chapters differ —
psalm titles as verse 1, Job recut), so `build-ostervald.py` is the first
script here to move a text onto KJV addresses itself, and French is the first
row since German to fill `numbering` (1,263 annotated addresses;
`check-ostervald.py` proves every book's letter stream identical to the
source's). `apps/web/e2e/french.spec.ts` green.

**Chinese is in, both editions.** `data/cuv1919t.jsonl` + `data/cuv1919s.jsonl`
— the 1919 和合本 from the two open-bibles USFX files, one build script,
proved parallel token-for-token. The layout cost §4 predicted collapsed once
the corpora tokenized PER CHARACTER: break opportunities are token boundaries
again, punctuation glued into pre/post is the kinsoku rule, and the one engine
change was the FFI zeroing `space_width` for a Han corpus the way it derives
`rtl`. Search splits a Han query into per-character words, which turns the
existing phrase tier into the substring search a Chinese reader expects.
`Script::Han`, one face (Noto Serif TC, subset to the corpora + catalogues,
~1 MB), two rows `zht`/`zhs` with locale routing for `zh-*` tags, printed
ranged verses (併於上節) shipped verbatim with a 22-row numbering table.
`apps/web/e2e/cjk.spec.ts` green.

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
a catalogue and no font work — and Chinese proved the pattern from the other
side: traditional and simplified are two ROWS on one `Script::Han`, one face.
Tamil, Gujarati and Malayalam are each a new script, a new face and a new
`Script` variant.

**Chinese's audit found exactly one `rtl`-style conflation left**, and it was
not in layout: the locale base-tag strip (`de-CH` → `de`) lived in three web
mirrors and the engine, and every one of them sent `zh-TW` to a nonexistent
`zh` row. `shippedBase` / `Lang::shipped` now route the zh tags; the layout
crate itself needed nothing — per-character tokens made break opportunities
token boundaries again, and the only engine change was the FFI deriving
`space_width = 0` from the corpus the way it derives `rtl`.

**Grapheme clusters — audited, and two were real.** `blank_out` masked on
`is_alphanumeric` and passed everything else through, so a masked Devanagari
word kept its viramas and a masked Arabic one kept its tashkeel, hanging off the
underscores; and `normalize_word` deleted every virama from the search index,
because `is_alphanumeric` is false for one. Both fixed. `first_letters` was
fine — it takes the first alphanumeric char, which is a base consonant.
Underscore counts still run one per codepoint rather than per cluster, which
overstates a masked word's length; exact needs UAX #29 and a dependency this
crate does not take.

**No Strong's tags exist for any of these texts.** Arabic, Punjabi, Hindi,
French and both Chinese rows all ship reader-only — `lexicon: None` — and
every language here would too: reading, search, notes, tags, weaves,
memorization, plans and the reading map all work, and every word tap answers
`study.noStrongs`. Word-level Greek alignment for Indian languages does exist
— `tfbf/irv_ugnt_alignment`, CC-BY-SA — but it is aligned to the IRV, a
different text. The Arabic row set the precedent for refusing machine-guessed
codes.

**i18n catalogue, ~875 keys per language.** `every_shipped_string_is_translated`
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
