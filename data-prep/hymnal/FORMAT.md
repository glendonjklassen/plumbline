# Hymnal source files

One JSON file per hymn, `data-prep/hymnal/<id>.json`. These are the SOURCE OF
TRUTH for the shipped hymnal; `scripts/build-hymnal.mjs` assembles them into
`data/hymnal.json` (format tag `hymnal-v1`), stripping `sources` and `notes`.
Texts are public domain; the files record where each text was taken from so a
wording question can be settled later.

## Shape

```json
{
  "id": "amazing-grace",
  "number": 14,
  "tune": "NEW BRITAIN",
  "meter": "8.6.8.6",
  "key": "G",
  "texts": {
    "en": {
      "title": "Amazing Grace",
      "author": "John Newton",
      "translator": null,
      "year": 1779,
      "stanzas": [
        "A[G]mazing grace! how [C]sweet the [G]sound,\nThat saved a wretch like [D]me!\nI [G]once was lost, but [C]now am [G]found,\nWas [Em]blind, but [D]now I [G]see.",
        "..."
      ],
      "chorus": null,
      "sources": ["https://hymnary.org/text/amazing_grace_how_sweet_the_sound"]
    },
    "de": null
  },
  "notes": ""
}
```

- `id`, `number` — from WORKLIST.json, verbatim. The number is the hymn's
  stable book number.
- `tune`, `meter` — the tune name (conventional all-caps form, e.g. NICAEA)
  and meter as hymnary.org lists them. Verify against a source; do not guess.
- `key` — the key the chords are written in. Chosen for guitars: take the
  common hymnal key and move it to whichever of G, D, A, E, C (in that order
  of preference) is nearest. One key per hymn; both languages share it.
- `texts.en` / `texts.de` — either an object or null. A translation carries
  `translator` (and `year` of the translation where known); an original
  carries `translator: null`.
- `stanzas` — one string per stanza, lines joined with `\n`. All the
  commonly-printed stanzas of the standard text, in order — do not trim to a
  "greatest hits" subset, and do not invent or paraphrase. Keep the source's
  punctuation and casing (including em dashes if the source has them —
  historical text is quoted, not styled). German texts use today's standard
  sung orthography (Evangelisches Gesangbuch style: ß where standard, modern
  spellings of daß→dass NOT applied when the sung form keeps the older word —
  follow the source).
- `chorus` — a stanza-shaped string for hymns with a refrain, else null. The
  shells repeat it after every stanza.
- `sources` — at least one URL per language actually consulted for the text.

## Chords

Inline ChordPro brackets: `[G]`, `[G7]`, `[Em]`, `[D/F#]`, `[Bb]`. A bracket
sits immediately before the syllable it strikes.

- Chord grammar (the core parser enforces this):
  `[A-G](#|b)? quality* (/[A-G](#|b)?)?` where quality is one of
  `m maj min dim aug sus2 sus4 add9 6 7 9 11 13 maj7 m7 mmaj7 dim7 m7b5 7sus4 aug7`
  — concatenations like `m7`, `maj7`, `7sus4` are fine.
- Voice the harmonization of the named tune, simplified to a playable guitar
  chart: chord changes on the harmonic rhythm (typically 1–2 per bar), standard
  I/IV/V/vi vocabulary with idiomatic 7ths and the occasional slash bass.
  Full SATB passing chords are noise for a strummed instrument.
- Stanza 1 of each language carries the full chart. Later stanzas of the SAME
  melody may omit brackets (the shells reuse stanza 1's chart when a stanza
  has none); a chorus always carries its own chart.
- Both language texts share the tune, so their charts must agree bar-for-bar.

## What not to do

- Do not write a text from memory without a source that confirms it.
- Do not normalize, modernize or "improve" wording.
- Do not add scripture references, themes, or commentary — `notes` is for
  sourcing/uncertainty remarks to the maintainer only.
