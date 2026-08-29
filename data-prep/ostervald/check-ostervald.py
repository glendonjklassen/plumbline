#!/usr/bin/env python3
"""Prove data/ost1996.jsonl against its claims — and against its source.

    python3 data-prep/ostervald/check-ostervald.py [fra-ostervald.osis.xml]

Eleven claims, in the `check-indic.py` mould. A textual finding counts only if
a script produced it from a local file; this is that script for French.

Two claims deliberately do NOT port from the Indic checker:

  - "NFD is a no-op" — false for accented Latin by design: é decomposes. The
    claim here is NFC-only (the corpus is composed), which is what the search
    fold expects.
  - "pre/post hold no letters" — French elision lives in `pre` ("l'homme" =
    `l'` + `homme`) so the search index holds the word a reader types. The
    claim becomes: `post` holds no letters, and every letter-bearing `pre` is
    optional leading punctuation plus one whitelisted elision prefix.
"""

import json
import re
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "data" / "ost1996.jsonl"
KJV = ROOT / "data" / "kjv.jsonl"
NUMBERING = ROOT / "crates" / "core" / "src" / "versification" / "ostervald-numbering.tsv"

FLAG_ADDED, FLAG_DIVINE, FLAG_TITLE, FLAG_PARA = 1, 2, 4, 8

DIVINE = {"Éternel", "ÉTERNEL", "Jéhovah"}
ELISION = ("jusqu'", "lorsqu'", "puisqu'", "quoiqu'", "qu'", "l'", "d'", "j'", "n'", "s'", "t'", "c'", "m'")
PRE_PUNCT = set('(["')

# The twenty TR discriminators (`TODO.md` §The rule): present with real text —
# at least three tokens — or the corpus is rejected. Acts 8:37 is the gate.
TR = [
    ("Matt", 17, 21), ("Matt", 18, 11), ("Matt", 23, 14), ("Mark", 7, 16),
    ("Mark", 9, 44), ("Mark", 9, 46), ("Mark", 11, 26), ("Mark", 15, 28),
    ("Mark", 16, 9), ("Mark", 16, 20), ("Luke", 17, 36), ("Luke", 23, 17),
    ("John", 5, 4), ("John", 7, 53), ("John", 8, 11), ("Acts", 8, 37),
    ("Acts", 15, 34), ("Acts", 24, 7), ("Acts", 28, 29), ("Rom", 16, 24),
]

# The alignment's directive sites, each pinned by words that must sit at the
# REMAPPED address — these fail against a build whose cut or merge landed
# elsewhere, which is the bug class the sequential aligner could hide.
LANDMARKS = {
    ("Luke", 10, 42): "mais une seule est nécessaire",
    ("Acts", 19, 41): "il congédia l'assemblée",
    ("2Cor", 13, 13): "Tous les Saints vous saluent",
    ("2Cor", 13, 14): "La grâce du Seigneur Jésus-Christ",
    ("1Sam", 20, 42): "et Jonathan rentra dans la ville",
    ("1Kgs", 22, 43): "les hauts lieux ne furent point détruits",
    ("Mark", 9, 50): "soyez en paix entre vous",
    ("Mark", 10, 52): "il suivait Jésus dans le chemin",
    ("3John", 1, 14): "Salue les amis, chacun par son nom",
    ("Jonah", 1, 17): "un grand poisson",
    ("Job", 41, 1): "Léviathan",
    ("Job", 41, 10): "point d'homme si hardi",
    ("Isa", 9, 1): "la terre de Zabulon",
    ("Eccl", 11, 9): "Jeune homme, réjouis-toi",
}
# Rev 13:1 must carry BOTH halves in order: the prepended 12:18 and its own.
REV_13_1 = ("je me tins debout", "je vis monter de la mer une bête")

TWO_VERSE_TITLES = {30, 51, 52, 54, 60}

VERSE_XML = re.compile(r"<verse osisID='([A-Za-z0-9]+)\.(\d+)\.(\d+)'\s*>(.*?)</verse>", re.S)


def die(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def letters(s: str) -> str:
    return "".join(c for c in s if unicodedata.category(c).startswith("L"))


def main(src: str | None) -> int:
    lines = CORPUS.read_text(encoding="utf-8").splitlines()
    header = json.loads(lines[0])

    # 1. Addresses are the KJV's.
    if header["tokenization"] != "ost1996-tok1":
        die(f"tokenization stamp {header['tokenization']}")
    kjv_set = set()
    kjv_books: list[str] = []
    with KJV.open() as f:
        next(f)
        for line in f:
            r = json.loads(line)
            kjv_set.add((r["b"], r["c"], r["v"]))
            if r["b"] not in kjv_books:
                kjv_books.append(r["b"])
    verses: dict = {}
    order: list[tuple] = []
    for line in lines[1:]:
        r = json.loads(line)
        key = (r["b"], r["c"], r["v"])
        if key in verses:
            die(f"duplicate verse {key}")
        verses[key] = r["t"]
        order.append(key)
    if len(verses) != 31102 or header["verses"] != 31102:
        die(f"{len(verses)} verses, header says {header['verses']}")
    if set(verses) != kjv_set:
        die(f"addresses differ from the KJV at e.g. {sorted(set(verses) ^ kjv_set)[:5]}")
    if len({b for b, _, _ in verses}) != 66 or len({(b, c) for b, c, _ in verses}) != 1189:
        die("book or chapter count is not the canon's")

    # 2. The twenty TR discriminators.
    for key in TR:
        if len(verses[key]) < 3:
            die(f"TR discriminator {key} has {len(verses[key])} tokens")

    # 3. Tokens are sound; reassembly is possible.
    for key, toks in verses.items():
        if not toks:
            die(f"empty verse {key}")
        for pre, word, post, codes, flags in toks:
            if not word or not any(unicodedata.category(c).startswith("L") for c in word):
                die(f"wordless token {pre!r}{word!r}{post!r} at {key}")
            if any(c.isalpha() for c in post):
                die(f"letters in post {post!r} at {key}")
            if any(c.isalpha() for c in pre):
                # Leading punctuation (quotes, parens, a carried dialogue dash)
                # then exactly one whitelisted elision prefix.
                bare = pre
                while bare and not bare[0].isalpha():
                    bare = bare[1:]
                if bare.lower() not in ELISION:
                    die(f"pre {pre!r} at {key} is not a whitelisted elision")
            if codes:
                die(f"Strong's code {codes} at {key}: this corpus must carry none")
            if flags & FLAG_ADDED or flags & FLAG_PARA:
                die(f"flag {flags} at {key}: no ADDED or PARA in this corpus")

    # 4. NFC is a no-op (NFD deliberately unasserted — see the module doc).
    blob = "\n".join(lines)
    if unicodedata.normalize("NFC", blob) != blob:
        die("corpus is not NFC-normal")

    # 5. The divine name: flagged wherever bare, only where bare, in band.
    flagged = 0
    for key, toks in verses.items():
        for _, word, _, _, flags in toks:
            if flags & FLAG_DIVINE:
                flagged += 1
                if word not in DIVINE:
                    die(f"DIVINE flag on {word!r} at {key}")
            elif word in DIVINE:
                die(f"bare divine name unflagged at {key}")
    if not 6000 <= flagged <= 7500:
        die(f"{flagged} divine-name tokens; expected the Ostervald band")

    # 6. Superscriptions: Psalms only, verse 1 only, the 62 title psalms.
    titled = set()
    for (b, c, v), toks in verses.items():
        for _, _, _, _, flags in toks:
            if flags & FLAG_TITLE:
                if b != "Ps" or v != 1:
                    die(f"TITLE token outside a psalm's verse 1: {(b, c, v)}")
                titled.add(c)
    if len(titled) != 62 or not TWO_VERSE_TITLES <= titled:
        die(f"{len(titled)} psalms carry titles; expected 62 including the five double-titled")
    # A title is a heading, not the whole verse: body tokens must follow.
    for c in titled:
        toks = verses[("Ps", c, 1)]
        if all(f & FLAG_TITLE for _, _, _, _, f in toks):
            die(f"Ps {c}:1 is title-only; the body verse was not folded in")

    # 7. Every directive site landed where the KJV puts it.
    def text_of(key) -> str:
        return " ".join(p + w + s for p, w, s, _, _ in verses[key])

    for key, needle in LANDMARKS.items():
        if needle not in text_of(key):
            die(f"landmark missing at {key}: {needle!r}")
    rev = text_of(("Rev", 13, 1))
    if not (REV_13_1[0] in rev and REV_13_1[1] in rev and rev.index(REV_13_1[0]) < rev.index(REV_13_1[1])):
        die("Rev 13:1 does not carry the prepended 12:18 ahead of its own text")

    # 8. The splice guard (`check-indic.py` claim 10): a sentence terminator
    # over 1% of the corpus's must be used by at least 90% of its books.
    term_total: dict = defaultdict(int)
    term_books: dict = defaultdict(set)
    for (b, _, _), toks in verses.items():
        for _, _, post, _, _ in toks:
            for ch in post:
                if ch in ".!?…":
                    term_total[ch] += 1
                    term_books[ch].add(b)
    all_terms = sum(term_total.values())
    for ch, n in term_total.items():
        if n > all_terms * 0.01 and len(term_books[ch]) < 60:
            die(f"terminator {ch!r} carries {n} uses but only {len(term_books[ch])} books")

    # 9. The numbering table agrees with the corpus's own remap.
    rows = {}
    for line in NUMBERING.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        b, c, v, printed = line.split("\t")
        key = (b, int(c), int(v))
        if key not in kjv_set:
            die(f"numbering row at a non-address {key}")
        if printed == f"{c}:{v}":
            die(f"numbering row {key} annotates agreement")
        rows[key] = printed
    if not 1200 <= len(rows) <= 1350:
        die(f"{len(rows)} numbering rows; the Ostervald table holds ~1263")
    for key, want in [(("Jonah", 1, 17), "2:1"), (("Ps", 3, 1), "3:2"), (("Isa", 9, 1), "8:23"), (("Job", 41, 1), "40:20")]:
        if rows.get(key) != want:
            die(f"numbering {key} = {rows.get(key)!r}, expected {want!r}")

    # 10-11. With the source: NO LETTER lost, invented, or reordered — proved
    # per BOOK, which is independent of the alignment entirely: titles fold,
    # merges join and splits cut all preserve source order, so each book's
    # letter stream must be identical through a deliberately naive second
    # parser that shares nothing with the builder.
    if src:
        raw = Path(src).read_text(encoding="utf-8")
        src_stream: dict = defaultdict(list)
        for m in VERSE_XML.finditer(raw):
            src_stream[m.group(1)].append(m.group(4))
        got_stream: dict = defaultdict(list)
        for key in order:
            got_stream[key[0]].append(text_of(key))
        for b in kjv_books:
            a = letters("".join(src_stream[b]))
            g = letters("".join(got_stream[b]))
            if a != g:
                at = next(i for i, (x, y) in enumerate(zip(a, g)) if x != y) if len(a) == len(g) else min(len(a), len(g))
                die(f"{b}: letter stream diverges from the source near offset {at}: {a[max(0,at-20):at+20]!r} vs {g[max(0,at-20):at+20]!r}")
        print("source letter streams: 66/66 books identical")

    print(f"ok: 31,102 KJV addresses, 20 TR discriminators, {flagged} divine-name tokens, "
          f"62 title psalms, {len(rows)} numbering rows, every directive landmark in place")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1] if len(sys.argv) == 2 else None))
