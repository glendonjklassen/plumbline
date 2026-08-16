#!/usr/bin/env python3
"""Build a language's Strong's dictionary — data/strongs-<code>.json.

    python3 data-prep/strongs-lang/build-strongs.py de
    python3 data-prep/strongs-lang/build-strongs.py es

Combines three inputs, one per field family:

  lemma / xlit / pron   copied from data/strongs.json — language-neutral.
  derivation / strongs_def
                        the machine translations from translations.<code>.json
                        (produced by translate.py; see its header for the
                        provenance and the app-side caveat). An untranslated
                        entry falls back to its English text — information is
                        never dropped for want of a translation.
  kjv_def               NOT a translation. In a localized dictionary this slot
                        holds THAT LANGUAGE'S OWN RENDERINGS — the words that
                        actually stand under each Strong's number in its tagged
                        corpus, most frequent first: Luther's for German,
                        Reina-Valera's for Spanish. Derived data, not AI output,
                        and more useful to a reader than any translation of the
                        KJV's renderings would be. (The slot keeps its frozen
                        wire name; the shells label it for the reader's Bible.)

The output has exactly data/strongs.json's shape, so `load_strongs` reads any of
them and everything downstream is language-blind.

WHICH CORPUS AND WHICH OUTPUT FILE come from the language registry
(`crates/core/src/i18n.rs`), read through `plumbline-hydrate languages`. This
script used to be `build-strongs-de.py` with `LUTHER` and `OUT` as constants,
which is one more place a second language had to be remembered.

TRANSLATED DEFINITIONS ARE A CLAIM THE APP MAKES ON SCREEN. The study card says
the definitions are machine-translated, so a dictionary whose definitions are
still English must not claim it — that is `machine_translated` in the language's
registry row, and this script prints what it should be set to.
"""

import json
import subprocess
import sys
import unicodedata
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STRONGS = ROOT / "data" / "strongs.json"
HERE = Path(__file__).resolve().parent

# A rendering must carry at least this share of a code's occurrences (and at
# least 2 of them) to be listed. This is what keeps tagging noise out: a source
# occasionally lands a code on a neighbouring function word ("und" under H430),
# and those strays sit far below any real rendering's share.
MIN_SHARE = 0.05
MAX_FORMS = 8
# Below this share of entries carrying a translated definition, the dictionary
# is not a translated one and must not be described as one.
TRANSLATED_FLOOR = 0.5


def registry() -> list[dict]:
    """The language rows, from the core — see the module docstring."""
    out = subprocess.run(
        ["cargo", "run", "--release", "--locked", "-q", "-p", "plumbline-hydrate", "--", "languages"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(out.stdout)["languages"]


def fold(word: str) -> str:
    return unicodedata.normalize("NFC", word).lower()


def renderings_of(corpus: Path) -> dict[str, str]:
    """code → 'Gott, Götter' — surface forms by frequency from the tagged corpus."""
    counts: dict[str, Counter] = defaultdict(Counter)
    display: dict[tuple[str, str], Counter] = defaultdict(Counter)
    with corpus.open(encoding="utf-8") as f:
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


def main(code: str) -> int:
    row = next((l for l in registry() if l["code"] == code), None)
    if row is None:
        print(f"{code} is not a language in the registry (crates/core/src/i18n.rs)", file=sys.stderr)
        return 2
    if not row["lexicon"] or not row["corpus"]:
        print(f"{code}'s registry row names no lexicon or no corpus", file=sys.stderr)
        return 2
    corpus = ROOT / "data" / row["corpus"]
    out_path = ROOT / "data" / row["lexicon"]
    translations = HERE / f"translations.{code}.json"
    if not corpus.exists():
        print(f"{corpus.relative_to(ROOT)} does not exist — build the corpus first", file=sys.stderr)
        return 1

    strongs = json.loads(STRONGS.read_text(encoding="utf-8"))
    # A missing translation file is not an error: the renderings alone are worth
    # shipping, and they are the half that cannot be machine-made.
    trans = json.loads(translations.read_text(encoding="utf-8")) if translations.exists() else {}
    renderings = renderings_of(corpus)

    untranslated = 0
    translatable = 0
    out = {}
    for scode, e in strongs.items():
        t = trans.get(scode, {})
        if e.get("strongs_def") or e.get("derivation"):
            translatable += 1
            if (e.get("strongs_def") and not t.get("strongs_def")) or (e.get("derivation") and not t.get("derivation")):
                untranslated += 1
        local = {}
        for k in ("lemma", "xlit", "pron"):
            if e.get(k):
                local[k] = e[k]
        for k in ("derivation", "strongs_def"):
            if t.get(k):
                local[k] = t[k]
            elif e.get(k):
                local[k] = e[k]
        if scode in renderings:
            local["kjv_def"] = renderings[scode]
        elif e.get("kjv_def"):
            local["kjv_def"] = e["kjv_def"]
        out[scode] = local

    out_path.write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":"), sort_keys=True), encoding="utf-8")
    done = (translatable - untranslated) / translatable if translatable else 0
    print(
        f"wrote {out_path.relative_to(ROOT)}: {len(out)} entries, "
        f"{len(renderings)} with {row['label']} renderings, {untranslated} falling back to English prose"
    )
    print(
        f"  registry: set `machine_translated: {str(done >= TRANSLATED_FLOOR).lower()}` "
        f"on {code}'s lexicon row ({done:.0%} of definitions translated)"
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
