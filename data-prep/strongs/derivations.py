#!/usr/bin/env python3
"""Strong's derivation families, decided offline and shipped as data.

    python3 data-prep/strongs/derivations.py            # writes data/derivation-families.json
    python3 data-prep/strongs/derivations.py --report   # prints the classification and the residue

The word study's "Same root" row used to be built at runtime by scanning each
entry's derivation prose for Strong's numbers. That read "(with G3588)" in
θεός — a usage note, ὁ θεός — as descent from the article, so θεός and six
relatives sat under ὁ; it followed "(or perhaps a form of the article)" on ὅς
and pulled the pronoun system in behind them; and on a translated dictionary
it ran over machine-translated prose (maintainer, 2026-10-24: "no false
information"). This script reads the ENGLISH prose once, with a grammar that
is explicit about what counts, writes the result as a file the engine loads,
and leaves everything it cannot classify to a human: the overrides file beside
it is the record of those decisions.

What counts as a parent — an EXPLICIT, UNHEDGED statement of descent or
identity in the entry's first derivation clause:

  from X · feminine/neuter/plural/dual of X · a form/variation of X · for X
  (a variant) · the same as X · from the same as / the base of / a derivative
  of X · participle/case/comparative/diminutive of X · patronymic/patrial/
  denominative/intensive/ordinal/adverb from X · prolonged/contracted/
  strengthened/reduplicated from X · another form for X · corresponding to X
  (an Aramaic word's Hebrew cognate) · from X and Y (a compound: every part)

What does not:

  hedged   — probably, perhaps, apparently, presumed, supposed, possibly,
             as if, seemingly, doubtful, or an "X or Y" alternative
  related  — akin to, compare, see, identical with (a kinship or a
             cross-reference, not descent), and ANY parenthetical that
             carries a code: "(with G3588)", "(in the sense of G1983)",
             "(or perhaps of Hebrew origin (H03196))", "(as in H1625)"
  foreign  — of Hebrew/Chaldee/Latin/Persian/foreign origin (the bridge tier
             owns cross-language descent)
  terminal — a primitive root, a primary word/verb/particle, of uncertain
             derivation/affinity/origin, from an unused root

Hedged and related claims are recorded in the report, not the file: the
family builder in crates/core/src/strongs.rs follows the file alone, so what
the reader sees under "Same root" is what Strong's states, nothing it guesses.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STRONGS = ROOT / "data" / "strongs.json"
OUT = ROOT / "data" / "derivation-families.json"
OVERRIDES = Path(__file__).resolve().parent / "derivation-overrides.json"

FORMAT = "plumbline-strongs-derivations-v1"

CODE = re.compile(r"\b([GH])0*(\d+)\b")

HEDGE = re.compile(
    r"\b(probably|prob|perhaps|apparently|appar|apparent|presum\w*|suppos\w*|assum\w*|possibly|seemingly|"
    r"doubtful\w*|conjectur\w*|as if|rather|uncertain|some think|some suppose|imaginary|facitious|fictitious)\b",
    re.I,
)
RELATED = re.compile(
    r"\b(akin|compare|cf\.|see|identical|equivalent|equiv\.?|formed like|like|allied|cognate|mediating|between|"
    r"imitation|distinction|analogy|similar)\b",
    re.I,
)
# What follows one of these is a second thought, a usage note or a gloss, never
# the parent: "passive participle of H2142, but used for H2145".
CUT = re.compile(
    r"\b(but|used for|used only|in distinction from|in imitation of|in the sense|in its original sense|including|"
    r"through the idea|whence|hence|which is|that is used|meaning|i\.e\. [a-z]|like|is|and equiv\w*)\b",
    re.I,
)
# Editorial residue the source left beside a lemma: "H2673 (חָצָה)lemma חָעָה ayin,
# corrected to חָצָה", "H1973 (הָלְאָה)xlit hâlᵉâh corrected to hâlᵉʼâh".
NOISE = re.compile(r"(lemma|xlit)\s+\S+(\s+\S+){0,4}?,?\s+corrected to\s+\S+", re.I)
# "the equivalent of X" / "a cognate of X" as the OBJECT of a descent verb is
# descent by another name (Strong's hedging the form, not the parentage);
# "equivalent to X" / "identical with X" as a predicate stays a relation.
SAME = re.compile(r"\b(the|an?)\s+(equivalent|equiv\.?|cognate)\s+(of|to)\b|\bidentical\s+with\b", re.I)
# A compound with a part the dictionary cannot name: "and (to stretch)", "and
# teino (to stretch)", "and ;" (the source lost the second code), or a prefix.
PARTIAL = re.compile(
    r"\band\s+(?:[A-Za-z\-]+\s+){0,3}\(|\)\s*and\b|\band\s*;?\s*$|\bprefix\b.*\band\b|^[^A-Za-z]*\S+\s+and\b", re.I
)
BARE = re.compile(r"(?<![\w:])(\d{1,4})(?![\w:])")
FOREIGN = re.compile(r"\b(hebrew|chaldee|aramaic|latin|persian|egyptian|foreign|syriac|greek)\s+(origin|derivation)\b", re.I)
TERMINAL = re.compile(
    r"(\bprimitive root\b|\bprimary (word|verb|particle|preposition|adverb|pronoun|interjection|conjunction|"
    r"article|root)\b|\bunused root\b|\buncertain (derivation|affinity|origin|etymology)\b|\bof uncertain\b)",
    re.I,
)

# The words that may stand between the clause's start and its first code —
# the vocabulary of descent, inflection and spelling variation. A head made of
# anything else is residue, for a human.
ALLOWED = set(
    """
    a an the its also properly rarely usually only now from of for and with by to as in or than
    feminine masculine neuter plural dual singular active passive present perfect future imperfect aorist
    optative past middle voice transitive intransitive oblique cases case irregular accusative dative
    genitive nominative vocative adverb adverbial adverbially adjective participle participial
    patronymic patronymical patronymically patron patrial denominative denominatively intensive intensively
    ordinal multiple multiplicative numeral diminutive comparative superlative reflexive collateral
    collaterally collective shortened shorter short fuller stronger simpler strengthened prolonged
    prolongation contracted contraction contr reduplicated reduplication redupl orthographical variation
    variant another form forms word noun verb particle root base derivative compound compounded alternate
    same first second third part parts corresponding corresp correspond transposition transp permutation
    transmutation lengthened elongated emphatic primary formed derived derivation origin hebrew causative
    hiphil niphal piel hithpael hophal pual hifil interjection preposition prepositional pronoun conjunction
    indicative imperative subjunctive infinitive person tense mood number gender masc fem neut plur sing
    abbreviated abbreviation apocopated apocopation aspirated transliteration augmented softened hardened
    inflected inflection modification modified frequentative frequentatively elative dialectal dialectically
    i e 1st 2nd 3rd tenth decade ultimately remote abridged directive borrowed constructive construction
    abstractly erroneous transcription clerical error provincial phrase combining means unused that one
    quantitative degree respectively certain title article absolute expanded act continued both rel prop
    causatively briefer otherwise all name
    """.split()
)


def descent_head(head: str) -> bool:
    # A variant spelling in Hebrew or Greek script ("or יְרוּשָׁה feminine passive
    # participle of") is not prose: tokens without a Latin letter are passed over.
    toks = [t for t in re.sub(r"[,.:'\"]", " ", head).lower().split() if re.search(r"[a-z]", t)]
    # "with H376 inserted" is a note about a form, never a parentage.
    if toks[:1] == ["with"]:
        return False
    return all(t in ALLOWED for t in toks)


def norm_codes(s: str) -> str:
    return CODE.sub(lambda m: f"{m.group(1)}{m.group(2)}", s)


def strip_lemma_parens(s: str) -> str:
    """`G2316 (θεός)` → `G2316 `: a parenthetical right after a code holding no
    Latin letters is the lemma, not prose. The space matters: the source
    sometimes glues an editorial note to the closing paren ("(אִי)lemma אי …"),
    and "H336lemma" is no code."""
    return re.sub(
        r"([GH]\d+)\s*\(([^()]*)\)", lambda m: m.group(1) + " " if not re.search(r"[A-Za-z]", m.group(2)) else m.group(0), s
    )


OBJECT = re.compile(r"\b(from|of|and|for)\s*$", re.I)
OBJECT_START = re.compile(r"^\s*(an?\s|the\s|compound|derivative|probably|perhaps|apparently|which|same)", re.I)


def split_clauses(s: str):
    """Split on `;` outside parentheses: "(Job 20:22; Job 36:18), from H5606"
    is one clause."""
    out, depth, cur = [], 0, []
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth = max(0, depth - 1)
        if ch == ";" and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append("".join(cur))
    return out


def drop_parens(s: str):
    """Remove every parenthetical (nested too) — EXCEPT one that is the object of
    a descent verb and reads as a derivation itself: "superlative of (a
    derivative of the same as G206)" keeps its content; "(with G3588)", "(in
    the sense of G1983)" and "(compare G1377)" go. Returns the text and the codes
    the dropped parentheticals carried — relations, never parents."""
    out, depth, inner, carried = [], 0, [], []
    for ch in s:
        if ch == "(":
            if depth == 0:
                inner = []
            depth += 1
            continue
        if ch == ")":
            depth = max(0, depth - 1)
            if depth == 0:
                text = "".join(inner)
                if OBJECT.search("".join(out)) and OBJECT_START.match(text) and CODE.search(text):
                    out.append(" " + text + " ")
                else:
                    carried += CODE.findall(text)
                inner = []
            continue
        (inner if depth else out).append(ch)
    if depth:
        # Unbalanced: the source forgot a `)`. What was opened is a gloss.
        carried += CODE.findall("".join(inner))
    return "".join(out), [f"{l}{n}" for l, n in carried]


def first_clause(s: str) -> str:
    """The derivation clause: the first `;`-separated clause that says anything
    about origin. Leading clauses that only spell a variant ("or כָּמוֹ;",
    "(Aramaic) or בְּנָה;") are passed over."""
    for clause in split_clauses(s):
        c = clause.strip()
        if not c:
            continue
        outside, _ = drop_parens(c)
        if CODE.search(outside) or TERMINAL.search(c) or FOREIGN.search(c) or re.search(r"\b(origin|derivation|root)\b", c, re.I):
            return c
    return ""


def classify(code: str, deriv: str):
    """→ (kind, parents, note). kind ∈ from | compound | hedged | related |
    foreign | terminal | none | residue."""
    lang = code[0]
    s = strip_lemma_parens(norm_codes(deriv))
    clause = first_clause(s)
    if not clause:
        return "none", [], ""
    clause = NOISE.sub(" ", clause)
    partial = PARTIAL.search(clause) is not None
    text, in_parens = drop_parens(clause)
    text = SAME.sub("the same as", text)
    text = re.split(r"\bcompare\b", text, flags=re.I)[0]
    text = CUT.split(text, maxsplit=1)[0].strip(" ,.")
    # A bare number in the entry's own language ("a compound of 4012 and G1510"):
    # the source drops the letter now and then. Only after the parentheticals are
    # gone, so a verse reference never qualifies.
    text = BARE.sub(lambda m: f"{lang}{int(m.group(1))}", text)
    codes = [f"{l}{n}" for l, n in CODE.findall(text) if l == lang]
    codes = [c for c in codes if c != code]
    # dedupe, keep order
    seen = []
    for c in codes:
        if c not in seen:
            seen.append(c)
    codes = seen
    if not codes:
        if FOREIGN.search(clause):
            return "foreign", [], clause
        if TERMINAL.search(clause):
            return "terminal", [], clause
        return "none", [], clause
    if HEDGE.search(text):
        return "hedged", codes, text
    if RELATED.search(text):
        return "related", codes, text
    if re.search(r"\b(or)\b", text, re.I) and len(codes) > 1:
        return "hedged", codes, text
    if partial and len(codes) == 1:
        # A compound with a part the dictionary cannot name ("from an obsolete
        # particle and G2031"): its own root, the way a compound of two unrelated
        # parts is. Filing it under the one known part would be the "devil
        # under 'to throw'" mistake by another route.
        return "compound-partial", codes, text
    head = text[: CODE.search(text).start()]
    if not descent_head(head):
        return "residue", codes, text
    return ("compound" if len(codes) > 1 else "from"), codes, text


def main(report: bool) -> int:
    dict_ = json.loads(STRONGS.read_text(encoding="utf-8"))
    overrides = json.loads(OVERRIDES.read_text(encoding="utf-8")) if OVERRIDES.exists() else {}
    counts: dict[str, int] = {}
    entries: dict[str, list[str]] = {}
    residue = []
    for code, e in dict_.items():
        deriv = e.get("derivation") or ""
        kind, parents, text = classify(code, deriv)
        if code in overrides:
            o = overrides[code]
            kind = "override"
            parents = [p for p in o["from"] if p in dict_]
        counts[kind] = counts.get(kind, 0) + 1
        if kind == "residue":
            residue.append((code, text, parents, deriv))
        parents = [p for p in parents if p in dict_]
        if kind in ("from", "compound", "override") and parents:
            entries[code] = parents
    if report:
        for k, n in sorted(counts.items(), key=lambda kv: -kv[1]):
            print(f"{n:6} {k}")
        print(f"\n{len(residue)} residue:")
        for code, text, parents, deriv in residue:
            print(f"  {code}: {deriv[:150]}")
        if "--debug" in sys.argv:
            for code in sys.argv[sys.argv.index("--debug") + 1 :]:
                print(code, classify(code, dict_[code].get("derivation") or ""))
        if "--all" in sys.argv:
            import random

            random.seed(1)
            for kind in ("related", "none-with-code"):
                rows = []
                for code, e in dict_.items():
                    deriv = e.get("derivation") or ""
                    k, parents, text = classify(code, deriv)
                    if kind == "none-with-code":
                        if k == "none" and CODE.search(deriv):
                            rows.append((code, parents, deriv))
                    elif k == kind:
                        rows.append((code, parents, deriv))
                if kind in ("from", "compound", "hedged"):
                    rows = random.sample(rows, min(40, len(rows)))
                print(f"\n── {kind}: {len(rows)} shown")
                for code, parents, deriv in rows:
                    print(f"  {code} -> {parents}: {deriv[:140]}")
        return 0
    OUT.write_text(
        json.dumps(
            {
                "format": FORMAT,
                "source": "data/strongs.json derivation prose, read by data-prep/strongs/derivations.py: explicit, "
                "unhedged descent only; hand decisions in data-prep/strongs/derivation-overrides.json",
                "entries": dict(sorted(entries.items())),
            },
            ensure_ascii=False,
            indent=0,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"wrote {len(entries)} entries with explicit parents; " + ", ".join(f"{k} {n}" for k, n in sorted(counts.items())))
    return 0


if __name__ == "__main__":
    raise SystemExit(main("--report" in sys.argv[1:]))
