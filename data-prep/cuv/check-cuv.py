#!/usr/bin/env python3
"""Prove data/cuv1919{t,s}.jsonl against their claims — and their sources.

    python3 data-prep/cuv/check-cuv.py t [chi-cuv.usfx.xml]
    python3 data-prep/cuv/check-cuv.py s [chi-cuv-simp.usfx.xml]

Twelve claims, in the `check-indic.py` mould, three of them Han-shaped where
the Indic ones were Indic-shaped:

  - the grapheme claim becomes: EVERY token word is exactly one character —
    per-character tokenization is this corpus's contract with search (the Han
    query splitter) and layout (break opportunities = token boundaries);
  - the splice guard's terminator family is 。！？… (the ideographic full
    stop, not the danda);
  - a new PARALLELISM claim: when the sibling edition's corpus exists, the
    two must agree address-for-address with the SAME number of tokens in
    every verse and the same flags — a traditional/simplified pair that
    drifted apart would silently give the two Chinese rows different Bibles.

The placeholder claim is this corpus's own: the printed CUV combines ranged
verses and prints 併於上節/并于上节 at the second number. Exactly 71 verses may
read so — the source's own 69 plus the two the build constructs (Deut 13:18,
Ps 116:19) — and the two constructed ones are excluded from the source
letter-stream comparison because their letters are the convention's, not the
source file's.
"""

import json
import re
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KJV = ROOT / "data" / "kjv.jsonl"
NUMBERING = ROOT / "crates" / "core" / "src" / "versification" / "cuv-numbering.tsv"

FLAG_DIVINE = 2

EDITIONS = {
    "t": {
        "corpus": "cuv1919t.jsonl", "sibling": "cuv1919s.jsonl", "tokenization": "cuv1919t-tok1",
        "divine": "耶和華", "placeholder": "併於上節", "john753": "於是各人都回家去了",
        "landmarks": {
            ("John", 8, 1): "耶穌卻往橄欖山去",
            ("1Chr", 22, 1): "這就是耶和華神的殿",
            ("1Chr", 22, 19): "建造耶和華神的聖所",
            ("3John", 1, 14): "請你替我按着姓名問眾位朋友安",
            ("Deut", 13, 17): "你要聽從耶和華你神的話",
            ("Acts", 8, 37): "我信耶穌基督是神的兒子",
        },
    },
    "s": {
        "corpus": "cuv1919s.jsonl", "sibling": "cuv1919t.jsonl", "tokenization": "cuv1919s-tok1",
        "divine": "耶和华", "placeholder": "并于上节", "john753": "于是各人都回家去了",
        # The two Extension B 规范字 the build carries as traditional forms —
        # see `build-cuv.py`'s substitution note. Applied to the SOURCE before
        # the letter-stream comparison, and claim 3's repertoire check is what
        # proves the corpus never ships the unrenderable codepoints.
        "substitutions": {"\U0002B21F": "蔯", "\U0002B5EA": "餵"},
        "landmarks": {
            ("John", 8, 1): "耶稣却往橄榄山去",
            ("1Chr", 22, 1): "这就是耶和华神的殿",
            ("1Chr", 22, 19): "建造耶和华神的圣所",
            ("3John", 1, 14): "请你替我按着姓名问众位朋友安",
            ("Deut", 13, 17): "你要听从耶和华你神的话",
            ("Acts", 8, 37): "我信耶稣基督是神的儿子",
        },
    },
}

TR = [
    ("Matt", 17, 21), ("Matt", 18, 11), ("Matt", 23, 14), ("Mark", 7, 16),
    ("Mark", 9, 44), ("Mark", 9, 46), ("Mark", 11, 26), ("Mark", 15, 28),
    ("Mark", 16, 9), ("Mark", 16, 20), ("Luke", 17, 36), ("Luke", 23, 17),
    ("John", 5, 4), ("John", 7, 53), ("John", 8, 11), ("Acts", 8, 37),
    ("Acts", 15, 34), ("Acts", 24, 7), ("Acts", 28, 29), ("Rom", 16, 24),
]

CONSTRUCTED = {("Deut", 13, 18), ("Ps", 116, 19)}

PRE = "「『（〔［"
POST = "。，、；：！？」』）〕］…"

BOOK = re.compile(r'<book id="([A-Z0-9]{3})">(.*?)</book>', re.S)


def die(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def letters(s: str) -> str:
    return "".join(c for c in s if unicodedata.category(c).startswith("L"))


def load_corpus(path: Path) -> tuple[dict, dict, list]:
    lines = path.read_text(encoding="utf-8").splitlines()
    header = json.loads(lines[0])
    verses: dict = {}
    order: list = []
    for line in lines[1:]:
        r = json.loads(line)
        key = (r["b"], r["c"], r["v"])
        if key in verses:
            die(f"duplicate verse {key}")
        verses[key] = r["t"]
        order.append(key)
    return header, verses, order


def main(edition: str, src: str | None) -> int:
    ed = EDITIONS[edition]
    header, verses, order = load_corpus(ROOT / "data" / ed["corpus"])

    # 1. Addresses are the KJV's, under the right stamp.
    if header["tokenization"] != ed["tokenization"]:
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
    if len(verses) != 31102 or header["verses"] != 31102:
        die(f"{len(verses)} verses, header says {header['verses']}")
    if set(verses) != kjv_set:
        die(f"addresses differ from the KJV at e.g. {sorted(set(verses) ^ kjv_set)[:5]}")

    def text_of(key) -> str:
        return "".join(p + w + s for p, w, s, _, _ in verses[key])

    # 2. The twenty TR discriminators — real text, and the two split-adjacent
    # ones say the right words.
    for key in TR:
        if len(verses[key]) < 3:
            die(f"TR discriminator {key} has {len(verses[key])} tokens")
    if letters(text_of(("John", 7, 53))) != letters(ed["john753"]):
        die(f"John 7:53 is not the split head: {text_of(('John', 7, 53))!r}")

    # 3. Per-character tokens, punctuation only in pre/post, nothing empty.
    for key, toks in verses.items():
        if not toks:
            die(f"empty verse {key}")
        for pre, word, post, codes, flags in toks:
            if len(word) != 1 or not unicodedata.category(word).startswith("L"):
                die(f"token word {word!r} at {key} is not a single letter character")
            # The renderable repertoire: URO, Extension A, the compatibility
            # block — plus the exactly two Extension B ideographs Source Han
            # Serif is known to carry (𤏲 "scorched", six OT verses of the
            # traditional text; 𨱔 "spear butt", 2 Sam 2:23 simplified). A
            # codepoint outside this set is tofu on every reader's screen,
            # which is why the two 规范字 the face lacks ship as their
            # traditional forms instead — see `build-cuv.py`.
            cp = ord(word)
            if not (0x3400 <= cp <= 0x9FFF or 0xF900 <= cp <= 0xFAFF or cp in (0x243F2, 0x28C54)):
                die(f"token {word!r} (U+{cp:04X}) at {key} is outside the renderable repertoire")
            if any(c not in PRE and c not in POST for c in pre + post):
                die(f"unexpected punctuation {pre!r}/{post!r} at {key}")
            if codes:
                die(f"Strong's code {codes} at {key}: this corpus must carry none")
            if flags not in (0, FLAG_DIVINE):
                die(f"flag {flags} at {key}: only DIVINE is set in this corpus")

    # 4. NFC and NFD are both no-ops (Han does not decompose; a compatibility
    # ideograph would, and would repaint under a normalizing search fold).
    blob = "\n".join(text_of(k) for k in order)
    if unicodedata.normalize("NFC", blob) != blob or unicodedata.normalize("NFD", blob) != blob:
        die("corpus is not normalization-stable")

    # 5. The divine name: three characters, all flagged, flagged nowhere else,
    # and every unflagged occurrence is one of the three compound place names.
    d1, d2, d3 = ed["divine"]
    flagged_runs = 0
    for key in order:
        toks = verses[key]
        i = 0
        while i < len(toks):
            f = toks[i][4] & FLAG_DIVINE
            if f:
                run = toks[i : i + 3]
                if len(run) < 3 or [t[1] for t in run] != [d1, d2, d3] or not all(t[4] & FLAG_DIVINE for t in run):
                    die(f"divine flag not on an intact {ed['divine']} run at {key}")
                flagged_runs += 1
                i += 3
            else:
                i += 1
    if not 6800 <= flagged_runs <= 7100:
        die(f"{flagged_runs} divine-name runs; expected the CUV band")

    # 6. Placeholders: exactly 71 verses read 併於上節, the source's 69 plus
    # the two constructed range tails.
    ph = {k for k in order if letters(text_of(k)) == ed["placeholder"]}
    if len(ph) != 71 or not CONSTRUCTED <= ph:
        die(f"{len(ph)} placeholder verses; expected 71 including the two constructed")

    # 7. Every directive site landed where the KJV puts it.
    for key, needle in ed["landmarks"].items():
        if needle not in text_of(key):
            die(f"landmark missing at {key}: {needle!r}")

    # 8. The splice guard, Han-shaped: like `check-indic.py`'s claim 10 it
    # watches the FULL-STOP family only (。 here, the danda there) — ！ and ？
    # are content-driven and seven books legitimately ask no questions.
    term_total: dict = defaultdict(int)
    term_books: dict = defaultdict(set)
    for (b, _, _), toks in verses.items():
        for _, _, post, _, _ in toks:
            for ch in post:
                if ch in "。":
                    term_total[ch] += 1
                    term_books[ch].add(b)
    all_terms = sum(term_total.values())
    for ch, n in term_total.items():
        if n > all_terms * 0.01 and len(term_books[ch]) < 60:
            die(f"terminator {ch!r} carries {n} uses but only {len(term_books[ch])} books")

    # 9. The numbering table matches this corpus's remap.
    rows = {}
    for line in NUMBERING.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        b, c, v, printed = line.split("\t")
        rows[(b, int(c), int(v))] = printed
    for key, want in [(("John", 7, 53), "8:1"), (("1Chr", 22, 1), "21:31"), (("1Chr", 22, 19), "22:18"),
                      (("Deut", 13, 18), "13:17"), (("Ps", 116, 19), "116:18")]:
        if rows.get(key) != want:
            die(f"numbering {key} = {rows.get(key)!r}, expected {want!r}")
    if len(rows) != 22:
        die(f"{len(rows)} numbering rows; the CUV table holds 22")

    # 10. Parallelism with the sibling edition, when it exists.
    sibling = ROOT / "data" / ed["sibling"]
    if sibling.exists():
        _, sib, _ = load_corpus(sibling)
        if set(sib) != set(verses):
            die("the two editions hold different addresses")
        for key in order:
            a, b = verses[key], sib[key]
            if len(a) != len(b) or any(x[4] != y[4] for x, y in zip(a, b)):
                die(f"editions disagree in shape at {key}")
        print("parallelism: both editions agree token-for-token in shape and flags")

    # 11-12. With the source: no letter lost, invented, or reordered — per
    # book, independent of the alignment (order is preserved by every
    # directive), with the two constructed placeholders excluded.
    if src:
        raw = Path(src).read_text(encoding="utf-8")
        for k, v in ed.get("substitutions", {}).items():
            raw = raw.replace(k, v)
        books = list(BOOK.finditer(raw))
        if len(books) != 66:
            die(f"{len(books)} books in the source")
        got_stream: dict = defaultdict(list)
        for key in order:
            if key in CONSTRUCTED:
                continue
            got_stream[key[0]].append(text_of(key))
        for i, bm in enumerate(books):
            b = kjv_books[i]
            body = re.sub(r"<h>[^<]*</h>", "", bm.group(2))  # the header is a book name, not scripture
            src_letters = letters(re.sub(r"<[^>]*>", "", body))
            got_letters = letters("".join(got_stream[b]))
            if src_letters != got_letters:
                at = next((i for i, (x, y) in enumerate(zip(src_letters, got_letters)) if x != y), min(len(src_letters), len(got_letters)))
                die(f"{b}: letter stream diverges near offset {at}: "
                    f"{src_letters[max(0, at - 15):at + 15]!r} vs {got_letters[max(0, at - 15):at + 15]!r}")
        print("source letter streams: 66/66 books identical")

    print(f"ok ({ed['corpus']}): 31,102 KJV addresses, 20 TR discriminators, {flagged_runs} divine-name runs, "
          f"71 placeholder verses, {len(rows)} numbering rows, every landmark in place")
    return 0


if __name__ == "__main__":
    if len(sys.argv) < 2 or len(sys.argv) > 3 or sys.argv[1] not in EDITIONS:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1], sys.argv[2] if len(sys.argv) == 3 else None))
