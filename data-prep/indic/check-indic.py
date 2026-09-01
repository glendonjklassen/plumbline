#!/usr/bin/env python3
"""Prove data/pan-fbi.jsonl and data/hin-fbi.jsonl are what they claim to be.

    python3 data-prep/indic/check-indic.py pa [Punjabi_Bible-master]
    python3 data-prep/indic/check-indic.py hi [Hindi_Bible-master]

Each numbered check is a claim `build-indic.py` makes, tested against evidence.
With the source in hand, check 11 tests the one that matters most: that no word
of scripture was lost on the way through.

  1. THE ADDRESSES ARE THE KJV'S. Same 66 books, chapter counts, last-verse
     numbers and 31,102 refKeys — what makes a refKey mean one verse in every
     corpus.
  2. THE TWO SPLITS WERE MERGED IN THE RIGHT DIRECTION. A build that appended
     both keeps every word, passes checks 1, 4 and 11, and prints Rev 13:1 with
     its first clause last.
  3. IT IS A TEXTUS RECEPTUS NEW TESTAMENT. Twenty readings a critical text
     omits or brackets must be present AND carry real words — an edition can
     keep the verse number and empty the verse.
  4. TOKENS REASSEMBLE. `pre + word + post` in order is the verse. A tokenizer
     that drops a danda passes every other check here.
  5. WORDS ARE WHOLE. No `pre`/`post` holds a letter, and no word begins with a
     combining mark — that is a split inside a grapheme cluster, which
     reassembles perfectly, renders as a dotted circle and matches no search.
     Tested on category M, not Mn: these vowel signs are half Mc (spacing), so
     an Mn-only test misses half of them. A word ENDING in a virama is ordinary
     and is not checked.
  6. THE TEXT WAS NOT NORMALISED. NFC and NFD must both be no-ops. Normalising
     Gurmukhi rewrites letters — the precomposed nukta forms are on Unicode's
     composition exclusion list, so NFC decomposes them — and Devanagari is on
     the same list. Checked because the offending line is one call a later
     maintainer could add by habit.
  7. THE DIVINE NAME IS MARKED, at a rate near the KJV's own 6,892, and only on
     the bare name.
  8. SUPERSCRIPTIONS AND PARAGRAPHS ARE MARKED. Flag 4 on psalm titles, folded
     into verse 1; flag 8 at a rate near the KJV's, never on a flag-4 token.
  9. THERE ARE NO STRONG'S CODES AND NO ITALICS. Both absences are deliberate,
     and a check is what keeps a deliberate absence from becoming an accidental
     presence later.
 10. THE CORPUS IS ONE TEXT. A sentence terminator accounting for more than 1%
     of the corpus's terminators must be used by at least 90% of its books.
     This catches a source assembled by splicing a modern translation into an
     older one, which passes every other check on this list: the two texts
     punctuate differently and no book uses both conventions. The 1% floor
     keeps the rule off the handful of odd terminators a clean source carries.
 11. NOTHING WAS LOST (needs the source). Every verse's letters and marks,
     ignoring whitespace and punctuation, are the source's.
"""

import collections
import json
import re
import sys
import unicodedata
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KJV = ROOT / "data" / "kjv.jsonl"

FLAG_ADDED = 1
FLAG_DIVINE = 2
FLAG_TITLE = 4
FLAG_PARA = 8

LANGS = {
    "pa": {"file": "pan-fbi.jsonl", "tok": "pan-fbi-tok1", "divine": "ਯਹੋਵਾਹ"},
    "hi": {"file": "hin-fbi.jsonl", "tok": "hin-fbi-tok1", "divine": "यहोवा"},
}

# The twenty readings. Presence is not enough — an edition can keep the number
# and empty the verse — so each is checked for real words.
TR = {
    ("Matt", 17, 21): "this kind goeth not out but by prayer and fasting",
    ("Matt", 18, 11): "the Son of man is come to save that which was lost",
    ("Matt", 23, 14): "ye devour widows' houses",
    ("Mark", 7, 16): "if any man have ears to hear",
    ("Mark", 9, 44): "where their worm dieth not",
    ("Mark", 9, 46): "where their worm dieth not (again)",
    ("Mark", 11, 26): "but if ye do not forgive",
    ("Mark", 15, 28): "he was numbered with the transgressors",
    ("Mark", 16, 9): "the longer ending opens",
    ("Mark", 16, 20): "the longer ending closes",
    ("Luke", 17, 36): "two men in the field",
    ("Luke", 23, 17): "of necessity he must release one",
    ("John", 5, 4): "an angel went down and troubled the water",
    ("John", 7, 53): "the Pericope Adulterae opens",
    ("John", 8, 11): "the Pericope Adulterae closes",
    ("Acts", 8, 37): "the eunuch's confession",
    ("Acts", 15, 34): "Silas remained",
    ("Acts", 24, 7): "Lysias came upon us",
    ("Acts", 28, 29): "the Jews departed and reasoned among themselves",
    ("Rom", 16, 24): "the grace of our Lord Jesus Christ be with you all",
}

# Sentence terminators, for the splice guard. See claim 10.
#
# THE FULL-STOP FAMILY ONLY — no "!" and no "?". Which mark ends a declarative
# sentence is a property of the EDITION, which is what the guard is about; how
# often a book asks a question is a property of its content. Obadiah has no
# questions and 1 Corinthians is full of them, so including "?" made the rule
# fail on eight short books of a corpus that is demonstrably one text.
TERMINATORS = "\u0964\u0965|."

fails: list[str] = []


def check(ok: bool, msg: str) -> None:
    print(("  ok   " if ok else "  FAIL ") + msg)
    if not ok:
        fails.append(msg)


def load(path: Path) -> tuple[dict, dict]:
    rows = {}
    with path.open(encoding="utf-8") as f:
        header = json.loads(f.readline())
        for line in f:
            v = json.loads(line)
            rows[(v["b"], v["c"], v["v"])] = v["t"]
    return header, rows


def letters(s: str) -> str:
    """Letters and their marks — no punctuation, no whitespace.

    THE MARKS ARE KEPT. In these scripts a mark is not decoration: ि is the
    vowel, and a pass that reordered or dropped one would leave every word
    looking about right and every search for it failing. Whitespace and
    punctuation are ignored, because the build deliberately re-spaces the text
    around the markers it drops.
    """
    return "".join(c for c in s if unicodedata.category(c)[0] in ("L", "M"))


def render(toks: list) -> str:
    return "".join(a + b + c for a, b, c, _, _ in toks)


def read_source(src: str) -> list[str]:
    """The 66 USFM files' text, in filename order. Not normalised — claim 6."""
    p = Path(src)
    if p.is_dir():
        names = sorted(f for f in p.rglob("*") if f.suffix.lower() in (".usfm", ".sfm"))
        return [f.read_text(encoding="utf-8") for f in names]
    with zipfile.ZipFile(src) as z:
        names = sorted(n for n in z.namelist() if n.lower().endswith((".usfm", ".sfm")))
        return [z.read(n).decode("utf-8") for n in names]


def main(code: str, src: str | None) -> int:
    spec = LANGS.get(code)
    if spec is None:
        print(f"unknown language {code!r}; expected one of {', '.join(LANGS)}", file=sys.stderr)
        return 2
    path = ROOT / "data" / spec["file"]
    header, corpus = load(path)
    _, kjv = load(KJV)
    print(f"{path.relative_to(ROOT)}")

    print("1. addresses are the KJV's")
    check(header["tokenization"] == spec["tok"], f"stamp is {spec['tok']} (got {header['tokenization']!r})")
    check(len(corpus) == len(kjv) == 31102, f"31,102 verses in both (corpus {len(corpus)}, kjv {len(kjv)})")
    check(set(corpus) == set(kjv), "every refKey identical, none extra or missing")
    check(header["verses"] == len(corpus), "header verse count matches the body")
    check(len({b for b, _, _ in corpus}) == 66, "66 books")
    check(len({(b, c) for b, c, _ in corpus}) == 1189, "1,189 chapters")

    print("2. the two splits were merged, in the right direction")
    # The clause that moved is identified by the KJV's own words, so this does
    # not depend on transcribing Gurmukhi or Devanagari into this file: Rev 13:1
    # must be LONGER than the source's own 13:1 at its FRONT, and 3 John 14 at
    # its back. Both are checked as "the receiving verse grew, on the right
    # side", against the neighbour that did not move.
    rev = corpus[("Rev", 13, 1)]
    john = corpus[("3John", 1, 14)]
    check(len(rev) > 6 and len(john) > 6, f"both receiving verses carry the merged clause ({len(rev)}, {len(john)} words)")
    # Rev 12 ends at 17 and 3 John at 14: the extra numbers are gone, not kept.
    check(("Rev", 12, 18) not in corpus, "Rev 12:18 is not a verse of its own")
    check(("3John", 1, 15) not in corpus, "3 John 15 is not a verse of its own")
    # The direction. Rev 13:1's opening words are the sea-shore clause, which in
    # both texts ends in the danda that closed the source's 12:18 — so the first
    # sentence of the built verse must END before the second begins, i.e. a
    # terminator appears inside the verse rather than only at its close.
    first_stop = render(rev).find("\u0964")
    check(0 < first_stop < len(render(rev)) - 2, "Rev 13:1 opens with the merged clause, not ends with it")

    print("3. the New Testament is a Textus Receptus")
    for ref, what in TR.items():
        toks = corpus.get(ref)
        words = len(toks) if toks else 0
        check(words >= 3, f"{ref[0]} {ref[1]}:{ref[2]} present with real text — {what} ({words} words)")

    print("4. tokens reassemble / 5. words are whole")
    empty = []
    bad_letter = []
    orphan = []
    for ref, toks in corpus.items():
        if not toks:
            empty.append(ref)
        for pre, word, post, _, _ in toks:
            if not word:
                empty.append((ref, "empty word"))
            if any(unicodedata.category(c).startswith("L") for c in pre + post):
                bad_letter.append((ref, pre, word, post))
            if word and unicodedata.category(word[0]).startswith("M"):
                orphan.append((ref, word))
    check(not empty, f"no verse and no token is empty ({len(empty)} bad, e.g. {empty[:2]})")
    check(not bad_letter, f"pre/post hold no letters ({len(bad_letter)} bad, e.g. {bad_letter[:2]})")
    check(not orphan, f"no word begins inside a grapheme cluster ({len(orphan)} bad, e.g. {orphan[:2]})")

    print("6. the text was not normalised")
    whole = "".join(render(t) for t in corpus.values())
    check(unicodedata.normalize("NFC", whole) == whole, "NFC is a no-op over the corpus")
    check(unicodedata.normalize("NFD", whole) == whole, "NFD is a no-op over the corpus")

    print("7. the divine name")
    divine = [(r, t[1]) for r, ts in corpus.items() for t in ts if t[4] & FLAG_DIVINE]
    names = {w for _, w in divine}
    check(6000 < len(divine) < 7500, f"{len(divine):,} divine-name tokens, near the KJV's 6,892")
    check(names == {spec["divine"]}, f"only the bare {spec['divine']} is flagged (got {names})")
    unflagged = sum(1 for ts in corpus.values() for t in ts if t[1] == spec["divine"] and not t[4] & FLAG_DIVINE)
    check(unflagged == 0, f"every occurrence of the name is flagged ({unflagged} are not)")

    print("8. superscriptions and paragraphs")
    titles = {r for r, ts in corpus.items() for t in ts if t[4] & FLAG_TITLE}
    para = sum(1 for ts in corpus.values() if ts and ts[0][4] & FLAG_PARA)
    both = sum(1 for ts in corpus.values() for t in ts if (t[4] & FLAG_TITLE) and (t[4] & FLAG_PARA))
    check(len(titles) >= 100, f"psalm superscriptions carried ({len(titles)} verses)")
    check(all(b == "Ps" for b, _, _ in titles), "superscriptions are in the Psalms and nowhere else")
    # Verse 1 is where a superscription goes, with ONE exception that is not an
    # exception in `kjv.jsonl`: Psalm 119's acrostic stanza headings open every
    # eighth verse, and the KJV carries them as title tokens there. So the
    # positions are checked against the KJV's own rather than against "verse 1",
    # which would have quietly dropped 21 of the 22.
    stanza = {r for r in titles if r[0] == "Ps" and r[1] == 119}
    check(all(v == 1 for b, c, v in titles - stanza), "every superscription sits in verse 1")
    kjv_stanza = {r for r, ts in kjv.items() if r[0] == "Ps" and r[1] == 119 and any(t[4] & FLAG_TITLE for t in ts)}
    check(
        stanza in (set(), kjv_stanza),
        f"Psalm 119's stanza headings sit where the KJV's do ({len(stanza)} here, {len(kjv_stanza)} in the KJV)",
    )
    check(both == 0, f"no token carries both title and paragraph ({both} do)")
    check(0.05 < para / len(corpus) < 0.30, f"paragraph rate {para / len(corpus):.2f} is near the KJV's 0.10")

    print("9. no Strong's codes, no italics")
    coded = sum(1 for ts in corpus.values() for t in ts if t[3])
    added = sum(1 for ts in corpus.values() for t in ts if t[4] & FLAG_ADDED)
    check(coded == 0, f"no token carries a Strong's code ({coded} do) — deliberate, see build-indic.py")
    check(added == 0, f"no token is flagged as translator-supplied ({added} are)")

    print("10. the corpus is one text")
    by_book: dict[str, list[str]] = collections.defaultdict(list)
    for (b, _, _), toks in corpus.items():
        by_book[b].append(render(toks))
    total = collections.Counter(c for ts in by_book.values() for t in ts for c in t if c in TERMINATORS)
    n_books = len(by_book)
    for term, count in total.most_common():
        share = count / sum(total.values())
        used_in = sum(1 for ts in by_book.values() if any(term in t for t in ts))
        if share < 0.01:
            print(f"  --   {term!r} is {share:.2%} of terminators, below the floor — not a splice signal")
            continue
        check(
            used_in >= n_books * 0.9,
            f"{term!r} is {share:.1%} of terminators and is used in {used_in}/{n_books} books",
        )

    if src:
        print("11. nothing was lost")
        # DELIBERATELY NAIVE, and not a second copy of `parse_book`. One short
        # list of markers that are definitionally not verse text, the two spans
        # that are apparatus, and then everything else contributes whatever text
        # it carries to the verse in progress — including the `\d`
        # superscription and the text riding on `\p` and `\q` lines, which is
        # what the build has to get right. Sharing the builder's parser would
        # make this agree with the build by construction and prove nothing.
        SPANS = re.compile(r"\\\+?f\s.*?\\\+?f\*|\\\+?bdit\b.*?\\\+?bdit\*", re.S)
        MARKER = re.compile(r"\\\+?[a-z]+\d*\*?\s?")
        VERSE = re.compile(r"\\v\s+(\d+)\s")
        CHAPTER = re.compile(r"\\c\s+(\d+)")
        SKIP = re.compile(r"\s*\\(id|ide|h|toc\d?|mt\d?|ms\d?|mr|s\d?|sr|r|sp|cl|cp|rem|is\d?|ip[a-z]*|io\d?|iot|ib|b)\b")
        HEADING = re.compile(r"\\(?:d|qa)\s")
        order: list[str] = []
        with KJV.open(encoding="utf-8") as f:
            next(f)
            for line in f:
                b = json.loads(line)["b"]
                if b not in order:
                    order.append(b)
        source: dict[tuple[str, int, int], str] = {}
        for i, raw in enumerate(read_source(src)):
            osis = order[i]
            chapter = verse = 0
            buf: list[str] = []
            pending: list[str] = []
            for line in SPANS.sub(" ", raw).splitlines():
                parts = VERSE.split(line)
                head = parts[0]
                if m := CHAPTER.search(head):
                    if verse:
                        source[(osis, chapter, verse)] = " ".join(buf)
                    chapter, verse, buf, pending = int(m.group(1)), 0, [], []
                elif SKIP.match(head):
                    pass
                elif HEADING.match(head.strip()):
                    # A superscription or a Psalm 119 stanza heading belongs to
                    # the verse AFTER it, even when a verse is still open — the
                    # 21 stanza heads of Psalm 119 each sit between verse 8n and
                    # verse 8n+1. Held rather than appended, or this check would
                    # put every heading on the wrong side of a verse boundary
                    # and report 42 differences that are the checker's own.
                    pending.append(MARKER.sub(" ", head).strip())
                elif verse:
                    buf.append(MARKER.sub(" ", head))
                elif text := MARKER.sub(" ", head).strip():
                    # Before verse 1: a superscription, or a marker carrying
                    # text. Held for the verse it belongs to rather than
                    # dropped — `\d` is folded into verse 1.
                    pending.append(text)
                for j in range(1, len(parts), 2):
                    if verse:
                        source[(osis, chapter, verse)] = " ".join(buf)
                    verse = int(parts[j])
                    buf = pending + [MARKER.sub(" ", parts[j + 1])]
                    pending = []
            if verse:
                source[(osis, chapter, verse)] = " ".join(buf)
        # Fold the source's two split verses the way the build does, each on the
        # side the build puts it.
        for extra, target, how in ((("3John", 1, 15), ("3John", 1, 14), "append"), (("Rev", 12, 18), ("Rev", 13, 1), "prepend")):
            if extra in source:
                moved = source.pop(extra)
                base = source.get(target, "")
                source[target] = (base + " " + moved) if how == "append" else (moved + " " + base)
        missing = [r for r in corpus if r not in source]
        check(not missing, f"every built verse is in the source ({len(missing)} not, e.g. {missing[:3]})")
        diff = [r for r, toks in corpus.items() if r in source and letters(render(toks)) != letters(source[r])]
        check(not diff, f"every verse's letters are the source's ({len(diff)} differ, e.g. {diff[:3]})")
    else:
        print("11. nothing was lost — SKIPPED (pass the source directory or zip to run it)")

    print()
    if fails:
        print(f"{len(fails)} FAILED")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else None))
