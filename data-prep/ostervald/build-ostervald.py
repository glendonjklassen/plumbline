#!/usr/bin/env python3
"""Build data/ost1996.jsonl from the Ostervald OSIS edition — and remap it.

    python3 data-prep/ostervald/build-ostervald.py fra-ostervald.osis.xml

Source: https://github.com/seven1m/open-bibles (`fra-ostervald.osis.xml`), the
J.F. Ostervald translation in its 1996 revision. Public domain per the
open-bibles catalogue; the OSIS header names the edition (`ostv1996`), and the
About screen must say "1996" too — the 1744 original is not what these bytes
are.

WHY THIS TEXT: Ostervald is the French TR line — the Geneva/Olivétan tradition.
The gate verse Acts 8:37 is present ("Et Philippe lui dit: Si tu crois de tout
ton cour, cela t'est permis." — `cour` is the source's own typo for `cœur`, and
this build ships the source's letters), and all twenty TR discriminators carry
text. Louis Segond was rejected: eclectic and critical-leaning.

UNLIKE Reina-Valera, THE SOURCE DOES NOT SIT AT KJV ADDRESSES. It prints the
French/Hebrew-style numbering: 31,172 verses against the KJV's 31,102, with 91
chapters breaking differently. This build moves the text onto KJV addresses —
the same thing the Unbound editors had already done for Luther — and writes
what the printed French Bible calls each moved verse into
`crates/core/src/versification/ostervald-numbering.tsv` for the
`NumberingSpec` annotation. The whole disagreement is three primitives:

  - TITLES: 62 psalms number their superscription as verse 1 (five of them as
    verses 1–2). The title verse(s) fold into KJV verse 1 as FLAG_TITLE tokens
    — exactly where the KJV itself carries them — and the body shifts down.
  - MERGES: six places where the source prints two verses for one KJV verse
    (1Sam 20:42–43, 1Kgs 22:43–44, Mark 9:50–51, Mark 10:52–53,
    3John 14–15, and Rev 12:18 prepended into 13:1 — the same two splits
    `build-indic.py` merges, and Rev 12:18 reads "Et je me tins debout", the
    TR's "I stood"). The pair joins with a space at the KJV address.
  - SPLITS: three places where the source prints one verse for two KJV verses
    (Luke 10:41, Acts 19:40, 2Cor 13:12). Each cut is a hardcoded sentence
    boundary asserted against the source text, so a changed upstream file
    fails loudly instead of splitting in the wrong place.

Every other difference is a chapter break falling elsewhere (Num 12/13, Num
29/30, 1Sam 23/24, Job 38–41, Eccl 11/12, Isa 8/9, Ezek 20/21, Hos 11/12,
Jonah 1/2), and those need no directive at all: both texts run in canon order,
so walking the KJV's addresses while consuming the source's verses in order
lands every verse — the ledger (31,102 = 31,172 − 67 titles − 6 merges
+ 3 splits) is asserted at the end, alongside every-source-verse-consumed.

The output is `kjv.jsonl`'s frozen shape (CLAUDE.md §Data formats): a header
line, then one verse per line with positional tokens
`[pre, word, post, [strongs], flags]`. No Strong's codes — none exist for this
text, and machine-guessing them was refused when Arabic set the precedent.

Of the flag bits, DIVINE (2) and TITLE (4) are set. Not ADDED (1): the OSIS
file carries no <transChange> markup (its 31,172 verse elements contain no
inner markup at all). Not PARAGRAPH (8): no paragraph markup either.

FRENCH TOKENIZATION, two things beyond the rv1909 recipe:

  - ELISION goes into `pre`: "l'homme" becomes pre "l'", word "homme", so the
    search index holds the word a reader would type. Only a closed list of
    elision prefixes is peeled (l' d' j' n' s' t' c' m' qu' jusqu' lorsqu'
    puisqu' quoiqu'); "aujourd'hui" and "quelqu'un" stay whole because their
    heads are not words on their own. This means `pre` carries LETTERS here,
    which `check-rv1909.py`-style checkers forbid — `check-ostervald.py`
    allows exactly the whitelist.
  - The apostrophe is never peeled as trailing punctuation, and the hyphen is
    never peeled at all ("rendra-t-il", "Bath-Shéba" are single tokens).
"""

import json
import re
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

TOKENIZATION = "ost1996-tok1"
FORMAT = "overlay-kjv-canonical"
SOURCE = "La Bible Ostervald, révision 1996 (via seven1m/open-bibles; domaine public)"

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "data" / "ost1996.jsonl"
NUMBERING = ROOT / "crates" / "core" / "src" / "versification" / "ostervald-numbering.tsv"

FLAG_DIVINE = 2
FLAG_TITLE = 4

# Ostervald renders the Tetragrammaton "l'Éternel" where the KJV sets LORD —
# 6,921 occurrences, plus the one place the name itself is the point (the
# "Jéhovah" of Exod 6:3's tradition). The bare name only, after elision peel.
DIVINE = {"Éternel", "ÉTERNEL", "Jéhovah"}

# Whitespace-split, then punctuation peeled off each end. The straight double
# quote is this text's only quotation mark and it faces both ways, so it sits
# in both sets and position decides. No apostrophe in either set (elision is
# handled separately) and no hyphen (French compounds and inversion).
POST = '.,;:!?)]"…'
PRE = '(["'

# The closed elision list — peeled into `pre` with the apostrophe, longest
# first so "jusqu'" wins over "qu'".
ELISION = ("jusqu'", "lorsqu'", "puisqu'", "quoiqu'", "qu'", "l'", "d'", "j'", "n'", "s'", "t'", "c'", "m'")

VERSE = re.compile(r"<verse osisID='([A-Za-z0-9]+)\.(\d+)\.(\d+)'\s*>(.*?)</verse>", re.S)

# One source verse that holds two KJV verses: source address → the exact text
# the FIRST KJV verse ends with. Asserted present, so an upstream edit fails
# the build rather than moving the cut.
SPLITS = {
    ("Luke", 10, 41): "tu te mets en peine et tu t'agites pour beaucoup de choses;",
    ("Acts", 19, 40): "n'ayant aucune raison pour justifier ce rassemblement.",
    ("2Cor", 13, 12): "Saluez vous les uns les autres par un saint baiser.",
}

# Two source verses that hold one KJV verse: the KJV address → how many source
# verses it consumes. Rev 13:1 consumes 12:18 ("Et je me tins debout sur le
# sable de la mer") and 13:1 — the same prepend `build-indic.py` documents.
MERGES = {
    ("1Sam", 20, 42): 2,
    ("1Kgs", 22, 43): 2,
    ("Mark", 9, 50): 2,
    ("Mark", 10, 52): 2,
    ("3John", 1, 14): 2,
    ("Rev", 13, 1): 2,
}

# The five psalms whose superscription spans two source verses; every other
# title psalm is derived from the counts (source max = KJV max + 1, Psalms
# only) and asserted against this expectation in numbers.
TWO_VERSE_TITLES = {30, 51, 52, 54, 60}


def kjv_shape(kjv: Path) -> tuple[list[str], dict]:
    """Canon order and per-chapter last verse, from the KJV corpus itself.

    Read rather than restated, for `build-luther.py`'s reason: this script's
    whole claim is that its output sits at `kjv.jsonl`'s addresses, and a
    hand-typed shape would be a second source for that fact.
    """
    order: list[str] = []
    shape: dict = defaultdict(dict)
    with kjv.open() as f:
        next(f)
        for line in f:
            r = json.loads(line)
            if r["b"] not in order:
                order.append(r["b"])
            shape[r["b"]][r["c"]] = max(r["v"], shape[r["b"]].get(r["c"], 0))
    return order, shape


def read_source(src: str) -> dict:
    """(book, chapter, verse) → verse text, NFC-normalized (a no-op here —
    asserted, since a normalizing read that changed letters would break the
    letters-are-the-source's check downstream)."""
    raw = Path(src).read_text(encoding="utf-8")
    if unicodedata.normalize("NFC", raw) != raw:
        raise SystemExit("source is not NFC-normal; investigate before building")
    verses: dict = {}
    for m in VERSE.finditer(raw):
        key = (m.group(1), int(m.group(2)), int(m.group(3)))
        if key in verses:
            raise SystemExit(f"duplicate source verse {key}")
        verses[key] = " ".join(m.group(4).split())
    return verses


def tokenize(text: str, flags: int = 0) -> list:
    """Whitespace-split, punctuation peeled, elision into `pre`."""
    tokens: list = []
    pending = ""  # a letterless chunk (a dialogue dash) rides the next word's pre
    for raw in text.split():
        if not any(ch.isalpha() for ch in raw):
            pending += raw
            continue
        pre = pending
        pending = ""
        post = ""
        word = raw
        while word and word[0] in PRE:
            pre += word[0]
            word = word[1:]
        for el in ELISION:
            if len(word) > len(el) and word[: len(el)].lower() == el:
                pre += word[: len(el)]
                word = word[len(el) :]
                break
        while word and word[-1] in POST:
            post = word[-1] + post
            word = word[:-1]
        f = flags
        if word in DIVINE:
            f |= FLAG_DIVINE
        tokens.append([pre, word, post, [], f])
    if pending and tokens:
        # A letterless chunk at the very end hangs on the last token instead.
        tokens[-1][2] += pending
    return tokens


def main(src: str) -> int:
    order, shape = kjv_shape(ROOT / "data" / "kjv.jsonl")
    if len(order) != 66:
        print(f"expected 66 books in kjv.jsonl, found {len(order)}", file=sys.stderr)
        return 1
    verses = read_source(src)
    src_books = {b for b, _, _ in verses}
    if src_books != set(order):
        print(f"source books differ from the canon: {sorted(src_books ^ set(order))}", file=sys.stderr)
        return 1

    src_shape: dict = defaultdict(dict)
    for b, c, v in verses:
        src_shape[b][c] = max(v, src_shape[b].get(c, 0))

    out_lines = []
    numbering: list[tuple[str, int, int, str]] = []
    consumed = 0
    titles_folded = 0

    for book in order:
        # The source's verses for this book, in its own printed order — canon
        # order in both texts, which is what lets the walk below be sequential.
        queue = [(c, v) for c in sorted(src_shape[book]) for v in range(1, src_shape[book][c] + 1)]
        qi = 0
        carry: str | None = None  # tail of a split verse, waiting for the next KJV address

        def take() -> tuple[tuple[int, int], str]:
            nonlocal qi
            c, v = queue[qi]
            qi += 1
            return (c, v), verses[(book, c, v)]

        for chapter in sorted(shape[book]):
            for verse in range(1, shape[book][chapter] + 1):
                tokens: list = []
                printed: tuple[int, int]
                if carry is not None:
                    # The rest of a split source verse IS this KJV verse.
                    tokens = tokenize(carry)
                    printed = queue[qi - 1]
                    carry = None
                elif book == "Ps" and verse == 1 and src_shape[book][chapter] > shape[book][chapter]:
                    surplus = src_shape[book][chapter] - shape[book][chapter]
                    expected = 2 if chapter in TWO_VERSE_TITLES else 1
                    if surplus != expected:
                        raise SystemExit(f"Ps {chapter}: surplus {surplus}, expected {expected}")
                    for _ in range(surplus):
                        _, title_text = take()
                        tokens.extend(tokenize(title_text, FLAG_TITLE))
                        titles_folded += 1
                    printed, body = take()
                    tokens.extend(tokenize(body))
                elif (book, chapter, verse) in MERGES:
                    parts = []
                    for k in range(MERGES[(book, chapter, verse)]):
                        addr, text = take()
                        if k == 0:
                            printed = addr
                        parts.append(text)
                    tokens = tokenize(" ".join(parts))
                else:
                    printed, text = take()
                    if (book, *printed) in SPLITS:
                        head = SPLITS[(book, *printed)]
                        at = text.find(head)
                        if at < 0 or not text[at + len(head) :].strip():
                            raise SystemExit(f"split point not found in {book} {printed}: {head!r}")
                        carry = text[at + len(head) :].strip()
                        text = text[: at + len(head)]
                    tokens = tokenize(text)

                if printed != (chapter, verse):
                    numbering.append((book, chapter, verse, f"{printed[0]}:{printed[1]}"))
                out_lines.append(
                    json.dumps(
                        {"b": book, "c": chapter, "v": verse, "t": tokens},
                        ensure_ascii=False,
                        separators=(",", ":"),
                    )
                )
        if qi != len(queue) or carry is not None:
            print(f"{book}: consumed {qi} of {len(queue)} source verses (carry={carry is not None})", file=sys.stderr)
            return 1
        consumed += qi

    if len(out_lines) != 31102:
        print(f"expected 31,102 verses out, wrote {len(out_lines)}", file=sys.stderr)
        return 1
    if consumed != len(verses):
        print(f"consumed {consumed} of {len(verses)} source verses", file=sys.stderr)
        return 1
    if titles_folded != 67:
        print(f"expected 67 psalm title verses folded, folded {titles_folded}", file=sys.stderr)
        return 1

    header = json.dumps(
        {"format": FORMAT, "source": SOURCE, "tokenization": TOKENIZATION, "verses": len(out_lines)},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    OUT.write_text(header + "\n" + "\n".join(out_lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}: {len(out_lines)} verses")

    tsv = [
        "# The French verse number a printed Ostervald shows, by the KJV",
        "# address the text sits at here. Generated by",
        "# data-prep/ostervald/build-ostervald.py; read by NumberingSpec (Fr).",
        "# osis\tchapter\tverse\tprintedRef",
    ]
    tsv += [f"{b}\t{c}\t{v}\t{p}" for b, c, v, p in numbering]
    NUMBERING.write_text("\n".join(tsv) + "\n", encoding="utf-8")
    print(f"wrote {NUMBERING.relative_to(ROOT)}: {len(numbering)} disagreeing addresses")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
