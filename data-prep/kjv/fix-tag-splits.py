#!/usr/bin/env python3
"""Rejoin KJV words the SWORD import split at a markup boundary.

The eBible `engKJV2006eb` module closes a Strong's `<w>` tag in the middle of
some words (`<w lemma="strong:G4314">agains</w>t`, `<w>who</w><w>soever</w>`),
and the overlay importer treats every tag boundary as a word boundary, so the
corpus carried "agains t", "who soever", "mid night" and 44 more two-piece
words — every one in the New Testament, where the module's tagging is denser.
Eight further readings are spaced in the module itself ("what soever",
"can not") though the 1769 text prints them as one word and the corpus prints
them joined everywhere else; they are listed separately below.

This script merges those pieces into one token (Strong's refs concatenated,
flags OR-ed, the first piece's `pre` and the last piece's `post` kept) and
re-indexes the two sidecars addressed by token index:

  - data/akjv.jsonl   spans `[start, end, text]` — shifted; a span that now
                      matches the KJV text letter for letter is dropped, a
                      verse left with no spans is dropped, and the header's
                      verse count follows
  - data/morphology.jsonl entries `[tok, strongs, homograph, code]` — shifted;
                      the pieces' entries all land on the merged token

Run from the repo root once, after a re-import; it refuses to write when any
listed verse no longer carries its pieces (a corpus already fixed), so a second
run cannot damage anything. Rebuild the packed overlay afterwards:

    cargo run --release -p plumbline-hydrate -- akjvb --from data/akjv.jsonl

Derivation: `grep -nE '</w>[A-Za-z’]|[A-Za-z’]<w |</w><w ' data/raw/kjv.imp`
over the module dump in the overlay checkout, plus a bigram scan of the corpus
for two-piece words whose join is a common corpus word.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KJV = ROOT / "data" / "kjv.jsonl"
AKJV = ROOT / "data" / "akjv.jsonl"
MORPH = ROOT / "data" / "morphology.jsonl"

# (ref, [pieces]) — the pieces are the token words, in order, as the import
# left them. A piece's trailing punctuation (`day’` + `s`) stays inside the word.
TAG_SPLITS = [
    ("Matt 11:6", ["who", "soever"]),
    ("Matt 11:20", ["where", "in"]),
    ("Matt 11:27", ["whom", "soever"]),
    ("Matt 12:4", ["shew", "bread"]),
    ("Matt 12:32", ["who", "soever"]),
    ("Matt 12:50", ["who", "soever"]),
    ("Matt 25:6", ["mid", "night"]),
    ("Matt 26:53", ["can", "not"]),
    ("Mark 2:19", ["can", "not"]),
    ("Mark 4:24", ["give", "n"]),
    ("Mark 7:18", ["can", "not"]),
    ("Mark 10:15", ["there", "in"]),
    ("Luke 7:44", ["tear", "s"]),
    ("Luke 8:25", ["afrai", "d"]),
    ("Luke 11:7", ["can", "not"]),
    ("John 3:3", ["can", "not"]),
    ("John 3:5", ["can", "not"]),
    ("John 10:35", ["can", "not"]),
    ("John 13:20", ["tha", "t"]),
    ("Acts 1:12", ["day", "s"]),
    ("Acts 7:49", ["foot", "stool"]),
    ("Gal 6:8", ["tha", "t"]),
    ("Eph 6:12", ["agains", "t"]),
    ("Phil 1:18", ["there", "in"]),
    ("Phil 2:2", ["like", "minded"]),
    ("Phil 3:16", ["where", "to"]),
    ("Phil 4:10", ["where", "in"]),
    ("Col 2:7", ["there", "in"]),
    ("Col 2:12", ["where", "in"]),
    ("1Thess 1:8", ["God", "-", "ward"]),
    ("1Thess 2:6", ["burden", "some"]),
    ("1Tim 6:4", ["where", "of"]),
    ("1Tim 6:7", ["no", "thing"]),
    ("Heb 3:3", ["in", "asmuch"]),
    ("Heb 4:6", ["there", "in"]),
    ("Heb 4:15", ["can", "not"]),
    ("Heb 4:16", ["bold", "ly"]),
    ("Heb 5:3", ["here", "of"]),
    ("Heb 7:20", ["in", "asmuch"]),
    ("Heb 9:2", ["where", "in"]),
    ("Heb 9:4", ["where", "in"]),
    ("Heb 10:29", ["where", "with"]),
    ("Heb 11:37", ["goat", "skins"]),
    ("Heb 12:11", ["there", "by"]),
    ("Heb 12:15", ["there", "by"]),
    ("Heb 13:2", ["there", "by"]),
    ("Heb 13:10", ["where", "of"]),
]
# Spaced in the module itself; one word in the 1769 text and everywhere else in
# this corpus ("whatsoever" ×150, "cannot" ×170).
SOURCE_SPACED = [
    ("Col 3:17", ["what", "soever"]),
    ("Col 3:23", ["what", "soever"]),
    ("Acts 4:16", ["can", "not"]),
    ("Acts 4:20", ["can", "not"]),
    ("Acts 5:39", ["can", "not"]),
    ("1Tim 5:25", ["can", "not"]),
    ("2Tim 2:13", ["can", "not"]),
    ("Heb 9:5", ["can", "not"]),
]
MERGES = TAG_SPLITS + SOURCE_SPACED


def ref_of(v):
    return f'{v["b"]} {v["c"]}:{v["v"]}'


def find_runs(tokens, pieces):
    """Start indices where `pieces` sit as adjacent tokens with nothing but a
    possessive apostrophe between them."""
    out = []
    n = len(pieces)
    for i in range(len(tokens) - n + 1):
        ok = all(tokens[i + k][1] == pieces[k] for k in range(n))
        ok = ok and all(tokens[i + k][2] in ("", "’") for k in range(n - 1))
        ok = ok and all(tokens[i + k][0] == "" for k in range(1, n))
        if ok:
            out.append(i)
    return out


def merge(tokens, i, n):
    run = tokens[i : i + n]
    word = "".join(t[1] + (t[2] if k < n - 1 else "") for k, t in enumerate(run))
    strongs = [s for t in run for s in t[3]]
    flags = 0
    for t in run:
        flags |= t[4]
    tokens[i : i + n] = [[run[0][0], word, run[-1][2], strongs, flags]]


def main(dry):
    want = {}
    for ref, pieces in MERGES:
        want.setdefault(ref, []).append(pieces)

    # ── the corpus ───────────────────────────────────────────────────────
    # per verse: list of (start, n) in ORIGINAL indices, for re-indexing sidecars
    plan = {}
    out_lines = []
    with KJV.open(encoding="utf-8") as f:
        out_lines.append(f.readline())
        for line in f:
            v = json.loads(line)
            ref = ref_of(v)
            if ref not in want:
                out_lines.append(line)
                continue
            runs = []
            for pieces in want[ref]:
                starts = find_runs(v["t"], pieces)
                if len(starts) != 1:
                    sys.exit(f"{ref}: expected one run of {pieces}, found {starts}")
                runs.append((starts[0], len(pieces)))
            runs.sort()
            for start, n in reversed(runs):
                merge(v["t"], start, n)
            plan[ref] = runs
            print(f'{ref}: ' + "; ".join(
                f'tok{s}..{s+n-1} -> {v["t"][s - sum(m - 1 for st, m in runs if st < s)][1]!r}' for s, n in runs))
            out_lines.append(json.dumps(v, separators=(",", ":"), ensure_ascii=False, sort_keys=True) + "\n")
    if len(plan) != len(want):
        sys.exit(f"merged {len(plan)} verses, expected {len(want)}: missing {set(want) - set(plan)}")

    def remap(ref):
        runs = plan[ref]

        def f(j):
            shift = 0
            for s, n in runs:
                if j >= s + n:
                    shift += n - 1
                elif j > s:
                    return s - shift
            return j - shift
        return f

    # ── akjv spans ───────────────────────────────────────────────────────
    kjv_tokens = {}
    for line in out_lines[1:]:
        v = json.loads(line)
        if ref_of(v) in plan:
            kjv_tokens[ref_of(v)] = v["t"]
    akjv_lines = []
    dropped = 0
    dropped_verses = 0
    with AKJV.open(encoding="utf-8") as f:
        header = json.loads(f.readline())
        for line in f:
            v = json.loads(line)
            ref = ref_of(v)
            if ref not in plan:
                akjv_lines.append(line)
                continue
            f_ = remap(ref)
            spans = []
            for s, e, text in v["d"]:
                s2, e2 = f_(s), f_(e)
                kjv = " ".join(t[1] for t in kjv_tokens[ref][s2 : e2 + 1])
                if kjv == text:
                    dropped += 1
                    continue
                spans.append([s2, e2, text])
            if not spans:
                dropped_verses += 1
                continue
            v["d"] = spans
            akjv_lines.append(json.dumps(v, separators=(",", ":"), ensure_ascii=False) + "\n")
    header["verses"] = len(akjv_lines)
    akjv_lines.insert(0, json.dumps(header, separators=(",", ":"), ensure_ascii=False) + "\n")
    print(f"akjv: dropped {dropped} spans that now match the KJV, {dropped_verses} verses left empty; "
          f"header verses = {header['verses']}")

    # ── morphology entries ───────────────────────────────────────────────
    morph_lines = []
    with MORPH.open(encoding="utf-8") as f:
        morph_lines.append(f.readline())
        for line in f:
            v = json.loads(line)
            ref = ref_of(v)
            if ref not in plan:
                morph_lines.append(line)
                continue
            f_ = remap(ref)
            v["e"] = [[f_(e[0]), *e[1:]] for e in v["e"]]
            morph_lines.append(json.dumps(v, separators=(",", ":"), ensure_ascii=False, sort_keys=True) + "\n")

    if dry:
        print("dry run: nothing written")
        return
    KJV.write_text("".join(out_lines), encoding="utf-8")
    AKJV.write_text("".join(akjv_lines), encoding="utf-8")
    MORPH.write_text("".join(morph_lines), encoding="utf-8")
    print(f"wrote {KJV.name}, {AKJV.name}, {MORPH.name}: {sum(len(r) for r in plan.values())} words in {len(plan)} verses")


if __name__ == "__main__":
    main(dry="--dry-run" in sys.argv)
