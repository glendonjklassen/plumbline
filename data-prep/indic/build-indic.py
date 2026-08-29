#!/usr/bin/env python3
"""Build the Punjabi and Hindi corpora from Free Bibles India's USFM.

    python3 data-prep/indic/build-indic.py pa Punjabi_Bible-master
    python3 data-prep/indic/build-indic.py hi Hindi_Bible-master

Either a directory or a zip of one. The sources:

    https://github.com/FreeBiblesIndia/Punjabi_Bible
    https://github.com/FreeBiblesIndia/Hindi_Bible

both CC BY-SA 4.0, both the traditional Protestant Bible of their language —
ਪਵਿੱਤਰ ਬਾਈਬਲ and पवित्र बाइबल, the texts a Punjabi or Hindi speaking church
reads from — with modernised spelling and an editorial apparatus this build
discards.

ONE SCRIPT FOR TWO LANGUAGES, which is not the pattern the other corpora
follow. `build-luther.py`, `build-rv1909.py` and `build-svd.py` are each their
own file because each source is its own format from its own publisher. These
two are the SAME publisher's export in the same conventions, down to the
markers used and the two verse splits; two files here would be one file and a
copy of it, and the copy is where they would drift.

WHY THESE TEXTS AND NOT THE OLDER ONES. The obvious candidate for Punjabi was
tfbf/Bible-Punjabi-Pavitr-Bible-1945, a volunteer digitisation of a 1945 print
that is public domain outright. It was rejected on the evidence: eight whole
books of it (Titus, John, James, 1 Peter, 1-2 Thessalonians, 2 Peter,
1 Corinthians — 1,772 verses) are not the 1945 text at all but a modern
translation spliced in, along with ~217 scattered verses elsewhere. The tell is
punctuation: the 1945 keyboarding types the danda as an ASCII "|" in 19,306
verses, and the spliced material uses a real ਦ U+0964 danda and ASCII quotes.
Acts 8:37 is one of the splices. Its own STATUS.md says the files "are not
ready to be used in a real project". `check-indic.py` carries that test as a
standing guard, because the next Indian-language corpus offered to this app is
likely to have been assembled the same way.

WHAT THESE SOURCES GIVE:

  - THEY SIT AT KJV VERSE ADDRESSES. 66 books, 1,189 chapters, and 31,102 of
    their 31,104 verses at `data/kjv.jsonl`'s address. The two extras are
    splits — see MERGES.
  - THEY ARE TEXTUS RECEPTUS. All twenty verses a critical text omits or
    brackets are present and carry real words: Matt 17:21, 18:11, 23:14; Mark
    7:16, 9:44, 9:46, 11:26, 15:28, 16:9-20; Luke 17:36, 23:17; John 5:4,
    7:53-8:11; Acts 8:37, 15:34, 24:7, 28:29; Rom 16:24. `check-indic.py`
    proves it rather than taking the publisher's word.
  - THEY SPELL THE DIVINE NAME. ਯਹੋਵਾਹ and यहोवा, ~6,900 and ~7,000 times
    against the KJV's 6,892 — so unlike Arabic, where ٱلرَّبّ renders YHWH and
    Adonai alike and could not be flagged, FLAG_DIVINE means here exactly what
    it means in the KJV.
  - THEY MARK PSALM SUPERSCRIPTIONS with `\\d`, folded into verse 1 and flagged
    the way `kjv.jsonl` folds them.
  - THEY HAVE REAL PROSE PARAGRAPHS (`\\p`, `\\m`), at a rate near the KJV's own.

WHAT THEY DO NOT GIVE:

  - NO STRONG'S TAGS, and none are invented. Both registry rows carry
    `lexicon: None`, as Arabic's does. A word study in Punjabi or Hindi is
    honestly absent rather than machine-guessed.
  - NO TRANSLATOR-SUPPLIED-WORD MARKUP. `\\it` in these files is emphasis and a
    footnote anchor, not USFM's `\\add`; FLAG_ADDED is never set.

TWO THINGS THAT ARE DISCARDED, and they are the reason this build is not a
three-line loop over `\\v`:

  - FOOTNOTES (`\\f … \\f*`): 2,951 in Hindi, 469 in Punjabi, and they sit
    INSIDE the verse, mid-sentence. A parser that only drops markers keeps
    every word of the note as scripture.
  - CROSS-REFERENCES (`\\bdit … \\bdit*`): 2,513 spans in Hindi, all of them a
    parenthesised reference list like "(इब्रा. 1:10, इब्रा. 11:3)". Punjabi
    has none. Same hazard, and this one looks like text.

  Both are spans with an end marker, so they are cut whole. Every other
  character style (`\\wj` words of Jesus, `\\it`, `\\qs` Selah, `\\tl`) keeps its
  text and loses its marker, because this token model has nowhere to put the
  distinction and the words are scripture either way.

THE TEXT IS NOT NORMALISED. The Punjabi source repo warns in capitals that
Unicode normalisation must not be applied to Gurmukhi: NFC decomposes the
precomposed nukta letters, because they are on Unicode's composition exclusion
list, so "normalising" would silently rewrite letters. Devanagari is on the
same list. Both files are already stable under NFC and NFD — measured, not
assumed — so passing them through unchanged costs nothing and removes the one
line a later maintainer might add by habit. `check-indic.py` asserts it.

The output is `kjv.jsonl`'s frozen shape (CLAUDE.md §Data formats): a header
line, then one verse per line with positional tokens
`[pre, word, post, [strongs], flags]`. Of the flag bits, DIVINE NAME (2), TITLE
(4) and PARAGRAPH (8) are set.
"""

import json
import re
import sys
import unicodedata
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

FLAG_DIVINE = 2
FLAG_TITLE = 4
FLAG_PARA = 8


class Spec:
    """One language's row: everything that differs between the two builds."""

    def __init__(self, code: str, out: str, tokenization: str, label: str, divine: str, source: str):
        self.code = code
        self.out = ROOT / "data" / out
        self.tokenization = tokenization
        self.label = label
        self.divine = divine
        self.source = source


LANGS = {
    "pa": Spec(
        "pa",
        "pan-fbi.jsonl",
        "pan-fbi-tok1",
        "ਪਵਿੱਤਰ ਬਾਈਬਲ",
        "ਯਹੋਵਾਹ",
        "ਪਵਿੱਤਰ ਬਾਈਬਲ (Free Bibles India, Punjabi; CC BY-SA 4.0)",
    ),
    "hi": Spec(
        "hi",
        "hin-fbi.jsonl",
        "hin-fbi-tok1",
        "पवित्र बाइबल",
        "यहोवा",
        "पवित्र बाइबल (Free Bibles India, Hindi; CC BY-SA 4.0)",
    ),
}

# THE TWO VERSE SPLITS, and the direction each one merges.
#
# Both sources number 31,104 verses to the KJV's 31,102, and both extras are the
# same thing the Van Dyck's were: a verse the KJV ends one clause later, given a
# number of its own. Every refKey in this app is frozen KJV addressing, so the
# text is merged back to the KJV address rather than the corpus addressing two
# verses differently from every other corpus.
#
# THE DIRECTION IS NOT THE SAME FOR BOTH, which `build-svd.py` never had to say
# because both of its merges ran the same way:
#
#   3 John 1:15 -> 1:14, APPENDED.   "Peace be to thee. Our friends salute
#                                     thee…" is the tail of the KJV's v14.
#   Rev 12:18   -> 13:1, PREPENDED.  "And he stood upon the sand of the sea" is
#                                     the HEAD of the KJV's 13:1, so appending
#                                     it would put the first clause of the verse
#                                     after the last one and read as nonsense.
#
# A note on that clause: these texts read "he stood", where the Textus Receptus
# has "I stood" (ἐστάθην). It is the one place in either New Testament where a
# critical reading shows through the twenty discriminators `check-indic.py`
# tests. Recorded rather than corrected — this build does not edit scripture.
MERGES = {
    ("3John", 1, 15): ("3John", 1, 14, "append"),
    ("Rev", 12, 18): ("Rev", 13, 1, "prepend"),
}

# Punctuation that belongs beside a word rather than being a word of its own.
#
# THE HYPHEN IS NOT HERE, and that is the one difference from `build-svd.py`'s
# lists that matters. Arabic uses it as a parenthetical dash pinned to a word
# edge, so that build peels it; Hindi and Punjabi use it INSIDE a word, for the
# reduplication both languages are full of — "चलते-चलते", "ਸੁੱਖ-ਸਾਂਦ", 3,263
# and 4,764 of them. Peeling only happens at the edges, so a word-internal
# hyphen would survive either way; naming it as punctuation would break the 17
# words that do end in one.
POST = ".,;:!?)]»\u201d\u2019\u0964\u0965\u2013\u2014"
PRE = "([«\u201c\u201e\u2018"


# Markers whose text is NOT scripture, cut span and all. See the header.
SPANS = re.compile(r"\\\+?f\s.*?\\\+?f\*|\\\+?bdit\b.*?\\\+?bdit\*", re.S)
# Any remaining marker, including USFM's nested `\+wj` form. The text around it
# is kept.
MARKER = re.compile(r"\\\+?[a-z]+\d*\*?\s?")

ID = re.compile(r"\\id\s+(\S+)")
CHAPTER = re.compile(r"\\c\s+(\d+)")
# Verse markers are NOT always at the start of a line: the Punjabi source writes
# "\p \v 1 …" 507 times. Splitting on the marker anywhere handles both.
VERSE = re.compile(r"\\v\s+(\d+)\s")
# `\d` is a psalm superscription and `\qa` an ACROSTIC STANZA HEADING — the 22
# Hebrew letter names that open each eight-verse block of Psalm 119. They are
# the same thing to this token model, and `kjv.jsonl` already says so: its
# Ps 119:1 opens with "א" and "ALEPH" carrying FLAG_TITLE, exactly as Ps 3:1
# opens with its superscription. So both fold into the verse that follows and
# both are flagged. The Punjabi source marks all 22; the Hindi source marks
# none, and neither is an error.
TITLE = re.compile(r"\\(?:d|qa)\s+(.*)", re.S)
# Prose paragraph openings. `\q` IS NOT ONE, and it is not skippable either:
# it is a POETRY LINE, one per line of verse — 12,080 of them in Hindi against
# 5,538 `\p` — so treating each as a paragraph would mark 40% of the Psalms as
# opening one, and dropping the line loses the second half of nearly every
# verse in the Psalms. It falls through to the continuation branch, which keeps
# the text and marks nothing. (Caught by the round-trip check, which found
# Ps 23:1 ending at "the LORD is my shepherd," — `\q` had been in SKIP.)
PARA = re.compile(r"\\(p|m|pi\d?|nb|pc|pr)\b")
# NOT VERSE TEXT. Same list as `build-svd.py`'s and the same reason — the
# fallback branch keeps whatever text a line carries, because that is how a
# verse continued after a `\p` survives — plus the book INTRODUCTIONS these
# sources carry (`\is` headings, `\ip` paragraphs, `\iot`/`\io1` outlines). That
# apparatus is a modern publisher's, it runs to 400 paragraphs, and it sits
# between `\id` and the first `\c` where nothing else would drop it. `\b` is a
# stanza break and carries no text of its own.
SKIP = re.compile(r"\\(id|ide|h|toc\d?|mt\d?|ms\d?|mr|s\d?|sr|r|sp|cl|cp|rem|is\d?|ip[a-z]*|io\d?|iot|ib|b)\b")


def osis_order(kjv: Path) -> list[str]:
    """The 66 OSIS ids in canon order, taken from the KJV corpus itself.

    Read rather than restated, for `build-svd.py`'s reason: this script's whole
    claim is that its output sits at `kjv.jsonl`'s addresses, and a hand-typed
    list here would be a second source for that fact.
    """
    order: list[str] = []
    with kjv.open(encoding="utf-8") as f:
        next(f)
        for line in f:
            b = json.loads(line)["b"]
            if b not in order:
                order.append(b)
    return order


def has_letter(s: str) -> bool:
    return any(unicodedata.category(c).startswith("L") for c in s)


def tokenize(text: str, spec: Spec, lead_flags: int, title: bool) -> list:
    """Verse text → `[pre, word, post, [strongs], flags]` tokens.

    Whitespace-split, then punctuation peeled off each end. Gurmukhi and
    Devanagari are written with spaces between words exactly as English is, so
    nothing more is needed — the conjuncts and matras that make a syllable are
    all INSIDE a word, and no rule here may look at a character in isolation.

    The code list is ALWAYS empty; see the header on Strong's.
    """
    tokens: list = []
    first = True
    carry = ""
    for raw in text.split():
        pre = carry
        carry = ""
        post = ""
        word = raw
        while word and word[0] in PRE:
            pre += word[0]
            word = word[1:]
        while word and word[-1] in POST:
            post = word[-1] + post
            word = word[:-1]
        if not has_letter(word):
            # Punctuation standing alone between two spaces — a stray dash, or
            # an opening quote the source separates from the word it opens
            # ("\wj “ इस रीति से…", 42 verses in Hindi). It is never a token of
            # its own: an empty `word` is a tap target that leads nowhere and an
            # index entry no search can match.
            #
            # CARRIED FORWARD, not hung on the token behind. A "“" belongs to
            # the word it opens, and at the head of a verse there is nothing
            # behind it to hang on — which is exactly where these 42 are.
            carry = pre + word + post
            continue
        flags = 0
        if title:
            flags |= FLAG_TITLE
        if first:
            flags |= lead_flags
            first = False
        if word == spec.divine:
            flags |= FLAG_DIVINE
        tokens.append([pre, word, post, [], flags])
    if carry:
        # Trailing punctuation with no word after it. Now there IS something
        # behind it, so the verse still reassembles character for character.
        if tokens:
            tokens[-1][2] += carry
        else:
            tokens.append(["", carry, "", [], lead_flags])
    return tokens


def parse_book(body: str) -> dict[tuple[int, int], tuple[str, bool, str]]:
    """One book's USFM → `(chapter, verse) → (text, starts_paragraph, title)`.

    A verse's text runs from its `\\v` until the next `\\v` or `\\c` — NOT until
    the next marker, because `\\p` and the poetry markers interrupt a verse
    mid-sentence and the rest of it follows them. And a `\\v` can appear part
    way along a line, so every line is split on the marker rather than matched
    against it.
    """
    body = SPANS.sub(" ", body)
    out: dict[tuple[int, int], tuple[str, bool, str]] = {}
    chapter = 0
    verse = 0
    buf: list[str] = []
    para = False
    pending_para = False
    title = ""
    pending_title = ""

    def flush() -> None:
        if verse:
            out[(chapter, verse)] = (" ".join(buf).strip(), para, title)

    for line in body.splitlines():
        parts = VERSE.split(line)
        head = parts[0]
        # Everything before the line's first `\v` belongs to whatever came
        # before it: a chapter change, a paragraph mark, a superscription, a
        # skipped heading, or the continuation of the open verse.
        if m := CHAPTER.search(head):
            flush()
            buf, verse, para, title = [], 0, False, ""
            chapter = int(m.group(1))
            pending_para = False
            pending_title = ""
        elif m := TITLE.match(head.strip()):
            # `\d` sits between `\c` and `\v 1`; it belongs to the verse that
            # follows, which is how `kjv.jsonl` carries superscriptions —
            # prepended into verse 1 and flagged, not a verse of their own.
            # Appended rather than assigned, because a superscription can run
            # to more than one line.
            pending_title = (pending_title + " " + MARKER.sub(" ", m.group(1))).strip()
        elif SKIP.match(head.strip()):
            pass
        elif PARA.match(head.strip()):
            # Before a verse it marks that verse as opening a paragraph; inside
            # one it is a mid-verse break this token model has no place for, and
            # the remaining text simply continues. AND THE REST OF THE LINE IS
            # VERSE TEXT — `build-svd.py` learned that one the hard way, where
            # 50 verses lost a clause riding on a `\p` line.
            rest = MARKER.sub(" ", head).strip()
            if verse:
                buf.append(rest if rest else "")
            elif rest:
                pending_title = (pending_title + " " + rest).strip()
            pending_para = True
        elif verse and (rest := MARKER.sub(" ", head).strip()):
            buf.append(rest)
        for i in range(1, len(parts), 2):
            flush()
            verse = int(parts[i])
            buf = [MARKER.sub(" ", parts[i + 1]).strip()]
            para = pending_para
            title = pending_title
            pending_para = False
            pending_title = ""
    flush()
    return out


def read_source(src: str) -> list[tuple[str, str]]:
    """`(name, text)` for the 66 USFM files, in filename order.

    Filename order IS canon order for these sources — 01_GEN…67_REV, Paratext's
    numbering, which skips 40 — and the `\\id` line is checked against
    `kjv.jsonl`'s book order by the caller rather than trusted here.

    THE BYTES ARE NOT NORMALISED. See the header: NFC would rewrite Gurmukhi
    letters that are on Unicode's composition exclusion list.
    """
    p = Path(src)
    if p.is_dir():
        names = sorted(str(f.relative_to(p)) for f in p.rglob("*") if f.suffix.lower() in (".usfm", ".sfm"))
        return [(n, (p / n).read_text(encoding="utf-8")) for n in names]
    with zipfile.ZipFile(src) as z:
        names = sorted(n for n in z.namelist() if n.lower().endswith((".usfm", ".sfm")))
        return [(n, z.read(n).decode("utf-8")) for n in names]


def main(code: str, src: str) -> int:
    spec = LANGS.get(code)
    if spec is None:
        print(f"unknown language {code!r}; expected one of {', '.join(LANGS)}", file=sys.stderr)
        return 2

    order = osis_order(ROOT / "data" / "kjv.jsonl")
    if len(order) != 66:
        print(f"expected 66 books in kjv.jsonl, found {len(order)}", file=sys.stderr)
        return 1

    books = read_source(src)
    if len(books) != 66:
        print(f"expected 66 USFM files in {src}, found {len(books)}", file=sys.stderr)
        return 1

    out_lines: list[str] = []
    counts = {"title": 0, "para": 0, "divine": 0, "merged": 0, "verses": 0}
    for i, (name, raw) in enumerate(books):
        osis = order[i]
        verses = parse_book(raw)
        # Tokenize first, then move the two splits, so a merge carries real
        # tokens rather than re-tokenized text at a different flag state.
        prepend: dict[tuple[int, int], list] = {}
        append: dict[tuple[int, int], list] = {}
        rows: list[tuple[int, int, list]] = []
        for (c, v), (text, para, title) in sorted(verses.items()):
            title_tokens = tokenize(title, spec, 0, True) if title else []
            # A titled verse never also opens a paragraph: no token in
            # `kjv.jsonl` carries both bits, and the superscription is the block
            # opening in any case.
            lead = 0 if title_tokens else (FLAG_PARA if para else 0)
            tokens = title_tokens + tokenize(text, spec, lead, False)
            if target := MERGES.get((osis, c, v)):
                _, tc, tv, how = target
                # The merged text is a continuation, never its own paragraph.
                if tokens:
                    tokens[0][4] &= ~FLAG_PARA
                (prepend if how == "prepend" else append).setdefault((tc, tv), []).extend(tokens)
                counts["merged"] += 1
                continue
            rows.append((c, v, tokens))
        for c, v, tokens in rows:
            head = prepend.get((c, v), [])
            if head:
                # The receiving verse's own first token loses the paragraph bit
                # to the clause that now opens it.
                if tokens and tokens[0][4] & FLAG_PARA:
                    tokens[0][4] &= ~FLAG_PARA
                    head[0][4] |= FLAG_PARA
            tokens = head + tokens + append.get((c, v), [])
            counts["title"] += sum(1 for t in tokens if t[4] & FLAG_TITLE)
            counts["para"] += 1 if tokens and tokens[0][4] & FLAG_PARA else 0
            counts["divine"] += sum(1 for t in tokens if t[4] & FLAG_DIVINE)
            counts["verses"] += 1
            out_lines.append(
                json.dumps({"b": osis, "c": c, "v": v, "t": tokens}, ensure_ascii=False, separators=(",", ":"))
            )
        _ = name

    header = json.dumps(
        {
            "format": "overlay-kjv-canonical",
            "source": spec.source,
            "tokenization": spec.tokenization,
            "verses": len(out_lines),
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
    spec.out.write_text(header + "\n" + "\n".join(out_lines) + "\n", encoding="utf-8")
    print(f"wrote {spec.out.relative_to(ROOT)}: {len(out_lines)} verses")
    print(
        f"  title tokens {counts['title']}, paragraph verses {counts['para']} "
        f"({100 * counts['para'] / counts['verses']:.0f}%), "
        f"divine-name tokens {counts['divine']}, split verses merged {counts['merged']}"
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1], sys.argv[2]))
