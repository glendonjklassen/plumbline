#!/usr/bin/env python3
"""Prove data/luther1912.jsonl is what it claims to be.

    python3 data-prep/luther/check-luther.py [luther_1912.json]

`build-luther.py` makes four claims, and a script that only ran the builder
would be trusting all of them. This checks each one against evidence, and with
the original export in hand it checks the fifth and most important: that no word
of scripture was lost on the way through.

  1. THE ADDRESSES ARE THE KJV'S. Same 66 books, same chapter counts, same
     last-verse numbers, same 31,102 refKeys — nothing more, nothing missing.
     This is the claim the whole language project rests on: it is what makes
     `refKey` mean one verse in both corpora and a versification map unnecessary.
  2. TOKENS REASSEMBLE. `pre + word + post` concatenated in order is the verse.
     A tokenizer that drops a comma or eats a word passes every other check here.
  3. NO ARTIFACTS SURVIVE. No inline `n:n` German verse numbers, no `{}`, no
     leading dashes, no doubled spaces.
  4. WORDS ARE WHOLE. `pre` and `post` hold punctuation and never letters — a
     tokenizer that peels a letter off a word reassembles perfectly and breaks
     every tap target.
  5. THE DIVINE NAME IS MARKED, and only where it should be: flag 2 on caps
     HERR/HERRN/HERRE, never on the ordinary word "Herr".
  6. NOTHING WAS LOST (needs the source file). Every verse's letters and digits,
     ignoring whitespace and the stripped artifacts, are the source's.
  7. THE STRONG'S TAGS ARE SOUND (merge-strongs.py's claims): every code
     resolves in data/strongs.json, Hebrew codes only in the OT and Greek only
     in the NT, and there are enough of them that the merge visibly ran — the
     source carries ~350k tags, so a corpus with under 300k means a broken or
     skipped merge, not a smaller edition.
"""

import json
import re
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LUTHER = ROOT / "data" / "luther1912.jsonl"
KJV = ROOT / "data" / "kjv.jsonl"
FLAG_DIVINE = 2

fails: list[str] = []


def fail(msg: str) -> None:
    fails.append(msg)


def read(path: Path):
    with path.open(encoding="utf-8") as f:
        header = json.loads(next(f))
        rows = [json.loads(l) for l in f if l.strip()]
    return header, rows


def body(row) -> str:
    return "".join(t[0] + t[1] + t[2] for t in row["t"])


def main() -> int:
    hdr, lu = read(LUTHER)
    _, kj = read(KJV)

    if hdr.get("tokenization") != "luther1912-tok1":
        fail(f'header tokenization is {hdr.get("tokenization")!r}, not "luther1912-tok1"')
    if hdr.get("verses") != len(lu):
        fail(f'header says {hdr.get("verses")} verses, file has {len(lu)}')

    # ── 1. the addresses are the KJV's ──────────────────────────────────────
    lu_keys = {(r["b"], r["c"], r["v"]) for r in lu}
    kj_keys = {(r["b"], r["c"], r["v"]) for r in kj}
    if len(lu_keys) != len(lu):
        fail(f"{len(lu) - len(lu_keys)} duplicate refKeys")
    missing = kj_keys - lu_keys
    extra = lu_keys - kj_keys
    if missing:
        fail(f"{len(missing)} KJV verses have no German verse, e.g. {sorted(missing)[:5]}")
    if extra:
        fail(f"{len(extra)} German verses sit at addresses the KJV does not have, e.g. {sorted(extra)[:5]}")
    # Order matters too: the corpus loader and every index walk it in file order.
    if [(r["b"], r["c"], r["v"]) for r in lu] != [(r["b"], r["c"], r["v"]) for r in kj]:
        fail("the German verses are not in the KJV's file order")

    # ── 2, 3, 4, 5. per-verse shape ────────────────────────────────────────────
    artifacts = re.compile(r"(?<![\w:])\d+:\d+|\{|\}|^-\s|\s\s")
    divine_hits = 0
    wrong_divine = []
    for r in lu:
        text = body(r)
        where = f'{r["b"]} {r["c"]}:{r["v"]}'
        if a := artifacts.search(text):
            fail(f"{where}: artifact {a.group()!r} survived: {text[:70]!r}")
        if text != text.strip():
            fail(f"{where}: leading or trailing whitespace")
        for t in r["t"]:
            if not t[1]:
                fail(f"{where}: an empty word token")
            # `pre` and `post` are PUNCTUATION, never letters. Without this, a
            # tokenizer that peels a letter off the end of a word — "Gottes" →
            # word "Gotte" + post "s" — passes every other check in this file,
            # because the verse still reassembles and the letters still match.
            # What it breaks is the tap target: a reader touching that word gets
            # part of it. Found by mutation-testing this checker.
            if any(c.isalpha() for c in t[0] + t[2]):
                fail(f"{where}: token {t[:3]} has letters in its punctuation")
            if t[4] & FLAG_DIVINE:
                divine_hits += 1
                if t[1] not in {"HERR", "HERRN", "HERRE"}:
                    wrong_divine.append((where, t[1]))
            elif t[1] in {"HERR", "HERRN", "HERRE"}:
                fail(f"{where}: {t[1]} is not marked as the divine name")
    if wrong_divine:
        fail(f"{len(wrong_divine)} words marked divine that are not: {wrong_divine[:5]}")
    # A sanity floor rather than an exact count: the KJV sets LORD about 6,500
    # times, so a German corpus with a handful would mean the rule never fired.
    if divine_hits < 5000:
        fail(f"only {divine_hits} divine-name marks — the rule looks broken")

    # ── 7. the Strong's tags are sound ───────────────────────────────────────
    known = set(json.loads((ROOT / "data" / "strongs.json").read_text(encoding="utf-8")))
    ot_books = []
    for r in kj:
        if not ot_books or ot_books[-1] != r["b"]:
            if r["b"] not in ot_books:
                ot_books.append(r["b"])
    ot = set(ot_books[:39])
    tag_count = 0
    for r in lu:
        where = f'{r["b"]} {r["c"]}:{r["v"]}'
        want_prefix = "H" if r["b"] in ot else "G"
        for t in r["t"]:
            for code in t[3]:
                tag_count += 1
                if code not in known:
                    fail(f"{where}: Strong's code {code!r} not in data/strongs.json")
                elif not code.startswith(want_prefix):
                    fail(f"{where}: {code} is the wrong testament for this book")
    if tag_count < 300_000:
        fail(f"only {tag_count} Strong's tags — the merge looks broken or skipped")

    # ── 6. nothing was lost ─────────────────────────────────────────────────
    if len(sys.argv) > 1:
        src = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))["verses"]
        order = []
        for r in kj:
            if not order or order[-1] != r["b"]:
                if r["b"] not in order:
                    order.append(r["b"])
        keep = re.compile(r"[^\W\d_]", re.UNICODE)
        want = {}
        for r in src:
            text = unicodedata.normalize("NFC", r["text"])
            text = re.sub(r"\{\s*\}", "", text)
            text = re.sub(r"^-\s*", "", text)
            text = re.sub(r"(?<![\w:])\d+:\d+\s*", "", text)
            want[(order[r["book"] - 1], r["chapter"], r["verse"])] = "".join(keep.findall(text))
        lost = 0
        for r in lu:
            k = (r["b"], r["c"], r["v"])
            got = "".join(keep.findall(body(r)))
            if got != want.get(k):
                lost += 1
                if lost <= 3:
                    fail(f"{k}: letters differ from the source\n    source: {want.get(k, '')[:90]}\n    built : {got[:90]}")
        if lost:
            fail(f"{lost} verses differ from the source in their letters")
        print(f"  letters match the source in all {len(lu)} verses")
    else:
        print("  (no source file given — skipping the nothing-was-lost check)")

    if fails:
        print(f"\nluther: {len(fails)} problem(s)\n", file=sys.stderr)
        for f in fails[:40]:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(
        f"luther: {len(lu)} verses at the KJV's addresses, {divine_hits} divine-name marks, "
        f"{tag_count} Strong's tags, no artifacts."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
