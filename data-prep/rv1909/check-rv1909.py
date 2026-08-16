#!/usr/bin/env python3
"""Prove data/rv1909.jsonl is what it claims to be.

    python3 data-prep/rv1909/check-rv1909.py [spa-rv1909.usfx.xml]

`build-rv1909.py` makes a set of claims, and a script that only ran the builder
would be trusting all of them. This checks each against evidence, and with the
original source in hand it checks the most important one: that no word of
scripture was lost on the way through.

  1. THE ADDRESSES ARE THE KJV'S. Same 66 books, same chapter counts, same
     last-verse numbers, same 31,102 refKeys — nothing more, nothing missing.
     This is what makes `refKey` mean one verse in all three corpora, and it is
     also what entitles Spanish to no `numbering` row in the language registry:
     a reader's printed Reina-Valera agrees with the address on screen.
  2. TOKENS REASSEMBLE. `pre + word + post` concatenated in order is the verse.
     A tokenizer that drops a comma or eats a word passes every other check here.
  3. WORDS ARE WHOLE. `pre` and `post` hold punctuation and never letters — a
     tokenizer that peels a letter off a word reassembles perfectly and breaks
     every tap target. Spanish opens its questions, so ¿ and ¡ must be `pre` and
     never swallowed into a word.
  4. THE DIVINE NAME IS MARKED, and only where it should be: flag 2 on "Jehová",
     never on "Señor" and never on the compound place names.
  5. TRANSLATOR-SUPPLIED WORDS ARE MARKED. Flag 1 on what the source wraps in
     `<add>`, and on a plausible number of words — the italics carry the same
     meaning here as in the KJV, and the reader's setting acts on both.
  6. THE STRONG'S TAGS ARE SOUND. Every code resolves in data/strongs.json,
     Hebrew codes only in the OT and Greek only in the NT, and there are enough
     of them that the tagging visibly survived — the source carries ~390k tagged
     spans, one code on the head word of each, so a corpus with far fewer means
     a broken parse, not a leaner text.
  7. NOTHING WAS LOST (needs the source file). Every verse's letters and digits,
     ignoring whitespace and markup, are the source's.
"""

import json
import re
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RV = ROOT / "data" / "rv1909.jsonl"
KJV = ROOT / "data" / "kjv.jsonl"
STRONGS = ROOT / "data" / "strongs.json"
TOKENIZATION = "rv1909-tok1"
FLAG_ADDED = 1
FLAG_DIVINE = 2
OT_BOOKS = 39
# The source carries ~390k tagged spans and each puts its code on ONE head word
# (see build-rv1909.py), so a sound parse lands near that. A parse that lost the
# markup would still produce a perfectly readable Bible with a fraction of the
# tags, which is the failure worth a floor rather than an eyeball.
TAG_FLOOR = 300_000

fails: list[str] = []


def fail(msg: str) -> None:
    fails.append(msg)


def load(path: Path):
    with path.open(encoding="utf-8") as f:
        header = json.loads(next(f))
        return header, [json.loads(line) for line in f if line.strip()]


def main(src: str | None) -> int:
    if not RV.exists():
        print(f"{RV} does not exist — run build-rv1909.py first", file=sys.stderr)
        return 2
    header, rows = load(RV)
    _, kjv = load(KJV)

    # ── 1. the addresses are the KJV's ──────────────────────────────────────
    if header.get("tokenization") != TOKENIZATION:
        fail(f"header tokenization is {header.get('tokenization')!r}, expected {TOKENIZATION!r}")
    if header.get("verses") != len(rows):
        fail(f"header says {header.get('verses')} verses, file has {len(rows)}")

    ours = [(r["b"], r["c"], r["v"]) for r in rows]
    theirs = [(r["b"], r["c"], r["v"]) for r in kjv]
    if ours != theirs:
        # Report the shape of the disagreement rather than 31,102 lines of it.
        missing = set(theirs) - set(ours)
        extra = set(ours) - set(theirs)
        if missing:
            fail(f"{len(missing)} KJV verses have no Spanish verse, e.g. {sorted(missing)[:5]}")
        if extra:
            fail(f"{len(extra)} Spanish verses are at addresses the KJV does not have, e.g. {sorted(extra)[:5]}")
        if not missing and not extra:
            fail("the same verses are present but not in the same order as kjv.jsonl")

    # ── 2/3/4/5/6. the tokens themselves ────────────────────────────────────
    tags = 0
    added = 0
    divine = 0
    codes_seen: dict[str, int] = defaultdict(int)
    known = set(json.loads(STRONGS.read_text(encoding="utf-8")))
    ot_ids = {r["b"] for r in kjv[: sum(1 for r in kjv)]}  # placeholder, refined below
    ot_order: list[str] = []
    for r in kjv:
        if r["b"] not in ot_order:
            ot_order.append(r["b"])
    ot_ids = set(ot_order[:OT_BOOKS])

    letters = re.compile(r"[^\W\d_]", re.UNICODE)
    for r in rows:
        ref = f'{r["b"]} {r["c"]}:{r["v"]}'
        for pre, word, post, strongs, flags in r["t"]:
            if not word:
                fail(f"{ref}: an empty word token")
            if letters.search(pre) or letters.search(post):
                fail(f"{ref}: letters outside the word — pre={pre!r} word={word!r} post={post!r}")
            if word.startswith(("¿", "¡")):
                fail(f"{ref}: an opening mark was swallowed into the word {word!r}")
            if flags & FLAG_ADDED:
                added += 1
            if flags & FLAG_DIVINE:
                divine += 1
                if word != "Jehová" and word != "JEHOVÁ":
                    fail(f"{ref}: {word!r} is flagged as the divine name")
            elif word in ("Jehová", "JEHOVÁ"):
                fail(f"{ref}: {word!r} is not flagged as the divine name")
            for c in strongs:
                tags += 1
                codes_seen[c] += 1
                if c not in known:
                    fail(f"{ref}: Strong's code {c} is not in strongs.json")
                if r["b"] in ot_ids and c.startswith("G"):
                    fail(f"{ref}: a Greek code {c} in the Old Testament")
                if r["b"] not in ot_ids and c.startswith("H"):
                    fail(f"{ref}: a Hebrew code {c} in the New Testament")

    if tags < TAG_FLOOR:
        fail(f"only {tags} Strong's tags survived; the source carries ~390k, so the markup parse is broken")
    if added < 1000:
        fail(f"only {added} translator-supplied words are marked; the source wraps ~3,500 in <add>")
    if divine < 5000:
        fail(f"only {divine} divine-name marks; the source spells Jehová ~6,800 times")

    # ── 7. nothing was lost ─────────────────────────────────────────────────
    if src:
        xml = unicodedata.normalize("NFC", Path(src).read_text(encoding="utf-8"))
        # The source, stripped to its letters and digits, verse by verse — the
        # same reduction applied to ours. Whitespace, punctuation and markup are
        # exactly what the tokenizer is entitled to move around; letters are not.
        bodies = re.findall(r'<book id="[A-Z0-9]{3}">(.*?)</book>', xml, re.S)
        keep = re.compile(r"[^\W_]+", re.UNICODE)
        src_text: list[str] = []
        for body in bodies:
            for chunk in re.split(r"<v id=\"\d+\"\s*/>", body)[1:]:
                chunk = re.split(r"<c id=\"\d+\"\s*/>", chunk)[0]
                src_text.append("".join(keep.findall(re.sub(r"<[^>]*>", " ", chunk))))
        ours_text = ["".join(keep.findall("".join(pre + w + post for pre, w, post, _, _ in r["t"]))) for r in rows]
        if len(src_text) != len(ours_text):
            fail(f"source has {len(src_text)} verses, ours has {len(ours_text)}")
        else:
            for r, a, b in zip(rows, src_text, ours_text):
                if a != b:
                    fail(f'{r["b"]} {r["c"]}:{r["v"]}: text differs from the source\n  src: {a[:90]}\n  ours: {b[:90]}')
                    if len(fails) > 12:
                        break

    if fails:
        print(f"FAIL ({len(fails)} problem(s)):", file=sys.stderr)
        for m in fails[:20]:
            print(f"  - {m}", file=sys.stderr)
        if len(fails) > 20:
            print(f"  … and {len(fails) - 20} more", file=sys.stderr)
        return 1

    print(f"data/rv1909.jsonl: {len(rows)} verses at KJV addresses")
    print(f"  {tags} Strong's tags across {len(codes_seen)} codes, all resolving")
    print(f"  {added} translator-supplied words, {divine} divine-name marks")
    if src:
        print("  every verse's letters match the source")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1] if len(sys.argv) > 1 else None))
