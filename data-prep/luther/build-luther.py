#!/usr/bin/env python3
"""Build data/luther1912.jsonl from the Unbound Bible's Luther 1912 export.

    python3 data-prep/luther/build-luther.py luther_1912.json

Source: https://github.com/kesaranb/luther1912 (a mirror of The Unbound Bible's
`luther_1912` module, Biola University). The module's own metadata says
`"copyright": 0, "copyright_statement": "This Bible is in the Public Domain."`
and `"strongs": 0` — there is no Strong's tagging in it, which the language
project's scope already accepts (docs/I18N.md).

WHY THIS SOURCE, and it is the whole reason a German Bible is tractable at all:
it has ALREADY BEEN MAPPED TO KJV VERSIFICATION. All 66 books, every chapter
count and every last-verse number match `data/kjv.jsonl` exactly — 31,102
verses. German tradition numbers about 350 verses differently (Joel and Malachi
split chapters, Exodus and Numbers shift within them), and rather than renumber,
the Unbound editors moved the text to the KJV address and LEFT THE GERMAN NUMBER
IN THE VERSE as a `3:19 ` prefix. So `refKey` means the same verse in both
corpora, and nothing the reader has written needs migrating.

Those prefixes have to come out — they are an editorial artifact, not scripture —
and `check-luther.py` proves they are gone. They are also the exact data a
"show German verse numbers" feature would want, so this script writes them to
`data-prep/luther/german-numbering.tsv` on the way past instead of discarding
them.

The output is `kjv.jsonl`'s frozen shape (CLAUDE.md §Data formats): a header
line, then one verse per line with positional tokens
`[pre, word, post, [strongs], flags]`. Strong's is always empty here. Of the
flag bits only DIVINE NAME (2) is set — see `divine_name` below.
"""

import json
import re
import sys
import unicodedata
from pathlib import Path

TOKENIZATION = "luther1912-tok1"
FORMAT = "overlay-kjv-canonical"
SOURCE = "Luther 1912 (The Unbound Bible, Biola University; public domain)"

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "data" / "luther1912.jsonl"
NUMBERING = Path(__file__).resolve().parent / "german-numbering.tsv"

# Bit 2 of the token flags. Luther sets the Tetragrammaton in caps — HERR,
# HERRN, HERRE — exactly as the KJV sets LORD, and `data/kjv.jsonl` flags those,
# so the German corpus can carry the same mark and the reader's divine-name
# styling works in both without a shell knowing which text it is painting.
#
# Matched on the CAPS FORM ONLY. "Herr" with a lowercase tail is the ordinary
# word for lord or sir and appears constantly ("mein Herr"); treating it as the
# divine name would mark half the dialogue in Genesis.
FLAG_DIVINE = 2
DIVINE = {"HERR", "HERRN", "HERRE", "HERRLICH"} - {"HERRLICH"}

# The German verse number the Unbound editors left in the text. Nearly always
# leading; twice it sits mid-verse, where a KJV verse spans two German ones.
GERMAN_NUM = re.compile(r"(?<![\w:])(\d+):(\d+)\s*")
# Empty braces left by the export (5 of them, no content between).
EMPTY_BRACES = re.compile(r"\{\s*\}")
# A leading dash the export uses on three verses.
LEAD_DASH = re.compile(r"^-\s*")

# Punctuation that belongs AFTER a word rather than being a word of its own.
POST = ".,;:!?)]»“”\"'…"
PRE = "([«„\"'"


def osis_order(kjv: Path) -> list[str]:
    """The 66 OSIS ids in canon order, taken from the KJV corpus itself.

    Read rather than restated: this script's whole claim is that its output sits
    at the same addresses as `kjv.jsonl`, and a hand-typed list here would be a
    second source for that fact.
    """
    order: list[str] = []
    with kjv.open() as f:
        next(f)
        for line in f:
            b = json.loads(line)["b"]
            if not order or order[-1] != b:
                if b not in order:
                    order.append(b)
    return order


def divine_name(word: str) -> bool:
    """Whether this word is Luther's rendering of the Tetragrammaton."""
    bare = word.strip("".join(set(POST) | set(PRE)))
    return bare in DIVINE


def tokenize(text: str) -> list:
    """Split a verse into `[pre, word, post, [], flags]` tokens.

    Whitespace-split, then punctuation peeled off each end. That is the whole
    algorithm, and it is enough because the invariant the format actually needs
    is that `pre + word + post` concatenated back in order reproduces the verse —
    which `check-luther.py` asserts for all 31,102 of them rather than trusting
    it here.

    German hyphenated compounds and apostrophes stay INSIDE the word
    ("Gefängnis", "Juda's", "Bath-Seba"): a reader tapping one wants the whole
    word, and splitting on the hyphen would make two words out of one name.
    """
    tokens = []
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
        if not word:
            # Punctuation alone (a stray dash). Hang it on the previous token's
            # `post` so nothing is lost and no empty word is emitted.
            if tokens:
                tokens[-1][2] += raw
            continue
        flags = FLAG_DIVINE if divine_name(word) else 0
        tokens.append([pre, word, post, [], flags])
    return tokens


def main(src: str) -> int:
    rows = json.loads(Path(src).read_text(encoding="utf-8"))["verses"]
    order = osis_order(ROOT / "data" / "kjv.jsonl")
    if len(order) != 66:
        print(f"expected 66 books in kjv.jsonl, found {len(order)}", file=sys.stderr)
        return 1

    numbering = []
    out_lines = []
    for r in rows:
        osis = order[r["book"] - 1]
        text = unicodedata.normalize("NFC", r["text"])
        text = EMPTY_BRACES.sub("", text)
        text = LEAD_DASH.sub("", text)
        for m in GERMAN_NUM.finditer(text):
            numbering.append(f'{osis}\t{r["chapter"]}\t{r["verse"]}\t{m.group(1)}:{m.group(2)}')
        text = GERMAN_NUM.sub("", text)
        text = re.sub(r"\s+", " ", text).strip()
        tokens = tokenize(text)
        out_lines.append(
            json.dumps({"b": osis, "c": r["chapter"], "v": r["verse"], "t": tokens}, ensure_ascii=False, separators=(",", ":"))
        )

    header = json.dumps(
        {"format": FORMAT, "source": SOURCE, "tokenization": TOKENIZATION, "verses": len(out_lines)},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    OUT.write_text(header + "\n" + "\n".join(out_lines) + "\n", encoding="utf-8")
    NUMBERING.write_text(
        "# The German verse number the Unbound export left inline, by the KJV\n"
        "# address it was found at. Kept for a future 'show German numbering'\n"
        "# feature; nothing reads it today.\n"
        "# osis\tchapter\tverse\tgermanRef\n" + "\n".join(numbering) + "\n",
        encoding="utf-8",
    )
    print(f"wrote {OUT.relative_to(ROOT)}: {len(out_lines)} verses")
    print(f"wrote {NUMBERING.relative_to(ROOT)}: {len(numbering)} inline German numbers")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
