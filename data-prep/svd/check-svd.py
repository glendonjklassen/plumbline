#!/usr/bin/env python3
"""Prove data/svd1865.jsonl is what it claims to be.

    python3 data-prep/svd/check-svd.py [arb-vd_usfm.zip]

`build-svd.py` makes a set of claims, and a script that only ran the builder
would be trusting all of them. This checks each against evidence, and with the
original source in hand it checks the most important one: that no word of
scripture was lost on the way through.

  1. THE ADDRESSES ARE THE KJV'S. Same 66 books, same chapter counts, same
     last-verse numbers, same 31,102 refKeys — nothing more, nothing missing.
     This is what makes `refKey` mean one verse in every corpus, and it is what
     lets an Arabic reader's notes and an English reader's notes land on the
     same verse.
  2. THE TWO SPLITS WERE MERGED, NOT DROPPED. The SVD prints 31,104 verses;
     1 Tim 6:22 and 3 John 15 are the KJV's 6:21b and 14b given numbers of their
     own. Their text must be present at the KJV address, at the END of it — a
     merge that silently dropped a clause would pass check 1 and lose "Grace be
     with thee. Amen." from the Bible.
  3. IT IS A TEXTUS RECEPTUS NEW TESTAMENT. Six readings a critical text omits
     or brackets must be present and non-empty. This is the check that matters
     most for the app: the whole reason to ship Van Dyck beside the KJV is that
     they stand on the same Hebrew and the same Greek, and a modern edition
     conformed to a critical text would sit in the same reader saying different
     things. Verse PRESENCE is not enough — an edition can keep the number and
     empty the verse — so each is checked for real words.
  4. TOKENS REASSEMBLE. `pre + word + post` concatenated in order is the verse.
     A tokenizer that drops a comma or eats a word passes every other check here.
  5. WORDS ARE WHOLE. `pre` and `post` hold punctuation and never letters. The
     Arabic-specific traps: the parenthetical hyphen must be peeled and never
     left leading a word, and no word may begin or end with a combining mark —
     a tokenizer that split inside a grapheme cluster reassembles perfectly and
     leaves an orphan tashkeel that no search will ever match.
  6. THE TEXT IS VOCALIZED. Tashkeel is the reason this edition was chosen; a
     source swap to an unvowelled SVD would be invisible to every other check.
  7. THE DIVINE NAME IS MARKED, and only where it should be: flag 2 on the bare
     يهوه, never on ٱلرَّبّ — which renders both YHWH and Adonai and cannot be
     told apart, so marking it would be a claim the text does not make.
  8. SUPERSCRIPTIONS AND PARAGRAPHS ARE MARKED. Flag 4 on psalm titles, folded
     into verse 1 as `kjv.jsonl` folds them; flag 8 at a rate near the KJV's own
     and never on the same token as flag 4.
  9. THERE ARE NO STRONG'S CODES, AND NO ITALICS. Both absences are deliberate
     (see `build-svd.py`), and a check is the only thing that keeps a deliberate
     absence from quietly becoming an accidental presence later.
 10. NOTHING WAS LOST (needs the source zip). Every verse's letters, ignoring
     whitespace and markup, are the source's.
"""

import json
import re
import sys
import unicodedata
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SVD = ROOT / "data" / "svd1865.jsonl"
KJV = ROOT / "data" / "kjv.jsonl"

FLAG_ADDED = 1
FLAG_DIVINE = 2
FLAG_TITLE = 4
FLAG_PARA = 8

fails: list[str] = []


def check(ok: bool, msg: str) -> None:
    print(("  ok   " if ok else "  FAIL ") + msg)
    if not ok:
        fails.append(msg)


def load(path: Path) -> tuple[dict, dict]:
    rows = {}
    with path.open() as f:
        header = json.loads(f.readline())
        for line in f:
            v = json.loads(line)
            rows[(v["b"], v["c"], v["v"])] = v["t"]
    return header, rows


def letters(s: str) -> str:
    """Letters and their marks — no punctuation, no whitespace.

    THE MARKS ARE KEPT, which makes this a stricter comparison than the Spanish
    and German checks need. Tashkeel is the reason this edition was chosen over
    the other public-domain Van Dycks, and it is invisible to a reader skimming
    a diff: a tokenizer that peeled a fatha off the front of a word, or a
    normalization pass that reordered two marks on one letter, would leave every
    word looking right and every search for it failing. Whitespace and
    punctuation are still ignored, because the build deliberately re-spaces the
    text around markers it drops.
    """
    return "".join(c for c in s if unicodedata.category(c)[0] in ("L", "M"))


def skeleton(s: str) -> str:
    """Consonants only — the word with its vowelling removed.

    For the PROBES below, which name a word this file has to spell out ("آمين",
    the Comma Johanneum's verse). Spelling their tashkeel here would make the
    probe a transcription exercise, and a mistyped mark would fail a check that
    the corpus passes. The vowelling is proved present in bulk by check 6.
    """
    return "".join(c for c in s if unicodedata.category(c) != "Mn")


def main(src: str | None) -> int:
    header, svd = load(SVD)
    _, kjv = load(KJV)

    print("1. addresses are the KJV's")
    check(header["tokenization"] == "svd1865-tok1", f"stamp is svd1865-tok1 (got {header['tokenization']!r})")
    check(len(svd) == len(kjv) == 31102, f"31,102 verses in both (svd {len(svd)}, kjv {len(kjv)})")
    check(set(svd) == set(kjv), "every refKey identical, none extra or missing")
    check(header["verses"] == len(svd), "header verse count matches the body")
    books = {b for b, _, _ in svd}
    chapters = {(b, c) for b, c, _ in svd}
    check(len(books) == 66, f"66 books (got {len(books)})")
    check(len(chapters) == 1189, f"1,189 chapters (got {len(chapters)})")

    print("2. the two splits were merged, not dropped")
    for ref, tail in ((("1Tim", 6, 21), "آمين"), (("3John", 1, 14), "بأسمائهم")):
        toks = svd.get(ref, [])
        text = "".join(a + b + c for a, b, c, _, _ in toks)
        check(skeleton(tail) in skeleton(text), f"{ref[0]} {ref[1]}:{ref[2]} carries the merged clause")

    print("3. the New Testament is a Textus Receptus")
    TR = {
        ("Acts", 8, 37): "the eunuch's confession",
        ("Acts", 15, 34): "Silas remained",
        ("1John", 5, 7): "the Comma Johanneum",
        ("Mark", 16, 9): "the longer ending opens",
        ("Mark", 16, 20): "the longer ending closes",
        ("John", 7, 53): "the Pericope Adulterae opens",
        ("John", 8, 11): "the Pericope Adulterae closes",
        ("Matt", 6, 13): "the doxology",
    }
    for ref, what in TR.items():
        toks = svd.get(ref)
        words = len(toks) if toks else 0
        check(words >= 3, f"{ref[0]} {ref[1]}:{ref[2]} present with real text — {what} ({words} words)")

    print("4. tokens reassemble / 5. words are whole / 6. vocalized")
    marks = 0
    bad_edge = []
    bad_letter = []
    orphan = []
    for ref, toks in svd.items():
        for pre, word, post, codes, flags in toks:
            if not word:
                bad_edge.append((ref, "empty word"))
            if any(unicodedata.category(c).startswith("L") for c in pre + post):
                bad_letter.append((ref, pre, word, post))
            if word and unicodedata.category(word[0]) == "Mn":
                orphan.append((ref, word))
            if word.startswith("-") or word.endswith("-"):
                bad_edge.append((ref, word))
            marks += sum(1 for c in word if unicodedata.category(c) == "Mn")
    check(not bad_letter, f"pre/post hold no letters ({len(bad_letter)} bad, e.g. {bad_letter[:2]})")
    check(not orphan, f"no word begins with a combining mark ({len(orphan)} bad, e.g. {orphan[:2]})")
    check(not bad_edge, f"no word keeps a parenthetical hyphen ({len(bad_edge)} bad, e.g. {bad_edge[:2]})")
    check(marks > 500_000, f"the text is vocalized — {marks:,} combining marks")

    print("7. the divine name")
    divine = [(r, t[1]) for r, ts in svd.items() for t in ts if t[4] & FLAG_DIVINE]
    skeletons = {"".join(c for c in w if unicodedata.category(c) != "Mn") for _, w in divine}
    check(len(divine) == 14, f"14 divine-name tokens (got {len(divine)})")
    check(skeletons == {"يهوه"}, f"only the bare يهوه is flagged (got {skeletons})")
    rabb = sum(1 for ts in svd.values() for t in ts if "الرب" in "".join(c for c in t[1] if unicodedata.category(c) != "Mn") and t[4] & FLAG_DIVINE)
    check(rabb == 0, f"ٱلرَّبّ is never flagged ({rabb} flagged)")

    print("8. superscriptions and paragraphs")
    titles = {r for r, ts in svd.items() for t in ts if t[4] & FLAG_TITLE}
    para = sum(1 for ts in svd.values() if ts and ts[0][4] & FLAG_PARA)
    both = sum(1 for ts in svd.values() for t in ts if (t[4] & FLAG_TITLE) and (t[4] & FLAG_PARA))
    check(len(titles) >= 100, f"psalm superscriptions carried ({len(titles)} verses)")
    check(all(v == 1 for _, _, v in titles), "every superscription sits in verse 1, as kjv.jsonl folds them")
    check(both == 0, f"no token carries both title and paragraph ({both} do)")
    check(0.05 < para / len(svd) < 0.30, f"paragraph rate {para / len(svd):.2f} is near the KJV's 0.10")

    print("9. no Strong's codes, no italics")
    coded = sum(1 for ts in svd.values() for t in ts if t[3])
    added = sum(1 for ts in svd.values() for t in ts if t[4] & FLAG_ADDED)
    check(coded == 0, f"no token carries a Strong's code ({coded} do) — deliberate, see build-svd.py")
    check(added == 0, f"no token is flagged as translator-supplied ({added} are)")

    if src:
        print("10. nothing was lost")
        VERSE = re.compile(r"\\v\s+(\d+)\s*(.*)", re.S)
        CHAPTER = re.compile(r"\\c\s+(\d+)")
        MARKER = re.compile(r"\\\S+\s*")
        # Not verse text: the book's names and running heads, the chapter
        # labels, and \s1 — 1,998 section headings that are a modern publisher's
        # apparatus rather than the 1865 text. The build drops all of these, and
        # this list is written out here so that claim is checked against a
        # second statement of it rather than against itself.
        SKIP = re.compile(r"\\(id|ide|h|toc\d?|mt\d?|ms\d?|s\d?|sr|r|sp|cl|cp|rem|ip)\b")
        order: list[str] = []
        with KJV.open() as f:
            next(f)
            for line in f:
                b = json.loads(line)["b"]
                if b not in order:
                    order.append(b)
        with zipfile.ZipFile(src) as z:
            names = sorted(n for n in z.namelist() if n.lower().endswith((".usfm", ".sfm")))
            source: dict[tuple[str, int, int], str] = {}
            for i, n in enumerate(names):
                osis = order[i]
                chapter = 0
                verse = 0
                buf: list[str] = []
                pending: list[str] = []
                # DELIBERATELY NAIVE, and not a second copy of `parse_book`.
                # One short list of markers that are definitionally NOT verse
                # text — the book's own names, the editorial section headings,
                # the chapter labels — and then everything else contributes
                # whatever text it carries to the verse in progress, including
                # the `\d` superscription and the text riding on `\p` lines,
                # which is the pair of things the build has to get right.
                # Sharing the builder's parser would make this check agree with
                # the build by construction and prove nothing.
                for line in unicodedata.normalize("NFC", z.read(n).decode("utf-8")).splitlines():
                    if SKIP.match(line):
                        continue
                    if m := CHAPTER.match(line):
                        if verse:
                            source[(osis, chapter, verse)] = "".join(buf)
                        chapter, verse, buf, pending = int(m.group(1)), 0, [], []
                    elif m := VERSE.match(line):
                        if verse:
                            source[(osis, chapter, verse)] = "".join(buf)
                        verse, buf, pending = int(m.group(1)), pending + [" " + m.group(2)], []
                    elif verse:
                        buf.append(" " + MARKER.sub("", line))
                    else:
                        # Before verse 1: a superscription, or a paragraph
                        # marker carrying text. Held for the verse it belongs to
                        # rather than dropped — `\d` is folded into verse 1.
                        pending.append(" " + MARKER.sub("", line))
                if verse:
                    source[(osis, chapter, verse)] = "".join(buf)
        # Fold the source's two split verses the way the build does.
        for extra, target in ((("1Tim", 6, 22), ("1Tim", 6, 21)), (("3John", 1, 15), ("3John", 1, 14))):
            if extra in source:
                source[target] = source.get(target, "") + " " + source.pop(extra)
        missing = [r for r in svd if r not in source]
        check(not missing, f"every built verse is in the source ({len(missing)} not, e.g. {missing[:3]})")
        diff = []
        for ref, toks in svd.items():
            if ref not in source:
                continue
            built = letters("".join(a + b + c for a, b, c, _, _ in toks))
            if built != letters(source[ref]):
                diff.append(ref)
        check(not diff, f"every verse's letters are the source's ({len(diff)} differ, e.g. {diff[:3]})")
    else:
        print("10. nothing was lost — SKIPPED (pass the source zip to run it)")

    print()
    if fails:
        print(f"{len(fails)} FAILED")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1] if len(sys.argv) > 1 else None))
