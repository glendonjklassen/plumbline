#!/usr/bin/env python3
"""Build data/strongs-de.json — the German Strong's dictionary.

    python3 data-prep/strongs-de/build-strongs-de.py

Combines three inputs, one per field family:

  lemma / xlit / pron   copied from data/strongs.json — language-neutral.
  derivation / strongs_def
                        the machine translations from translations.json
                        (produced by translate.py; see its header for the
                        provenance and the app-side caveat). An untranslated
                        entry falls back to its English text — information is
                        never dropped for want of a translation.
  kjv_def               NOT a translation. In the German dictionary this slot
                        holds the LUTHER RENDERINGS — the German words that
                        actually stand under each Strong's number in the tagged
                        data/luther1912.jsonl, most frequent first. Derived
                        data, not AI output, and more useful to a German reader
                        than any translation of the KJV's renderings would be.
                        (The slot keeps its frozen wire name; the shells label
                        it for the reader's Bible.)

The output file has exactly data/strongs.json's shape, so `load_strongs` reads
either file and everything downstream is language-blind.
"""

import json
import sys
import unicodedata
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STRONGS = ROOT / "data" / "strongs.json"
LUTHER = ROOT / "data" / "luther1912.jsonl"
TRANSLATIONS = Path(__file__).resolve().parent / "translations.json"
OUT = ROOT / "data" / "strongs-de.json"

# A rendering must carry at least this share of a code's occurrences (and at
# least 2 of them) to be listed. This is what keeps alignment noise out: the
# source tagging occasionally lands a code on a neighbouring function word
# ("und" under H430), and those strays sit far below any real rendering's share.
MIN_SHARE = 0.05
MAX_FORMS = 8


def fold(word: str) -> str:
    return unicodedata.normalize("NFC", word).lower()


def luther_renderings() -> dict[str, str]:
    """code → 'Gott, Götter' — surface forms by frequency from the tagged corpus."""
    counts: dict[str, Counter] = defaultdict(Counter)
    display: dict[tuple[str, str], Counter] = defaultdict(Counter)
    with LUTHER.open(encoding="utf-8") as f:
        next(f)
        for line in f:
            for t in json.loads(line)["t"]:
                for code in t[3]:
                    k = fold(t[1])
                    counts[code][k] += 1
                    display[(code, k)][t[1]] += 1
    out = {}
    for code, c in counts.items():
        total = sum(c.values())
        forms = [
            display[(code, k)].most_common(1)[0][0]
            for k, n in c.most_common(MAX_FORMS)
            if n >= 2 and n / total >= MIN_SHARE
        ]
        if forms:
            out[code] = ", ".join(forms)
    return out


def main() -> int:
    if not TRANSLATIONS.exists():
        print("translations.json missing — run translate.py first (needs ANTHROPIC_API_KEY)", file=sys.stderr)
        return 1
    strongs = json.loads(STRONGS.read_text(encoding="utf-8"))
    trans = json.loads(TRANSLATIONS.read_text(encoding="utf-8"))
    renderings = luther_renderings()

    untranslated = 0
    out = {}
    for code, e in strongs.items():
        t = trans.get(code, {})
        if (e.get("strongs_def") and not t.get("strongs_def")) or (e.get("derivation") and not t.get("derivation")):
            untranslated += 1
        de = {}
        for k in ("lemma", "xlit", "pron"):
            if e.get(k):
                de[k] = e[k]
        for k in ("derivation", "strongs_def"):
            if t.get(k):
                de[k] = t[k]
            elif e.get(k):
                de[k] = e[k]
        if code in renderings:
            de["kjv_def"] = renderings[code]
        elif e.get("kjv_def"):
            de["kjv_def"] = e["kjv_def"]
        out[code] = de

    OUT.write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":"), sort_keys=True), encoding="utf-8")
    print(
        f"wrote {OUT.relative_to(ROOT)}: {len(out)} entries, "
        f"{len(renderings)} with Luther renderings, {untranslated} falling back to English prose"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
