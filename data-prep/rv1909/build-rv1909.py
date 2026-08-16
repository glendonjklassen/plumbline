#!/usr/bin/env python3
"""Build data/rv1909.jsonl from the Reina-Valera 1909 USFX edition.

    python3 data-prep/rv1909/build-rv1909.py spa-rv1909.usfx.xml

Source: https://github.com/seven1m/open-bibles (`spa-rv1909.usfx.xml`), the
eBible.org USFX rendering of the Reina-Valera 1909. Public domain — the 1909
revision is long out of copyright, and it is the last Reina-Valera that is: the
1960 most Spanish readers own is held by the Bible societies and cannot ship.

WHY THIS SOURCE, and it is the same reason the Luther build gives:

  - IT SITS AT KJV VERSE ADDRESSES ALREADY. All 66 books in canon order, 1,189
    chapters, 31,102 verses, every per-book chapter and verse count identical to
    `data/kjv.jsonl`. Reina-Valera follows the KJV's chapter and verse breaks
    throughout, so unlike German there is not even a printed-numbering
    difference to annotate — `crates/core/src/i18n.rs` gives Spanish no
    `numbering` row and `check-rv1909.py` proves it is entitled to none.
  - IT IS ALREADY STRONG'S-TAGGED, inline, in the same file as the text
    (`<w s="H7225">EN el principio</w>`). German needed a second edition and an
    alignment pass (`data-prep/luther/merge-strongs.py`) because its text and
    its tags came from different publishers. Here they arrive together, so the
    tags are the source's own claim about its own words and nothing is guessed.
  - IT MARKS TRANSLATOR-SUPPLIED WORDS with `<add>`, which is exactly what the
    KJV's italics are. Those become `FLAG_ADDED`, so the reader's "italicize
    inserted words" setting means the same thing in Spanish as in English.

The output is `kjv.jsonl`'s frozen shape (CLAUDE.md §Data formats): a header
line, then one verse per line with positional tokens
`[pre, word, post, [strongs], flags]`.

Of the flag bits, ADDED (1) and DIVINE NAME (2) are set. Not TITLE (4): this
edition carries no superscription markup. Not PARAGRAPH (8) either — the
source's `<p>` elements are roughly one per chapter rather than real paragraph
divisions, and a pilcrow at every chapter opening would be an invention.
"""

import json
import re
import sys
import unicodedata
from pathlib import Path

TOKENIZATION = "rv1909-tok1"
FORMAT = "overlay-kjv-canonical"
SOURCE = "Reina-Valera 1909 (eBible.org USFX via seven1m/open-bibles; public domain)"

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "data" / "rv1909.jsonl"
STRONGS = ROOT / "data" / "strongs.json"

FLAG_ADDED = 1
FLAG_DIVINE = 2

# Reina-Valera renders the Tetragrammaton as "Jehová" where the KJV sets LORD,
# so the same bit carries the same meaning in both texts and a shell painting
# the divine name never has to know which Bible it is looking at.
#
# The bare word only. "Jehová-jireh" and its kin are place names the KJV does
# not flag either, and flagging them would style an altar's name as the name of
# God.
DIVINE = {"Jehová", "JEHOVÁ"}

# THE HEAD OF A TAGGED PHRASE, and why only it carries the code.
#
# This source tags SPANS, not words: `<w s="H8064">los cielos</w>` puts one code
# on an article and a noun. `data/kjv.jsonl` does the opposite — "In the
# beginning" tags `beginning` alone and leaves `In` and `the` bare — and that
# convention is the one worth matching, for two reasons that are not about
# tidiness:
#
#   - THE RENDERING LISTS. `build-strongs.py` derives each code's Spanish
#     renderings by counting the words that stand under it, so tagging every
#     word in the span put articles in the dictionary: H430 read
#     "Dios, de, tu" and G26 read "amor, caridad, la".
#   - THE CONCORDANCE. `occurrence_count` counts tagged tokens, so a phrase-
#     tagged corpus would report three occurrences where the KJV reports one,
#     and the same word study would give different numbers in two languages.
#
# The head is the last word of the span that is not a function word — Spanish
# puts it there ("de los cielos" → cielos, "estaban sobre" → sobre, "era buena"
# → buena) — falling back to the last word when the span is function words
# alone, which is what a span like "que" or "de" is.
FUNCTION = {
    "el", "la", "los", "las", "lo", "un", "una", "unos", "unas",
    "de", "del", "a", "á", "al", "en", "y", "e", "o", "u", "que",
    "se", "su", "sus", "mi", "tu", "no", "ni", "por", "con",
}

# Punctuation that belongs after a word rather than being a word of its own.
# Spanish opens its questions and exclamations, so ¿ and ¡ are PRE.
POST = ".,;:!?)]»“”\"'…"
PRE = "([«„\"'¿¡"

BOOK = re.compile(r'<book id="([A-Z0-9]{3})">(.*?)</book>', re.S)
MILESTONE = re.compile(r'<c id="(\d+)"\s*/>|<v id="(\d+)"\s*/>|<ve\s*/>')
# A tagged word run, a translator-supplied run, any other tag, or plain text.
PIECE = re.compile(r'<w s="([^"]*)"\s*>(.*?)</w>|<add>(.*?)</add>|<[^>]*>|([^<]+)', re.S)
CODE = re.compile(r"([HG])0*(\d+)$")


def osis_order(kjv: Path) -> list[str]:
    """The 66 OSIS ids in canon order, taken from the KJV corpus itself.

    Read rather than restated, for `build-luther.py`'s reason: this script's
    whole claim is that its output sits at the same addresses as `kjv.jsonl`,
    and a hand-typed list here would be a second source for that fact.
    """
    order: list[str] = []
    with kjv.open() as f:
        next(f)
        for line in f:
            b = json.loads(line)["b"]
            if b not in order:
                order.append(b)
    return order


def codes_of(attr: str, known: set[str], dropped: dict[str, int]) -> tuple[str, ...]:
    """`s="H0430 H3068"` → `("H430", "H3068")`, minus anything no dictionary has.

    The source zero-pads to four digits and `strongs.json` does not, so without
    this every tag in the corpus would be a lookup miss — a word study that
    silently says "not in dictionary" for the whole Bible.

    A code the dictionary genuinely lacks is DROPPED rather than carried: it
    would be a tappable word leading to an empty card, which reads as a broken
    app rather than as a gap in the data. The caller reports the count.
    """
    out = []
    for raw in attr.split():
        m = CODE.fullmatch(raw)
        if not m:
            dropped[raw] = dropped.get(raw, 0) + 1
            continue
        code = f"{m.group(1)}{int(m.group(2))}"
        if code not in known:
            dropped[code] = dropped.get(code, 0) + 1
            continue
        out.append(code)
    return tuple(out)


def attributed(chunk: str, known: set[str], dropped: dict[str, int]) -> list[tuple[str, tuple, int]]:
    """One verse of source XML → per-character (char, codes, flags).

    PER CHARACTER, rather than per element, because the markup and the words do
    not line up: `<w s="H0216">la luz</w>:` is two words and a colon inside one
    element, and `<w …>Y llamó</w> <w …>Dios</w>` puts the space between two
    elements. Attributing characters and tokenizing the flat text afterwards
    means the tokenizer is the same simple thing `build-luther.py` uses, and the
    round-trip property (`pre + word + post` rebuilds the verse) falls out
    instead of being argued for.
    """
    out: list[tuple[str, tuple, int]] = []

    def emit(text: str, codes: tuple, flags: int) -> None:
        for ch in text:
            if ch.isspace():
                # Collapse runs of whitespace as they are appended, so the
                # indices stay honest — a normalizing pass afterwards would
                # break the attribution alongside it.
                if out and out[-1][0] == " ":
                    continue
                out.append((" ", (), 0))
            else:
                out.append((ch, codes, flags))

    def emit_span(text: str, codes: tuple, flags: int) -> None:
        """A tagged run: the code goes on its HEAD word only — see FUNCTION."""
        words = text.split()
        head = None
        for w in reversed(words):
            if w.strip("".join(set(POST) | set(PRE))).lower() not in FUNCTION:
                head = w
                break
        if head is None and words:
            head = words[-1]
        # Re-emitted word by word so the head can be told apart, with the
        # original spacing preserved by splitting on it rather than rejoining.
        rest = text
        for w in words:
            at = rest.index(w)
            emit(rest[:at], (), flags)
            emit(w, codes if w is head else (), flags)
            rest = rest[at + len(w) :]
        emit(rest, (), flags)

    def walk(xml: str, flags: int) -> None:
        for m in PIECE.finditer(xml):
            tagged, word, added, plain = m.group(1), m.group(2), m.group(3), m.group(4)
            if word is not None:
                emit_span(word, codes_of(tagged, known, dropped), flags)
            elif added is not None:
                # `<add>` occasionally wraps tagged words, so recurse rather
                # than treating its contents as plain text.
                walk(added, flags | FLAG_ADDED)
            elif plain is not None:
                emit(plain, (), flags)

    walk(chunk, 0)
    return out


def divine(word: str) -> bool:
    return word in DIVINE


def tokenize(chars: list[tuple[str, tuple, int]]) -> list:
    """Attributed characters → `[pre, word, post, [strongs], flags]` tokens.

    Whitespace-split, then punctuation peeled off each end — `build-luther.py`'s
    algorithm, with the codes and flags read off the characters the word is made
    of. A word's codes are those of its first tagged character: a run like
    "la luz" tags both words with the same code, which is the source's own claim
    about the phrase.
    """
    tokens: list = []
    text = "".join(c[0] for c in chars).strip()
    if not text:
        return tokens
    # Re-walk with positions so each word can look its attribution back up.
    at = 0
    while at < len(chars) and chars[at][0] == " ":
        at += 1
    for raw in text.split(" "):
        span = chars[at : at + len(raw)]
        at += len(raw) + 1
        pre = ""
        post = ""
        word = raw
        first = 0
        while word and word[0] in PRE:
            pre += word[0]
            word = word[1:]
            first += 1
        while word and word[-1] in POST:
            post = word[-1] + post
            word = word[:-1]
        if not word:
            # Punctuation alone. Hang it on the previous token's `post` so
            # nothing is lost and no empty word is emitted.
            if tokens:
                tokens[-1][2] += raw
            continue
        body = span[first : first + len(word)]
        codes = next((c for _, c, _ in body if c), ())
        flags = FLAG_ADDED if any(f & FLAG_ADDED for _, _, f in body) else 0
        if divine(word):
            flags |= FLAG_DIVINE
        tokens.append([pre, word, post, list(codes), flags])
    return tokens


def main(src: str) -> int:
    xml = unicodedata.normalize("NFC", Path(src).read_text(encoding="utf-8"))
    order = osis_order(ROOT / "data" / "kjv.jsonl")
    if len(order) != 66:
        print(f"expected 66 books in kjv.jsonl, found {len(order)}", file=sys.stderr)
        return 1
    known = set(json.loads(STRONGS.read_text(encoding="utf-8")))
    dropped: dict[str, int] = {}

    out_lines = []
    books = list(BOOK.finditer(xml))
    if len(books) != 66:
        print(f"expected 66 books in the source, found {len(books)}", file=sys.stderr)
        return 1

    for i, bm in enumerate(books):
        osis = order[i]
        body = bm.group(2)
        chapter = 0
        verse = 0
        starts_at = None
        marks = list(MILESTONE.finditer(body))
        for j, m in enumerate(marks):
            if starts_at is not None:
                chunk = body[starts_at : m.start()]
                tokens = tokenize(attributed(chunk, known, dropped))
                out_lines.append(
                    json.dumps(
                        {"b": osis, "c": chapter, "v": verse, "t": tokens},
                        ensure_ascii=False,
                        separators=(",", ":"),
                    )
                )
                starts_at = None
            if m.group(1):
                chapter = int(m.group(1))
            elif m.group(2):
                verse = int(m.group(2))
                starts_at = m.end()
        if starts_at is not None:
            chunk = body[starts_at:]
            tokens = tokenize(attributed(chunk, known, dropped))
            out_lines.append(
                json.dumps(
                    {"b": osis, "c": chapter, "v": verse, "t": tokens},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
            )
        _ = j  # the loop index is only meaningful inside

    header = json.dumps(
        {"format": FORMAT, "source": SOURCE, "tokenization": TOKENIZATION, "verses": len(out_lines)},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    OUT.write_text(header + "\n" + "\n".join(out_lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}: {len(out_lines)} verses")
    if dropped:
        total = sum(dropped.values())
        worst = sorted(dropped.items(), key=lambda kv: -kv[1])[:5]
        print(f"dropped {total} tag(s) in {len(dropped)} code(s) no dictionary has: {worst}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
