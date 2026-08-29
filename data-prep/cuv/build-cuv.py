#!/usr/bin/env python3
"""Build data/cuv1919t.jsonl / cuv1919s.jsonl from the CUV 1919 USFX editions.

    python3 data-prep/cuv/build-cuv.py t chi-cuv.usfx.xml
    python3 data-prep/cuv/build-cuv.py s chi-cuv-simp.usfx.xml

Source: https://github.com/seven1m/open-bibles — `chi-cuv.usfx.xml`
(traditional) and `chi-cuv-simp.usfx.xml` (simplified), the 1919 Chinese Union
Version (和合本). Public domain: 1919 clears the US 95-year term outright. The
Revised CUV (2010) is in copyright and is NOT this text; the About screen
names the 1919 edition on purpose.

WHY THIS TEXT: the CUV NT is the missionary-era text of the TR/KJV line for
this purpose — all twenty TR discriminators carry text (John 7:53 via the
split below), and Acts 8:37 — the gate — is present with the eunuch's
confession. One translation reaches Mandarin and Cantonese readers both; the
two character sets ship as two corpora built by this one script, and the
editions are perfectly parallel (same 31,100 source verses, and every verse's
letter count identical between them — asserted by `check-cuv.py`).

THE SOURCE SITS ALMOST AT KJV ADDRESSES. Both editions differ from the KJV in
the same six places, and the fix is four primitives:

  - SPLIT: the source folds John 7:53 into the head of 8:1 (the printed CUV
    numbers the Pericope's opener). "於是各人都回家去了；" is KJV 7:53 word for
    word and the cut is asserted against the source text.
  - SHIFT: the source prints KJV 1Chr 22:1 as 21:31, pushing chapter 22 up by
    one. Both texts stay in canon order, so the sequential walk lands it
    without a directive.
  - MERGE: 3 John prints 15 verses to the KJV's 14 — the same split verse
    `build-indic.py` merges, appended into verse 14.
  - PLACEHOLDER: the printed CUV combines ranged verses and prints 併於上節
    ("combined with the previous verse") at the second number — the source
    carries 69 such placeholder verses, which ship verbatim: they are what a
    Chinese reader's printed Bible shows at those addresses. At two RANGE
    TAILS the source stops the chapter early instead of emitting the marker
    (Deut 13:18 — its content sits interleaved inside 13:17 — and Ps 116:19,
    inside 116:18); this build constructs the same placeholder there, in the
    edition's own characters, so every KJV address exists and the
    `NumberingSpec` annotation says what the printed Bible calls it. No clean
    textual split exists at either: the CUV reorders the English verses'
    clauses inside the merged verse (condition before consequence), which is
    the same shape as the 1 John 5:6-8 note on the Punjabi row — the words
    under a number differ; nothing can be cut honestly.

TOKENIZATION IS PER CHARACTER, and that is a product decision, not a
shortcut:

  - SEARCH. The query splitter in `crates/core/src/search.rs` breaks a Han
    run into per-character words, so the existing phrase tier — consecutive
    token runs — becomes exact substring search, which is what a Chinese
    reader expects. Dictionary segmentation was rejected: a reader's mental
    word boundaries and a segmenter's disagree (transliterated names worst of
    all), and every disagreement is a search that finds nothing.
  - LAYOUT. Chinese line breaks fall between almost any two characters, with
    the kinsoku prohibitions carried by punctuation. Per-character tokens
    make break opportunities exactly token boundaries — the greedy breaker in
    `crates/layout` needs no intra-token breaking — and gluing punctuation
    into `pre`/`post` IS the kinsoku rule: a closing 。」 can never open a
    line because it is not a token, and an opening 「 can never end one.
  - No Strong's exist for this text (reader-only, Arabic's precedent), so
    multi-character words buy nothing at a word tap.

The divine name 耶和華/耶和华 spans three tokens; all three carry FLAG_DIVINE
so the styling is contiguous. The three compound place names the KJV itself
does not flag (耶和華以勒, 耶和華尼西, 耶和華沙龍/沙龙) are left bare.

Flags: DIVINE (2) only. No ADDED (the CUV's 〔或作…〕 alternate-reading notes
ship inline as the printed page shows them), no TITLE (the CUV prints no
psalm superscriptions — they were not translated), no PARAGRAPH.
"""

import json
import re
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NUMBERING = ROOT / "crates" / "core" / "src" / "versification" / "cuv-numbering.tsv"

FLAG_DIVINE = 2

EDITIONS = {
    "t": {
        "out": "cuv1919t.jsonl",
        "tokenization": "cuv1919t-tok1",
        "source": "和合本 1919 (Chinese Union Version, traditional; via seven1m/open-bibles; public domain)",
        "divine": "耶和華",
        "compounds": ("以勒", "尼西", "沙龍"),
        "placeholder": "併於上節",
        "john753": "於是各人都回家去了；",
    },
    "s": {
        "out": "cuv1919s.jsonl",
        "tokenization": "cuv1919s-tok1",
        "source": "和合本 1919 (Chinese Union Version, simplified; via seven1m/open-bibles; public domain)",
        "divine": "耶和华",
        "compounds": ("以勒", "尼西", "沙龙"),
        "placeholder": "并于上节",
        "john753": "于是各人都回家去了；",
        # THE ONE TRANSFORMATION THIS PIPELINE MAKES TO ANY LETTER, and it is
        # about renderability, not text. The simplified conversion writes two
        # characters as their 2013 规范字 codepoints in CJK Extension B —
        # 𫈟 U+2B21F (茵𫈟, wormwood; 10 occurrences) and 𫗪 U+2B5EA (feed,
        # 1 Cor 3:2; 2 occurrences) — which Source Han Serif and virtually
        # every other font lack: the reader would get tofu in twelve verses,
        # and the engine would measure a glyph the canvas cannot paint. They
        # ship as their traditional forms 蔯/餵 instead — the same words, the
        # forms every font renders, and the convention simplified text uses
        # when a rare simplification is unencodable. Counted exactly, so an
        # upstream change is loud; `check-cuv.py` applies the same two-entry
        # table to the source before its letter-stream comparison and asserts
        # the whole corpus sits in the renderable repertoire.
        "substitutions": {"\U0002B21F": "蔯", "\U0002B5EA": "餵"},
        "substitution_count": 12,
    },
}

# Chinese punctuation peeled into pre/post; everything else is a letter and
# becomes a token of its own.
PRE = "「『（〔［"
POST = "。，、；：！？」』）〕］…"

BOOK = re.compile(r'<book id="([A-Z0-9]{3})">(.*?)</book>', re.S)
MILESTONE = re.compile(r'<c id="(\d+)"\s*/>|<v id="(\d+)"[^/]*/>|<ve\s*/>')
TAG = re.compile(r"<[^>]*>")

# KJV address → how many source verses it consumes (3 John's split verse).
MERGES = {("3John", 1, 14): 2}
# KJV addresses the source has no verse for: the placeholder is constructed.
CONSTRUCTED = [("Deut", 13, 18), ("Ps", 116, 19)]
# The split verse's printed number: the source folds John 7:53 into 8:1, so
# that is the number a reader of this edition finds the text under.
PRINTED_OVERRIDE = {("John", 7, 53): (8, 1)}


def kjv_shape(kjv: Path) -> tuple[list[str], dict]:
    """Canon order and per-chapter last verse, from the KJV corpus itself."""
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


def tokenize(text: str, divine: str, compounds: tuple) -> list:
    """One character, one token; punctuation glued into pre/post."""
    tokens: list = []
    pending_pre = ""
    for ch in text:
        if ch.isspace():
            continue
        if ch in PRE:
            pending_pre += ch
            continue
        if ch in POST:
            if tokens:
                tokens[-1][2] += ch
            else:
                pending_pre += ch  # a verse opening with punctuation
            continue
        tokens.append([pending_pre, ch, "", [], 0])
        pending_pre = ""
    if pending_pre and tokens:
        tokens[-1][2] += pending_pre
    # The divine name: flag each three-character run, skipping the compounds.
    n = len(tokens)
    for i in range(n - 2):
        if "".join(t[1] for t in tokens[i : i + 3]) == divine:
            # A run is a run only with nothing between its characters — a
            # sentence ending on the first character is not the name.
            if tokens[i][2] or tokens[i + 1][2]:
                continue
            tail = "".join(t[1] for t in tokens[i + 3 : i + 5])
            if any(tail.startswith(c) for c in compounds):
                continue
            for t in tokens[i : i + 3]:
                t[4] |= FLAG_DIVINE
    return tokens


def main(edition: str, src: str) -> int:
    ed = EDITIONS[edition]
    xml = Path(src).read_text(encoding="utf-8")
    if unicodedata.normalize("NFC", xml) != xml:
        print("source is not NFC-normal; investigate before building", file=sys.stderr)
        return 1
    if "substitutions" in ed:
        found = sum(xml.count(k) for k in ed["substitutions"])
        if found != ed["substitution_count"]:
            print(f"expected {ed['substitution_count']} substitutable characters, found {found}", file=sys.stderr)
            return 1
        for k, v in ed["substitutions"].items():
            xml = xml.replace(k, v)
    order, shape = kjv_shape(ROOT / "data" / "kjv.jsonl")
    if len(order) != 66:
        print(f"expected 66 books in kjv.jsonl, found {len(order)}", file=sys.stderr)
        return 1

    books = list(BOOK.finditer(xml))
    if len(books) != 66:
        print(f"expected 66 books in the source, found {len(books)}", file=sys.stderr)
        return 1

    # Read every source verse, keyed by the KJV book id at its position.
    verses: dict = {}
    for i, bm in enumerate(books):
        osis = order[i]
        chapter = 0
        verse = 0
        starts_at = None
        for m in MILESTONE.finditer(bm.group(2)):
            if starts_at is not None:
                text = TAG.sub("", bm.group(2)[starts_at : m.start()])
                verses[(osis, chapter, verse)] = "".join(text.split())
                starts_at = None
            if m.group(1):
                chapter = int(m.group(1))
            elif m.group(2):
                verse = int(m.group(2))
                starts_at = m.end()

    # The split: source John 8:1 opens with KJV 7:53.
    j81 = verses[("John", 8, 1)]
    head = ed["john753"]
    if not j81.startswith(head) or ("John", 7, 53) in verses:
        print(f"John 8:1 does not open with the 7:53 text: {j81[:30]!r}", file=sys.stderr)
        return 1
    verses[("John", 7, 53)] = head
    verses[("John", 8, 1)] = j81[len(head) :]

    src_shape: dict = defaultdict(dict)
    for b, c, v in verses:
        src_shape[b][c] = max(v, src_shape[b].get(c, 0))

    out_lines = []
    numbering: list[tuple[str, int, int, str]] = []
    consumed = 0
    constructed = 0

    for book in order:
        queue = [(c, v) for c in sorted(src_shape[book]) for v in range(1, src_shape[book][c] + 1)]
        qi = 0

        def take() -> tuple[tuple[int, int], str]:
            nonlocal qi
            c, v = queue[qi]
            qi += 1
            return (c, v), verses[(book, c, v)]

        for chapter in sorted(shape[book]):
            for verse in range(1, shape[book][chapter] + 1):
                if (book, chapter, verse) in MERGES:
                    parts = []
                    for k in range(MERGES[(book, chapter, verse)]):
                        addr, text = take()
                        if k == 0:
                            printed = addr
                        parts.append(text)
                    text = "".join(parts)
                elif (book, chapter, verse) in CONSTRUCTED:
                    printed, text = (chapter, verse - 1), ed["placeholder"]
                    constructed += 1
                else:
                    printed, text = take()
                printed = PRINTED_OVERRIDE.get((book, chapter, verse), printed)
                if printed != (chapter, verse):
                    numbering.append((book, chapter, verse, f"{printed[0]}:{printed[1]}"))
                tokens = tokenize(text, ed["divine"], ed["compounds"])
                out_lines.append(
                    json.dumps(
                        {"b": book, "c": chapter, "v": verse, "t": tokens},
                        ensure_ascii=False,
                        separators=(",", ":"),
                    )
                )
        if qi != len(queue):
            print(f"{book}: consumed {qi} of {len(queue)} source verses", file=sys.stderr)
            return 1
        consumed += qi

    if len(out_lines) != 31102:
        print(f"expected 31,102 verses out, wrote {len(out_lines)}", file=sys.stderr)
        return 1
    if consumed != len(verses) or constructed != len(CONSTRUCTED):
        print(f"consumed {consumed}/{len(verses)}, constructed {constructed}", file=sys.stderr)
        return 1

    out = ROOT / "data" / ed["out"]
    header = json.dumps(
        {"format": "overlay-kjv-canonical", "source": ed["source"], "tokenization": ed["tokenization"], "verses": len(out_lines)},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    out.write_text(header + "\n" + "\n".join(out_lines) + "\n", encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)}: {len(out_lines)} verses")

    # Both editions derive the SAME table (their structure is identical) —
    # `check-cuv.py` asserts the corpora agree, so the overwrite is idempotent.
    tsv = [
        "# The verse number a printed 和合本 shows, by the KJV address the",
        "# text sits at here. Generated by data-prep/cuv/build-cuv.py; read",
        "# by NumberingSpec (Zht and Zhs — one tradition, one table).",
        "# osis\tchapter\tverse\tprintedRef",
    ]
    tsv += [f"{b}\t{c}\t{v}\t{p}" for b, c, v, p in numbering]
    NUMBERING.write_text("\n".join(tsv) + "\n", encoding="utf-8")
    print(f"wrote {NUMBERING.relative_to(ROOT)}: {len(numbering)} disagreeing addresses")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in EDITIONS:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1], sys.argv[2]))
