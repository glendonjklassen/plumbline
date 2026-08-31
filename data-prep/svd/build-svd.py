#!/usr/bin/env python3
"""Build data/svd1865.jsonl from the Smith & Van Dyck Arabic Bible.

    python3 data-prep/svd/build-svd.py arb-vd_usfm.zip

Source: https://ebible.org/Scriptures/arb-vd_usfm.zip — eBible.org's USFM
edition of the 1865 Smith & Van Dyck (SVD/AVD), `arb-vd`, marked public domain
in its own `copr.htm` and in eBible's catalogue. The 1865 text has no copyright
page to inherit.

Its Old Testament is the Masoretic Hebrew and its New Testament the Textus
Receptus — the same two texts the KJV stands on. `check-svd.py` proves the TR
readings are actually present rather than taking the label on trust.

What the source gives: KJV verse addresses (31,102 of its 31,104 verses; the two
extras are splits — see MERGES), full vocalization, real paragraphs, and `\\d`
Psalm superscriptions. What it does not: Strong's tags (Arabic's registry row
carries `lexicon: None` — the available alignments are LLM-generated and are
deliberately not shipped) and `\\add` markup, so FLAG_ADDED is never set.

The output is `kjv.jsonl`'s frozen shape (CLAUDE.md §Data formats). Of the flag
bits, DIVINE NAME (2), TITLE (4) and PARAGRAPH (8) are set.
"""

import json
import re
import sys
import unicodedata
import zipfile
from pathlib import Path

TOKENIZATION = "svd1865-tok1"
FORMAT = "overlay-kjv-canonical"
SOURCE = "Smith & Van Dyck 1865 (eBible.org arb-vd USFM; public domain)"

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "data" / "svd1865.jsonl"

FLAG_DIVINE = 2
FLAG_TITLE = 4
FLAG_PARA = 8

# The two verse splits, merged back to KJV addressing. No text is added or
# lost; the choice is only which numbering the engine addresses by, and refKeys
# are frozen KJV addresses, so it must be the KJV's.
#
#   1 Tim 6:22  = the KJV's 6:21b
#   3 John 1:15 = the KJV's 14b
#
# `crates/core/src/versification/` carries a two-row table so the app can show
# "Van Dyck 6:22" beside the KJV address, as it does for Luther's numbering.
MERGES = {("1Tim", 6, 22): ("1Tim", 6, 21), ("3John", 1, 15): ("3John", 1, 14)}

# The Tetragrammaton, spelled out — the bare word only. The ordinary word for
# "the Lord" cannot be flagged: it renders Adonai too, and nothing in the text
# distinguishes them.
#
# Matched against the word with its tashkeel stripped: the vowelling varies
# with the word's position in the sentence, the consonantal skeleton does not.
DIVINE = {"يهوه"}

# Punctuation that belongs beside a word rather than being a word of its own.
# The Arabic forms are distinct codepoints from their ASCII lookalikes and the
# text uses them (U+060C comma, U+061B semicolon, U+061F question mark). The
# hyphen belongs here too: it is a parenthetical dash pinned to a word edge, so
# leaving it attached would index the word under a leading dash, unfindable.
POST = ".,;:!?)]»\u201d\u2019\u060C\u061B\u061F\u2026-"
PRE = "([«\u201c\u201e\u2018-"

# Everything this build reads. Any other marker is skipped, `\s1` deliberately:
# its section headings are a modern publisher's apparatus, not the 1865 text.
ID = re.compile(r"\\id\s+(\S+)")
CHAPTER = re.compile(r"\\c\s+(\d+)")
VERSE = re.compile(r"\\v\s+(\d+)\s*(.*)", re.S)
PARA = re.compile(r"\\(p|m|pi\d?|nb|q\d?|qa|qc|b)\b")
TITLE = re.compile(r"\\d\s+(.*)", re.S)
MARKER = re.compile(r"\\\S+\s*")
# NOT VERSE TEXT. The list has to be stated rather than "ignore unknown
# markers", because the fallback branch keeps whatever text a line carries
# (that is how a continuation on a `\p` line survives) — so an unnamed `\s1`
# heading would be appended to the verse above it.
SKIP = re.compile(r"\\(id|ide|h|toc\d?|mt\d?|ms\d?|s\d?|sr|r|sp|cl|cp|rem|ip)\b")


def strip_marks(word: str) -> str:
    """A word without its tashkeel — the consonantal skeleton.

    Used for the divine-name test only. The category test is `Mn` rather than a
    hardcoded U+064B-U+0652 range so that the superscript alef (U+0670) and the
    Quranic-style marks the text uses in a handful of places fall out too.
    """
    return "".join(c for c in word if unicodedata.category(c) != "Mn")


def osis_order(kjv: Path) -> list[str]:
    """The 66 OSIS ids in canon order, taken from the KJV corpus itself.

    Read rather than restated, for `build-rv1909.py`'s reason: this script's
    whole claim is that its output sits at the same addresses as `kjv.jsonl`,
    and a hand-typed list here would be a second source for that fact.
    """
    order: list[str] = []
    with kjv.open() as f:
        next(f)
        for line in f:
            b = json.loads(line)["b"]
            if b not in order:
                order.append(b)
    return order


def tokenize(text: str, lead_flags: int, title: bool) -> list:
    """Verse text → `[pre, word, post, [strongs], flags]` tokens.

    Whitespace-split, then punctuation peeled off each end — `build-luther.py`'s
    algorithm. Arabic needs no more than that: its script is written with spaces
    between words exactly as English is, and the cursive joining that makes it
    look continuous stops dead at every space.

    The code list is ALWAYS empty; see the header on Strong's.
    """
    tokens: list = []
    first = True
    for raw in text.split():
        pre = ""
        post = ""
        word = raw
        while word and word[0] in PRE:
            pre += word[0]
            word = word[1:]
        while word and word[-1] in POST:
            post = word[-1] + post
            word = word[:-1]
        # A leading combining mark is a source typo (there is one, in Num 2:1):
        # malformed Unicode that renders as a dotted circle and makes the word
        # unfindable, because nobody types the stray mark.
        #
        # Peeled into `pre` rather than deleted: `pre` already holds what sits
        # before the word without being part of it, so the verse still rebuilds
        # character for character and the app is never in the business of
        # silently editing the text of scripture.
        while len(word) > 1 and unicodedata.category(word[0]) == "Mn":
            pre += word[0]
            word = word[1:]
        if not word:
            # Punctuation alone. Hang it on the previous token's `post` so
            # nothing is lost and no empty word is emitted.
            if tokens:
                tokens[-1][2] += raw
            continue
        flags = 0
        if title:
            flags |= FLAG_TITLE
        if first:
            flags |= lead_flags
            first = False
        if strip_marks(word) in DIVINE:
            flags |= FLAG_DIVINE
        tokens.append([pre, word, post, [], flags])
    return tokens


def parse_book(body: str) -> dict[tuple[int, int], tuple[str, bool, str]]:
    """One book's USFM → `(chapter, verse) → (text, starts_paragraph, title)`.

    A verse's text runs from its `\\v` to the next marker that ends it, which is
    not always another `\\v` — `\\p` and the poetry markers interrupt a verse
    mid-sentence and the rest of the verse follows them. So the text is
    accumulated across those, and only a `\\v` or a `\\c` closes a verse.
    """
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
            out[(chapter, verse)] = ("".join(buf).strip(), para, title)

    for line in body.splitlines():
        line = line.rstrip()
        if not line or SKIP.match(line):
            continue
        if m := CHAPTER.match(line):
            flush()
            buf, verse, para, title = [], 0, False, ""
            chapter = int(m.group(1))
            pending_para = False
            pending_title = ""
            continue
        if m := TITLE.match(line):
            # `\d` sits between `\c` and `\v 1`; it belongs to the verse that
            # follows, which is how `kjv.jsonl` carries superscriptions —
            # prepended into verse 1 and flagged, not held as a verse of
            # their own.
            # APPENDED, because a superscription can run to more than one `\d`:
            # Psalms 56, 57, 59 and 60 set the musical direction on one line and
            # the occasion ("when the Philistines took him in Gath") on the
            # next. Assigning here instead of appending keeps only the last line
            # and drops the first half of four superscriptions.
            pending_title = (pending_title + " " + MARKER.sub("", m.group(1))).strip()
            continue
        if m := VERSE.match(line):
            flush()
            verse = int(m.group(1))
            buf = [m.group(2)]
            para = pending_para
            title = pending_title
            pending_para = False
            pending_title = ""
            continue
        if m := PARA.match(line):
            # Before a verse it marks that verse as opening a paragraph; inside
            # one it is a mid-verse break this token model has no place for, and
            # the remaining text simply continues.
            #
            # AND THE REST OF THE LINE IS VERSE TEXT. 50 of these carry the
            # continuation on the same line — "\p وَكَانَ بَنُو يَعْقُوبَ" is
            # the second half of Gen 35:22, not decoration. Treating the marker
            # as owning its whole line drops a clause from fifty verses and
            # every other check here still passes; `check-svd.py` compares
            # against the source precisely so that it cannot.
            rest = line[m.end() :].strip()
            if verse:
                buf.append(" " + rest if rest else " ")
            elif rest:
                pending_title = (pending_title + " " + rest).strip()
            pending_para = True
            continue
        buf.append(" " + MARKER.sub("", line))
    flush()
    return out


def main(src: str) -> int:
    order = osis_order(ROOT / "data" / "kjv.jsonl")
    if len(order) != 66:
        print(f"expected 66 books in kjv.jsonl, found {len(order)}", file=sys.stderr)
        return 1

    with zipfile.ZipFile(src) as z:
        names = sorted(n for n in z.namelist() if n.lower().endswith((".usfm", ".sfm")))
        if len(names) != 66:
            print(f"expected 66 USFM files in {src}, found {len(names)}", file=sys.stderr)
            return 1
        books = []
        for n in names:
            raw = unicodedata.normalize("NFC", z.read(n).decode("utf-8"))
            m = ID.search(raw)
            books.append((n, m.group(1) if m else "?", raw))

    out_lines = []
    counts = {"title": 0, "para": 0, "divine": 0, "merged": 0}
    for i, (name, usx, raw) in enumerate(books):
        osis = order[i]
        verses = parse_book(raw)
        merged_into: dict[tuple[int, int], list] = {}
        rows: list[tuple[int, int, list]] = []
        for (c, v), (text, para, title) in sorted(verses.items()):
            title_tokens = tokenize(title, 0, True) if title else []
            # A titled verse never also opens a paragraph: no token in
            # `kjv.jsonl` carries both bits, and the superscription is the
            # block opening in any case.
            lead = 0 if title_tokens else (FLAG_PARA if para else 0)
            body_tokens = tokenize(text, lead, False)
            tokens = title_tokens + body_tokens
            target = MERGES.get((osis, c, v))
            if target:
                merged_into.setdefault((target[1], target[2]), []).extend(tokens)
                counts["merged"] += 1
                continue
            rows.append((c, v, tokens))
        for c, v, tokens in rows:
            tokens = tokens + merged_into.get((c, v), [])
            counts["title"] += sum(1 for t in tokens if t[4] & FLAG_TITLE)
            counts["para"] += 1 if tokens and tokens[0][4] & FLAG_PARA else 0
            counts["divine"] += sum(1 for t in tokens if t[4] & FLAG_DIVINE)
            out_lines.append(
                json.dumps(
                    {"b": osis, "c": c, "v": v, "t": tokens},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
            )
        _ = name, usx

    header = json.dumps(
        {"format": FORMAT, "source": SOURCE, "tokenization": TOKENIZATION, "verses": len(out_lines)},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    OUT.write_text(header + "\n" + "\n".join(out_lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}: {len(out_lines)} verses")
    print(
        f"  title tokens {counts['title']}, paragraph verses {counts['para']}, "
        f"divine-name tokens {counts['divine']}, split verses merged {counts['merged']}"
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
