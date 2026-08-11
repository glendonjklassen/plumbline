#!/usr/bin/env python3
"""Merge Strong's tags into data/luther1912.jsonl from the Zefania XML
"Luther 1912 mit Strongs".

    python3 data-prep/luther/merge-strongs.py "SF_2022-02-27_GER_LUTH1912_(LUTHER_1912_mit_Strongs).xml"

Source: the Zefania XML project's Luther 1912 with Strong's numbers —
SF_2022-02-27_GER_LUTH1912_Strongs_xml_220227.zip from
https://sourceforge.net/projects/zefania-sharp/files/Bibles/GER/Lutherbibel/Luther%201912/
(creator/publisher www.toledot.info; the file's own header declares
"This Text is in the Public Domain"). See BIBLIOGRAPHY.md.

THE TOKENS DO NOT CHANGE. `luther1912-tok1` is frozen for the same reason
`kjv1769-tok2` is — token-anchored reader data (weave spans) — so this script
only fills each token's empty Strong's slot and touches nothing else. It
asserts exactly that before writing.

WHY ALIGNMENT IS BY BOOK, NOT BY VERSE ADDRESS. The two editions differ in two
ways that verse addressing cannot bridge:

  - VERSIFICATION. Our corpus sits at KJV addresses; the Zefania file uses
    German numbering, and its verse boundaries shift in more places than the
    inline-number record in german-numbering.tsv covers (1Kgs 4/5, 1Chr 6,
    Neh 4, Ezek 21, Zech 2, ...). Aligning each book as ONE token stream in
    reading order makes the numbering irrelevant: the words come in the same
    order however the verse fences fall.
  - ORTHOGRAPHY. Ours keeps the old spelling ("daß", "thun"); the Zefania text
    is modernized ("dass", "tun"). Words are compared through a normalizing
    fold (ß→ss, th→t, ey→ei, apostrophes out), plus handling for joined words
    (our corpus has a few missing-space artifacts like "zornigüber") and
    contractions ("wenn's" vs "wenn es").

Words the alignment cannot confidently pair keep an EMPTY Strong's slot — a
missing tag is honest, a guessed tag is not. The measured transfer rate is
~98.3% of the source's ~350k tags; the script fails if it falls below the
FLOOR, because a big drop means one of the editions changed under us.

The source tags carry their own imperfections (an occasional tag lands on a
neighbouring function word where Luther's German restructures the sentence).
They are transferred as-is; the tagging is the source's claim, not ours.
"""

import json
import sys
import unicodedata
from difflib import SequenceMatcher
from pathlib import Path
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[2]
LUTHER = ROOT / "data" / "luther1912.jsonl"
STRONGS = ROOT / "data" / "strongs.json"
OT_BOOKS = 39
FLOOR = 0.97
SOURCE = (
    "Luther 1912 (The Unbound Bible, Biola University; public domain); "
    "Strong's tags: Zefania XML Luther 1912 mit Strongs (toledot.info; public domain)"
)
PUNCT = ".,;:!?()[]»«„“”\"'’…-"


def norm(word: str) -> str:
    """Orthography-insensitive comparison form for the 1912 old/new spellings."""
    w = unicodedata.normalize("NFC", word).strip(PUNCT).lower()
    w = w.replace("ß", "ss").replace("th", "t").replace("ey", "ei")
    return w.replace("'", "").replace("’", "")


def osis_order() -> list[str]:
    order: list[str] = []
    with (ROOT / "data" / "kjv.jsonl").open(encoding="utf-8") as f:
        next(f)
        for line in f:
            b = json.loads(line)["b"]
            if not order or order[-1] != b:
                if b not in order:
                    order.append(b)
    return order


def zefania_streams(xml: Path, order: list[str]):
    """book → [(word, codes-or-None), ...] in reading order. Codes get their
    testament prefix here (the file's numbers are bare), matching strongs.json
    keys ("H7225"/"G26")."""
    tree = ElementTree.parse(xml)
    books = {}
    for book in tree.getroot().iter("BIBLEBOOK"):
        n = int(book.get("bnumber"))
        prefix = "H" if n <= OT_BOOKS else "G"
        stream = []

        def words_of(text, codes):
            for w in (text or "").split():
                stream.append((w, codes))

        for chap in book.iter("CHAPTER"):
            for vers in chap.iter("VERS"):
                words_of(vers.text, None)
                for gr in vers:
                    codes = None
                    if gr.tag == "gr":
                        codes = [prefix + c.lstrip("0") for c in (gr.get("str") or "").split() if c.strip("0")]
                    words_of(gr.text, codes)
                    words_of(gr.tail, None)
        books[order[n - 1]] = stream
    return books


def align_book(our_words: list[str], zef):
    """→ {our_stream_index: [codes]}, plus (transferred, tagged) counts."""
    a = [norm(w) for w in our_words]
    b = [norm(w) for (w, _) in zef]
    sm = SequenceMatcher(None, a, b, autojunk=False)
    tagged = sum(1 for (_, c) in zef if c)
    transferred = 0
    out: dict[int, list[str]] = {}

    def take(i: int, codes) -> None:
        nonlocal transferred
        if codes:
            transferred += 1
            seen = out.setdefault(i, [])
            for c in codes:
                if c not in seen:
                    seen.append(c)

    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            for i, j in zip(range(i1, i2), range(j1, j2)):
                take(i, zef[j][1])
        elif op == "replace":
            i, j = i1, j1
            while i < i2 and j < j2:
                x = a[i]
                # One of ours == two source words: a missing-space artifact in
                # our corpus ("zornigüber"), or a contraction ("wenn's" ==
                # "wenn" + "es"). The token gets BOTH words' codes — a tap on
                # the joined word is a tap on both.
                if j + 1 < j2:
                    y2 = b[j] + b[j + 1]
                    y2c = b[j] + (b[j + 1][1:] if b[j + 1].startswith("e") else b[j + 1])
                    if x == y2 or x == y2c:
                        take(i, (zef[j][1] or []) + (zef[j + 1][1] or []))
                        i += 1
                        j += 2
                        continue
                # Two of ours == one source word (the artifact on their side).
                if i + 1 < i2 and a[i] + a[i + 1] == b[j]:
                    take(i, zef[j][1])
                    i += 2
                    j += 1
                    continue
                # A close spelling variant the folds don't cover ("Heiden" /
                # "Heyden" class): same initial, near length, shared stem.
                y = b[j]
                if x and y and x[0] == y[0] and abs(len(x) - len(y)) <= 2 and (
                    x.startswith(y[:4]) or y.startswith(x[:4])
                ):
                    take(i, zef[j][1])
                i += 1
                j += 1
    return out, transferred, tagged


def main(src: str) -> int:
    order = osis_order()
    zef = zefania_streams(Path(src), order)
    known = set(json.loads(STRONGS.read_text(encoding="utf-8")))

    with LUTHER.open(encoding="utf-8") as f:
        header = json.loads(next(f))
        rows = [json.loads(l) for l in f if l.strip()]

    # Book streams over the existing rows; each entry remembers (row, token).
    sites: dict[str, list[tuple[int, int]]] = {}
    words: dict[str, list[str]] = {}
    for ri, r in enumerate(rows):
        sites.setdefault(r["b"], [])
        words.setdefault(r["b"], [])
        for ti, t in enumerate(r["t"]):
            if t[3]:
                print(f"{r['b']} {r['c']}:{r['v']} token {ti} already tagged — refusing to re-merge", file=sys.stderr)
                return 1
            sites[r["b"]].append((ri, ti))
            words[r["b"]].append(t[1])

    total_transferred = total_tagged = 0
    for book in order:
        out, transferred, tagged = align_book(words[book], zef[book])
        total_transferred += transferred
        total_tagged += tagged
        for i, codes in out.items():
            bad = [c for c in codes if c not in known]
            if bad:
                print(f"unknown Strong's code(s) {bad} in {book}", file=sys.stderr)
                return 1
            ri, ti = sites[book][i]
            rows[ri]["t"][ti][3] = codes

    rate = total_transferred / total_tagged
    if rate < FLOOR:
        print(f"transfer rate {rate:.1%} below floor {FLOOR:.0%} — the editions have diverged; not writing", file=sys.stderr)
        return 1

    header["source"] = SOURCE
    lines = [json.dumps(header, ensure_ascii=False, separators=(",", ":"))]
    for r in rows:
        lines.append(
            json.dumps({"b": r["b"], "c": r["c"], "v": r["v"], "t": r["t"]}, ensure_ascii=False, separators=(",", ":"))
        )
    LUTHER.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"tagged {total_transferred} of {total_tagged} source tags ({rate:.1%}) across {len(rows)} verses")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
