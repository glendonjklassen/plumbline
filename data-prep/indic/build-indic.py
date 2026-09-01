#!/usr/bin/env python3
"""Build the Punjabi and Hindi corpora from Free Bibles India's USFM.

    python3 data-prep/indic/build-indic.py pa Punjabi_Bible-master
    python3 data-prep/indic/build-indic.py hi Hindi_Bible-master

Either a directory or a zip of one. The sources, both CC BY-SA 4.0:

    https://github.com/FreeBiblesIndia/Punjabi_Bible
    https://github.com/FreeBiblesIndia/Hindi_Bible

One script for two languages because it is one publisher's export in one set of
conventions; the other corpora get their own scripts because their sources
differ.

What these sources give: KJV verse addresses (31,102 of their 31,104 verses; the
two extras are splits — see MERGES), a Textus Receptus base, the divine name
spelled out (so FLAG_DIVINE means what it means in the KJV), `\\d` Psalm
superscriptions, and real prose paragraphs. What they do not give: Strong's tags
(both registry rows carry `lexicon: None`; nothing is machine-guessed) and
`\\add` markup (`\\it` here is emphasis, so FLAG_ADDED is never set).

Footnotes (`\\f … \\f*`) and cross-references (`\\bdit … \\bdit*`) sit INSIDE the
verse and read as text, so they are cut whole by their end markers — a parser
that only drops markers keeps a footnote's words as scripture. Every other
character style keeps its text and loses its marker.

THE TEXT IS NOT NORMALISED, deliberately. NFC decomposes Gurmukhi's precomposed
nukta letters (they are on Unicode's composition exclusion list), so
"normalising" would rewrite letters; Devanagari is on the same list. Both files
are already stable under NFC and NFD. `check-indic.py` asserts it, and also
guards against a source assembled by splicing a modern translation into an older
one — the reason tfbf/Bible-Punjabi-Pavitr-Bible-1945 was rejected.

The output is `kjv.jsonl`'s frozen shape (CLAUDE.md §Data formats). Of the flag
bits, DIVINE NAME (2), TITLE (4) and PARAGRAPH (8) are set.
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

# The two verse splits, merged back to KJV addressing (refKeys are frozen KJV
# addresses). The direction differs, and getting it wrong reorders a verse:
#
#   3 John 1:15 -> 1:14, APPENDED.   It is the tail of the KJV's v14.
#   Rev 12:18   -> 13:1, PREPENDED.  "And he stood upon the sand of the sea" is
#                                     the HEAD of the KJV's 13:1.
MERGES = {
    ("3John", 1, 15): ("3John", 1, 14, "append"),
    ("Rev", 12, 18): ("Rev", 13, 1, "prepend"),
}

# Punctuation that belongs beside a word rather than being a word of its own.
# The hyphen is deliberately absent (unlike `build-svd.py`'s list): these
# languages hyphenate reduplications word-internally, and peeling it would break
# the words that legitimately end in one.
POST = ".,;:!?)]»\u201d\u2019\u0964\u0965\u2013\u2014"
PRE = "([«\u201c\u201e\u2018"


# Markers whose text is NOT scripture, cut span and all. See the header.
SPANS = re.compile(r"\\\+?f\s.*?\\\+?f\*|\\\+?bdit\b.*?\\\+?bdit\*", re.S)
# Any remaining marker, including USFM's nested `\+wj` form. The text around it
# is kept.
MARKER = re.compile(r"\\\+?[a-z]+\d*\*?\s?")

ID = re.compile(r"\\id\s+(\S+)")
CHAPTER = re.compile(r"\\c\s+(\d+)")
# Verse markers are not always at the start of a line ("\p \v 1 …"), so split on
# the marker anywhere.
VERSE = re.compile(r"\\v\s+(\d+)\s")
# `\d` (psalm superscription) and `\qa` (Psalm 119's acrostic stanza headings)
# are the same thing to this token model, as they are in `kjv.jsonl`: both fold
# into the verse that follows and both carry FLAG_TITLE. Only the Punjabi source
# marks the acrostic headings; that is not an error.
TITLE = re.compile(r"\\(?:d|qa)\s+(.*)", re.S)
# Prose paragraph openings. `\q` is neither one of these nor skippable: it is a
# poetry line, one per line of verse, so treating it as a paragraph over-marks
# the Psalms and skipping it drops half of nearly every verse in them. It falls
# through to the continuation branch, which keeps the text and marks nothing.
PARA = re.compile(r"\\(p|m|pi\d?|nb|pc|pr)\b")
# NOT VERSE TEXT, and it must be named here because the fallback branch keeps
# whatever text a line carries (that is how a verse continued after a `\p`
# survives). Includes the publisher's book introductions (`\is`, `\ip`,
# `\iot`/`\io1`), which sit between `\id` and the first `\c` where nothing else
# would drop them.
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
            # Punctuation standing alone between two spaces is never a token of
            # its own: an empty `word` is a tap target that leads nowhere and an
            # index entry no search can match. Carried FORWARD, not hung on the
            # token behind — an opening quote belongs to the word it opens, and
            # at the head of a verse there is nothing behind it anyway.
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
            # `\d` sits between `\c` and `\v 1` and belongs to the verse that
            # follows, the way `kjv.jsonl` carries superscriptions. Appended
            # rather than assigned: a superscription can run to several lines.
            pending_title = (pending_title + " " + MARKER.sub(" ", m.group(1))).strip()
        elif SKIP.match(head.strip()):
            pass
        elif PARA.match(head.strip()):
            # Before a verse it marks that verse as opening a paragraph; inside
            # one it is a mid-verse break this token model has no place for, and
            # the text continues. Either way the rest of the line is verse text
            # — dropping it loses a clause from every verse that rides a `\p`.
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
            # A titled verse never also opens a paragraph — no token in
            # `kjv.jsonl` carries both bits.
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
