#!/usr/bin/env python3
"""Turn the church's devotional booklet (.docx) into a source JSON.

The .docx is the CHURCH'S copy and is deliberately not in the repo
(`.gitignore`: `*.docx`) — this script is what turns it into the checked-in
source of truth under `data-prep/devotional/`, which `scripts/build-devotional.mjs`
then compiles into the shipped `data/devotional.json`. Same division as the
hymnal: a hand-run importer here, a repeatable build step in `scripts/`.

Run rarely — only when the church revises the booklet:

    python3 data-prep/devotional/import-docx.py \
        "data/DEVOTIONAL BOOKLET.docx" data-prep/devotional/new-believer-30.json

Stdlib only, on purpose: pandoc and python-docx are not on this machine and a
once-a-year importer is not worth a dependency. A .docx is a zip of XML, and
all this needs from it is the paragraph text in order.

Every scripture reference is resolved to OSIS ids read out of `crates/core/src/canon.rs`
itself — not a second copy of the 66 names typed here — and then CHECKED
AGAINST `data/kjv.jsonl`: a chapter that doesn't exist or a verse past the end
of one fails the import loudly rather than shipping a page that renders blank.
"""

import json
import re
import sys
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
ROOT = Path(__file__).resolve().parents[2]

# The booklet writes "Psalm 95", the canon calls the book "Psalms". Aliases are
# for that kind of ordinary variance only — anything not resolved here or in
# canon.rs is an error, never a guess.
ALIASES = {"Psalm": "Ps", "Song of Solomon": "Song", "Revelations": "Rev"}

# A plain descriptive label, not a title anyone wrote — see the note at the
# bottom of `main`. Edit it in the built source file, not here.
DEFAULT_NAME = "New Believer Devotional"


def paragraphs(docx: Path) -> list[str]:
    """Every paragraph's text, in document order, runs concatenated."""
    with zipfile.ZipFile(docx) as z:
        body = ET.fromstring(z.read("word/document.xml")).find(W + "body")
    out = []
    for p in body.iter(W + "p"):
        out.append("".join(t.text or "" for t in p.iter(W + "t")).strip())
    return out


def canon_books() -> dict[str, str]:
    """Display name → OSIS id, read from canon.rs so the two cannot drift."""
    src = (ROOT / "crates/core/src/canon.rs").read_text(encoding="utf-8")
    pairs = re.findall(r'Book \{ id: "([^"]+)", imp_name: "[^"]*", name: "([^"]+)" \}', src)
    if len(pairs) != 66:
        sys.exit(f"canon.rs: expected 66 books, matched {len(pairs)} — has the table's shape changed?")
    return {name: bid for bid, name in pairs}


def chapter_lengths() -> dict[tuple[str, int], int]:
    """(book, chapter) → last verse number, from the shipped corpus."""
    last: dict[tuple[str, int], int] = {}
    with (ROOT / "data/kjv.jsonl").open(encoding="utf-8") as fh:
        next(fh)  # the header line
        for line in fh:
            v = json.loads(line)
            key = (v["b"], v["c"])
            last[key] = max(last.get(key, 0), v["v"])
    return last


# "John 3:16–21", "1 Corinthians 15:33", "John 14:15–18, 25–27". The book name
# is non-greedy up to the chapter number so "1 John 1" splits at the right space.
REF = re.compile(r"^(?P<book>(?:[123]\s+)?[A-Za-z][A-Za-z ]*?)\s+(?P<chapter>\d+):(?P<ranges>[\d\s,–—-]+)$")


def parse_scripture(text: str, books: dict[str, str], lengths: dict) -> list[dict]:
    """`Scripture:` line → one entry per verse range, OSIS-keyed and bounds-checked."""
    m = REF.match(text.strip())
    if not m:
        sys.exit(f"unparseable scripture reference: {text!r}")
    name = re.sub(r"\s+", " ", m.group("book")).strip()
    book = books.get(name) or ALIASES.get(name)
    if not book:
        sys.exit(f"unknown book {name!r} in reference {text!r}")
    chapter = int(m.group("chapter"))
    if (book, chapter) not in lengths:
        sys.exit(f"{text!r}: {book} has no chapter {chapter}")
    last = lengths[(book, chapter)]

    out = []
    for part in m.group("ranges").split(","):
        part = part.strip()
        if not part:
            continue
        bits = [b.strip() for b in re.split(r"[–—-]", part) if b.strip()]
        start = int(bits[0])
        end = int(bits[1]) if len(bits) > 1 else None
        if end is not None and end <= start:
            sys.exit(f"{text!r}: range {part!r} does not run forwards")
        if start < 1 or (end or start) > last:
            sys.exit(f"{text!r}: {book} {chapter} ends at verse {last}, range {part!r} runs past it")
        ref = {"book": book, "chapter": chapter, "verse": start}
        if end is not None:
            ref["end"] = end
        out.append(ref)
    if not out:
        sys.exit(f"no verse ranges in reference: {text!r}")
    return out


WEEK = re.compile(r"^Week\s+(\d+)\s*[–—-]\s*(.+)$")
DAY = re.compile(r"^Day\s+(\d+)\s*[–—-]\s*(.+)$")


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    docx, out_path = Path(sys.argv[1]), Path(sys.argv[2])
    books, lengths = canon_books(), chapter_lengths()

    sections: list[dict] = []
    entries: list[dict] = []
    closing: list[str] = []
    # Which part of the booklet we are inside: an entry's reflection, its
    # activity, or the closing note after the last day.
    in_closing = False

    for text in paragraphs(docx):
        if not text:
            continue
        if m := WEEK.match(text):
            # `from`/`to` are DAY numbers, not the week's own number: the app
            # files days under sections, and a "Week 2" heading says nothing
            # about which days it holds. Both are filled by the days that
            # follow — `from` by the first, `to` by each in turn.
            sections.append({"from": 0, "to": 0, "title": m.group(2).strip()})
            continue
        if m := DAY.match(text):
            day = int(m.group(1))
            if sections:
                if not sections[-1]["from"]:
                    sections[-1]["from"] = day
                sections[-1]["to"] = day  # extended by each day it covers
            entries.append({"day": day, "title": m.group(2).strip(), "scripture": [], "reflection": [], "activity": ""})
            continue
        if text.startswith("Scripture:"):
            entries[-1]["scripture"] = parse_scripture(text[len("Scripture:"):], books, lengths)
            continue
        if text.startswith("Activity:"):
            entries[-1]["activity"] = text[len("Activity:"):].strip()
            continue
        if text.lower().startswith("closing note"):
            in_closing = True
            continue
        if in_closing:
            closing.append(text)
            continue
        # Anything else is reflection prose. The booklet's first reflection
        # paragraph carries a "Reflection:" label — sometimes without the space
        # after the colon — which is a heading in Word, not part of the text.
        entries[-1]["reflection"].append(re.sub(r"^Reflection:\s*", "", text))

    for e in entries:
        for field in ("scripture", "reflection"):
            if not e[field]:
                sys.exit(f"day {e['day']} has no {field}")
        if not e["activity"]:
            sys.exit(f"day {e['day']} has no activity")
    days = [e["day"] for e in entries]
    if days != list(range(1, len(days) + 1)):
        sys.exit(f"days are not 1..N without gaps: {days}")
    if not closing:
        sys.exit("no closing note found")

    doc = {
        "id": out_path.stem,
        "days": len(entries),
        # Whether the new-believer welcome starts this booklet automatically.
        # A FLAG IN THE DATA rather than an id hardcoded in a shell, so a second
        # booklet cannot quietly become the one a new believer is handed.
        # Maintainer-set, and preserved across a re-import below.
        "newBeliever": False,
        "sections": sections,
        "entries": [
            {
                "day": e["day"],
                "scripture": e["scripture"],
                "texts": {"en": {"title": e["title"], "reflection": e["reflection"], "activity": e["activity"]}},
            }
            for e in entries
        ],
        # The devotional's own name is NOT in the .docx (its title is the file
        # name). It is maintainer copy, so this is a plain placeholder to be
        # edited in the source file — and a re-import PRESERVES whatever is
        # there (below), so editing it is not undone the next time the church
        # revises the booklet.
        "texts": {"en": {"name": DEFAULT_NAME, "closing": closing}},
    }
    if out_path.exists():
        prior = json.loads(out_path.read_text(encoding="utf-8"))
        kept = prior.get("texts", {}).get("en", {}).get("name", "")
        if kept and kept != DEFAULT_NAME:
            doc["texts"]["en"]["name"] = kept
        doc["newBeliever"] = bool(prior.get("newBeliever", False))
    out_path.write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{out_path}: {len(entries)} days, {len(sections)} sections, {len(closing)} closing paragraphs")


if __name__ == "__main__":
    main()
