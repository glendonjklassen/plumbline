//! The morphology layer: per-token parsing codes as a second annotation axis
//! beside Strong's numbers. Hebrew/Aramaic comes from OSHB (a tagged Westminster
//! Leningrad Codex — the Masoretic text); Greek from Robinson's parsed Textus
//! Receptus. Both were projected offline onto the KJV tokens into
//! `data/morphology.jsonl` (keyed by verse + token index, the same frozen
//! addressing threads and weaves use).
//!
//! Ported from overlay `Morph.hs` — the **consuming** side only (data model,
//! the OSHM + Robinson code parsers, the study-panel renderer, and the sidecar
//! loader). The offline projection pipeline that *builds* the sidecar stays in
//! Python; this crate never generates it, only reads it. A stale tokenization
//! stamp is refused at load, like the concept cache; a missing file is a silent
//! `None` (the layer is optional).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use plumbline_core::reference::VRef;

/// The language of a parsing code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphLang {
    Hebrew,
    Aramaic,
    Greek,
}

/// One parsing code, structured. Fields hold canonical lowercase names
/// (`"qal"`, `"wayyiqtol"`, `"aorist"`, `"proper name"`); [`render_morph`] turns
/// them into the study-panel phrase. `raw` keeps the source code verbatim — the
/// parse is derived, the code is the datum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Morph {
    pub lang: MorphLang,
    pub pos: String,
    pub ty: Option<String>,
    pub stem: Option<String>,
    pub conj: Option<String>,
    pub voice: Option<String>,
    pub mood: Option<String>,
    pub person: Option<String>,
    pub case_: Option<String>,
    pub gender: Option<String>,
    pub number: Option<String>,
    pub state: Option<String>,
    pub suffixes: Vec<String>,
    pub raw: String,
}

impl Morph {
    fn empty(lang: MorphLang, pos: &str, raw: &str) -> Morph {
        Morph {
            lang,
            pos: pos.to_string(),
            ty: None,
            stem: None,
            conj: None,
            voice: None,
            mood: None,
            person: None,
            case_: None,
            gender: None,
            number: None,
            state: None,
            suffixes: Vec::new(),
            raw: raw.to_string(),
        }
    }
}

// ── code tables (verbatim from OSHB HebrewMorphologyCodes + Robinson docs) ─────

type Table = &'static [(char, &'static str)];

fn hebrew_stems() -> Table {
    &[
        ('q', "qal"),
        ('N', "niphal"),
        ('p', "piel"),
        ('P', "pual"),
        ('h', "hiphil"),
        ('H', "hophal"),
        ('t', "hithpael"),
        ('o', "polel"),
        ('O', "polal"),
        ('r', "hithpolel"),
        ('m', "poel"),
        ('M', "poal"),
        ('k', "palel"),
        ('K', "pulal"),
        ('Q', "qal passive"),
        ('l', "pilpel"),
        ('L', "polpal"),
        ('f', "hithpalpel"),
        ('D', "nithpael"),
        ('j', "pealal"),
        ('i', "pilel"),
        ('u', "hothpaal"),
        ('c', "tiphil"),
        ('v', "hishtaphel"),
        ('w', "nithpalel"),
        ('y', "nithpoel"),
        ('z', "hithpoel"),
    ]
}
fn aramaic_stems() -> Table {
    &[
        ('q', "peal"),
        ('Q', "peil"),
        ('u', "hithpeel"),
        ('p', "pael"),
        ('P', "ithpaal"),
        ('M', "hithpaal"),
        ('a', "aphel"),
        ('h', "haphel"),
        ('s', "saphel"),
        ('e', "shaphel"),
        ('H', "hophal"),
        ('i', "ithpeel"),
        ('t', "hishtaphel"),
        ('v', "ishtaphel"),
        ('w', "hithaphel"),
        ('o', "polel"),
        ('z', "ithpoel"),
        ('r', "hithpolel"),
        ('f', "hithpalpel"),
        ('b', "hephal"),
        ('c', "tiphel"),
        ('m', "poel"),
        ('l', "palpel"),
        ('L', "ithpalpel"),
        ('O', "ithpolel"),
        ('G', "ittaphal"),
    ]
}
fn stems_of(lang: MorphLang) -> Table {
    match lang {
        MorphLang::Aramaic => aramaic_stems(),
        _ => hebrew_stems(),
    }
}
const CONJUGATIONS: Table = &[
    ('p', "perfect"),
    ('q', "weqatal"),
    ('i', "imperfect"),
    ('w', "wayyiqtol"),
    ('h', "cohortative"),
    ('j', "jussive"),
    ('v', "imperative"),
    ('r', "active participle"),
    ('s', "passive participle"),
    ('a', "infinitive absolute"),
    ('c', "infinitive construct"),
];
const PERSONS: Table = &[('1', "1st"), ('2', "2nd"), ('3', "3rd")];
const GENDERS: Table = &[('b', "both"), ('c', "common"), ('f', "feminine"), ('m', "masculine")];
const NUMBERS: Table = &[('d', "dual"), ('p', "plural"), ('s', "singular")];
const STATES: Table = &[('a', "absolute"), ('c', "construct"), ('d', "determined")];
const NOUN_TYPES: Table = &[('c', "common"), ('g', "gentilic"), ('p', "proper name")];
const ADJ_TYPES: Table = &[('a', "adjective"), ('c', "cardinal number"), ('g', "gentilic"), ('o', "ordinal number")];
const PRONOUN_TYPES: Table =
    &[('d', "demonstrative"), ('f', "indefinite"), ('i', "interrogative"), ('p', "personal"), ('r', "relative")];
const SUFFIX_TYPES: Table =
    &[('d', "directional he"), ('h', "paragogic he"), ('n', "paragogic nun"), ('p', "pronominal")];
const PARTICLE_TYPES: Table = &[
    ('a', "affirmation"),
    ('d', "definite article"),
    ('e', "exhortation"),
    ('i', "interrogative"),
    ('j', "interjection"),
    ('m', "demonstrative"),
    ('n', "negative"),
    ('o', "direct object marker"),
    ('r', "relative"),
];

fn look(table: Table, c: char) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == c).map(|(_, v)| *v)
}

/// `'x'` is the scheme's placeholder for unknown/unnecessary values → `None`.
fn code(table: Table, what: &str, c: char, raw: &str) -> Result<Option<String>, String> {
    if c == 'x' {
        Ok(None)
    } else if let Some(name) = look(table, c) {
        Ok(Some(name.to_string()))
    } else {
        Err(format!("unknown {what} '{c}' in {raw}"))
    }
}

// ── OSHM (Hebrew / Aramaic) parser ─────────────────────────────────────────────

/// Parse a full OSHM code (`"HVqp3ms"`, `"HNcmsc/Sp2ms"`, `"ANcmsd/Td"`):
/// language letter, content segment, then any suffix segments joined by `/`.
pub fn parse_morph(raw: &str) -> Result<Morph, String> {
    let mut chars = raw.chars();
    let (lang, rest) = match chars.next() {
        Some('H') => (MorphLang::Hebrew, chars.as_str()),
        Some('A') => (MorphLang::Aramaic, chars.as_str()),
        _ => return Err(format!("morph code without language prefix: {raw}")),
    };
    let mut segs = rest.split('/');
    let content = segs.next().ok_or_else(|| format!("empty morph code: {raw}"))?;
    let mut m = parse_seg(lang, raw, content)?;
    for suf in segs {
        let parsed = parse_seg(lang, raw, suf)?;
        m.suffixes.push(render_as_suffix(&parsed));
    }
    Ok(m)
}

fn parse_seg(lang: MorphLang, raw: &str, seg: &str) -> Result<Morph, String> {
    let cs: Vec<char> = seg.chars().collect();
    match cs.as_slice() {
        ['V', stem, conj, rest @ ..] => {
            let st = code(stems_of(lang), "verb stem", *stem, raw)?;
            let cj = code(CONJUGATIONS, "verb conjugation", *conj, raw)?;
            let mut base = Morph::empty(lang, "verb", raw);
            base.stem = st;
            base.conj = cj;
            match rest {
                [p, more @ ..] if p.is_ascii_digit() => {
                    base.person = code(PERSONS, "person", *p, raw)?;
                    let (g, n, _) = gns(more, raw)?;
                    base.gender = g;
                    base.number = n;
                }
                more => {
                    let (g, n, st2) = gns(more, raw)?;
                    base.gender = g;
                    base.number = n;
                    base.state = st2;
                }
            }
            Ok(base)
        }
        ['V'] => Err(format!("truncated verb code {seg} in {raw}")),
        ['N', t, rest @ ..] => typed(lang, raw, "noun", NOUN_TYPES, *t, rest),
        ['A', t, rest @ ..] => typed(lang, raw, "adjective", ADJ_TYPES, *t, rest),
        ['P', t, rest @ ..] => {
            let ty = code(PRONOUN_TYPES, "pronoun type", *t, raw)?;
            let (p, g, n) = pgn(rest, raw)?;
            let mut m = Morph::empty(lang, "pronoun", raw);
            m.ty = ty;
            m.person = p;
            m.gender = g;
            m.number = n;
            Ok(m)
        }
        ['S', t, rest @ ..] => {
            let ty = code(SUFFIX_TYPES, "suffix type", *t, raw)?;
            let (p, g, n) = pgn(rest, raw)?;
            let mut m = Morph::empty(lang, "suffix", raw);
            m.ty = ty;
            m.person = p;
            m.gender = g;
            m.number = n;
            Ok(m)
        }
        ['T'] => Ok(Morph::empty(lang, "particle", raw)),
        ['T', t] => {
            let ty = code(PARTICLE_TYPES, "particle type", *t, raw)?;
            let mut m = Morph::empty(lang, "particle", raw);
            m.ty = ty;
            Ok(m)
        }
        ['T', ..] => Err(format!("trailing letters on particle code {seg} in {raw}")),
        ['R'] => Ok(Morph::empty(lang, "preposition", raw)),
        ['R', 'd'] => {
            let mut m = Morph::empty(lang, "preposition", raw);
            m.ty = Some("definite article".to_string());
            Ok(m)
        }
        ['R', ..] => Err(format!("unknown preposition code {seg} in {raw}")),
        ['C'] => Ok(Morph::empty(lang, "conjunction", raw)),
        ['D'] => Ok(Morph::empty(lang, "adverb", raw)),
        ['N'] => Err(format!("bare noun code in {raw}")),
        _ => Err(format!("unknown morph segment {seg} in {raw}")),
    }
}

fn typed(lang: MorphLang, raw: &str, pos: &str, table: Table, t: char, rest: &[char]) -> Result<Morph, String> {
    let ty = code(table, &format!("{pos} type"), t, raw)?;
    let (g, n, st) = gns(rest, raw)?;
    let mut m = Morph::empty(lang, pos, raw);
    m.ty = ty;
    m.gender = g;
    m.number = n;
    m.state = st;
    Ok(m)
}

type Triple = (Option<String>, Option<String>, Option<String>);

/// gender / number / state.
fn gns(cs: &[char], raw: &str) -> Result<Triple, String> {
    match cs {
        [] => Ok((None, None, None)),
        [g] => Ok((code(GENDERS, "gender", *g, raw)?, None, None)),
        [g, n] => Ok((code(GENDERS, "gender", *g, raw)?, code(NUMBERS, "number", *n, raw)?, None)),
        [g, n, st] => {
            Ok((code(GENDERS, "gender", *g, raw)?, code(NUMBERS, "number", *n, raw)?, code(STATES, "state", *st, raw)?))
        }
        _ => Err(format!("trailing letters {} in {raw}", cs.iter().collect::<String>())),
    }
}

/// person / gender / number.
fn pgn(cs: &[char], raw: &str) -> Result<Triple, String> {
    match cs {
        [] => Ok((None, None, None)),
        [p] => Ok((code(PERSONS, "person", *p, raw)?, None, None)),
        [p, g] => Ok((code(PERSONS, "person", *p, raw)?, code(GENDERS, "gender", *g, raw)?, None)),
        [p, g, n] => Ok((
            code(PERSONS, "person", *p, raw)?,
            code(GENDERS, "gender", *g, raw)?,
            code(NUMBERS, "number", *n, raw)?,
        )),
        _ => Err(format!("trailing letters {} in {raw}", cs.iter().collect::<String>())),
    }
}

// ── Robinson (Greek) parser ─────────────────────────────────────────────────

const TENSES: Table =
    &[('P', "present"), ('I', "imperfect"), ('F', "future"), ('A', "aorist"), ('R', "perfect"), ('L', "pluperfect")];
const VOICES: Table = &[
    ('A', "active"),
    ('M', "middle"),
    ('P', "passive"),
    ('E', "middle/passive"),
    ('D', "middle deponent"),
    ('O', "passive deponent"),
    ('N', "middle/passive deponent"),
];
const MOODS: Table = &[
    ('I', "indicative"),
    ('S', "subjunctive"),
    ('O', "optative"),
    ('M', "imperative"),
    ('N', "infinitive"),
    ('P', "participle"),
];
const CASES: Table = &[('N', "nominative"), ('V', "vocative"), ('G', "genitive"), ('D', "dative"), ('A', "accusative")];
const GNUMBERS: Table = &[('S', "singular"), ('P', "plural")];
const GGENDERS: Table = &[('M', "masculine"), ('F', "feminine"), ('N', "neuter")];

fn undeclined(p: &str) -> Option<(&'static str, Option<&'static str>)> {
    match p {
        "ADV" => Some(("adverb", None)),
        "CONJ" => Some(("conjunction", None)),
        "COND" => Some(("particle", Some("conditional"))),
        "PRT" => Some(("particle", None)),
        "PREP" => Some(("preposition", None)),
        "INJ" => Some(("interjection", None)),
        "ARAM" => Some(("transliterated word", Some("Aramaic"))),
        "HEB" => Some(("transliterated word", Some("Hebrew"))),
        _ => None,
    }
}

fn indeclinable(p: &str, sub: &str) -> Option<(&'static str, Option<&'static str>, &'static str)> {
    match (p, sub) {
        ("N", "PRI") => Some(("noun", Some("proper name"), "indeclinable")),
        ("A", "NUI") => Some(("adjective", Some("numeral"), "indeclinable")),
        ("N", "LI") => Some(("noun", Some("letter"), "indeclinable")),
        ("N", "OI") => Some(("noun", None, "indeclinable")),
        _ => None,
    }
}

fn declined_prefix(p: &str) -> Option<(&'static str, Option<&'static str>)> {
    match p {
        "N" => Some(("noun", None)),
        "A" => Some(("adjective", None)),
        "R" => Some(("pronoun", Some("relative"))),
        "C" => Some(("pronoun", Some("reciprocal"))),
        "D" => Some(("pronoun", Some("demonstrative"))),
        "T" => Some(("article", None)),
        "K" => Some(("pronoun", Some("correlative"))),
        "I" => Some(("pronoun", Some("interrogative"))),
        "X" => Some(("pronoun", Some("indefinite"))),
        "Q" => Some(("pronoun", Some("correlative/interrogative"))),
        "F" => Some(("pronoun", Some("reflexive"))),
        "S" => Some(("adjective", Some("possessive"))),
        "P" => Some(("pronoun", Some("personal"))),
        _ => None,
    }
}

fn mark_of(m: &str) -> Option<&'static str> {
    match m {
        "S" => Some("superlative"),
        "C" => Some("comparative"),
        "ABB" => Some("abbreviated form"),
        "I" => Some("interrogative"),
        "N" => Some("negative"),
        "K" => Some("crasis"),
        "ATT" => Some("Attic form"),
        _ => None,
    }
}

/// Parse a Robinson code (`"V-AAI-3S"`, `"N-GSF"`, `"T-ASM"`, `"P-2DP"`, …).
pub fn parse_robinson(raw: &str) -> Result<Morph, String> {
    let parts: Vec<&str> = raw.split('-').collect();
    match parts.as_slice() {
        ["V", rest @ ..] => robinson_verb(raw, rest),
        [p, rest @ ..] => {
            // Indeclinable specials keyed on (prefix, first-of-rest), before the
            // general declined grammar.
            if let Some(first) = rest.first() {
                if let Some((pos, ty, mark)) = indeclinable(p, first) {
                    let mut m = Morph::empty(MorphLang::Greek, pos, raw);
                    m.ty = ty.map(str::to_string);
                    m.suffixes = vec![mark.to_string()];
                    return with_marks(raw, &rest[1..], m);
                }
            }
            if let Some((pos, ty)) = undeclined(p) {
                let mut m = Morph::empty(MorphLang::Greek, pos, raw);
                m.ty = ty.map(str::to_string);
                return with_marks(raw, rest, m);
            }
            if let Some((pos, ty)) = declined_prefix(p) {
                return robinson_declined(raw, pos, ty, p, rest);
            }
            Err(format!("unknown Robinson code {raw}"))
        }
        _ => Err(format!("unknown Robinson code {raw}")),
    }
}

fn robinson_verb(raw: &str, rest: &[&str]) -> Result<Morph, String> {
    let (tvm, tail) = rest.split_first().ok_or_else(|| format!("truncated verb code {raw}"))?;
    // "2A" = second aorist etc.; functionally the same tense.
    let (second, tvm) = match tvm.strip_prefix('2') {
        Some(t) => (true, t),
        None => (false, *tvm),
    };
    let tvm: Vec<char> = tvm.chars().collect();
    let tense0 = code(TENSES, "tense", *tvm.first().ok_or_else(|| format!("missing tense in {raw}"))?, raw)?;
    let tense = if second { tense0.map(|t| format!("second {t}")) } else { tense0 };
    let (voice, mood) = match tvm.get(1..) {
        Some([v, m]) => (code(VOICES, "voice", *v, raw)?, code(MOODS, "mood", *m, raw)?),
        _ => return Err(format!("bad tense-voice-mood in {raw}")),
    };
    let mut base = Morph::empty(MorphLang::Greek, "verb", raw);
    base.conj = tense;
    base.voice = voice;
    base.mood = mood;
    match tail {
        [] => Ok(base),
        [pn, marks @ ..] if *pn == "ATT" => {
            let mut all = vec![*pn];
            all.extend_from_slice(marks);
            with_marks(raw, &all, base)
        }
        [pn, marks @ ..] => {
            let b = robinson_pn_or_cng(raw, pn, base)?;
            with_marks(raw, marks, b)
        }
    }
}

/// Verb ending: person-number ("3S") or case-number-gender ("NSM", participles).
fn robinson_pn_or_cng(raw: &str, t: &str, mut b: Morph) -> Result<Morph, String> {
    let cs: Vec<char> = t.chars().collect();
    match cs.as_slice() {
        [p, n] if p.is_ascii_digit() => {
            b.person = code(PERSONS, "person", *p, raw)?;
            b.number = code(GNUMBERS, "number", *n, raw)?;
            Ok(b)
        }
        [c, n, g] => {
            b.case_ = code(CASES, "case", *c, raw)?;
            b.number = code(GNUMBERS, "number", *n, raw)?;
            b.gender = code(GGENDERS, "gender", *g, raw)?;
            Ok(b)
        }
        _ => Err(format!("bad ending {t} in {raw}")),
    }
}

fn robinson_declined(raw: &str, pos: &str, ty: Option<&str>, p: &str, rest: &[&str]) -> Result<Morph, String> {
    let mut m = Morph::empty(MorphLang::Greek, pos, raw);
    m.ty = ty.map(str::to_string);
    let Some((t, marks)) = rest.split_first() else {
        return Ok(m);
    };
    let cs: Vec<char> = t.chars().collect();
    let m = robinson_decl_body(raw, p, &cs, m)?;
    with_marks(raw, marks, m)
}

fn robinson_decl_body(raw: &str, p: &str, cs: &[char], mut b: Morph) -> Result<Morph, String> {
    // [person][possessor-number]case number[gender]
    if let [d, more @ ..] = cs {
        if d.is_ascii_digit() {
            let pe = code(PERSONS, "person", *d, raw)?;
            if p == "S" {
                // Possessive adjectives carry the possessor's number too
                // (S-2PDSM: 2nd person plural possessor, dative sing. masc.).
                let (pn, more2) = more.split_first().ok_or_else(|| format!("bad declension ending in {raw}"))?;
                let pnum = code(GNUMBERS, "possessor number", *pn, raw)?;
                let mut b1 = robinson_cng(raw, more2, b)?;
                b1.person = pe;
                if let Some(n) = pnum {
                    b1.suffixes.push(format!("{n} possessor"));
                }
                return Ok(b1);
            } else {
                let mut b1 = robinson_cng(raw, more, b)?;
                b1.person = pe;
                return Ok(b1);
            }
        }
    }
    b = robinson_cng(raw, cs, b)?;
    Ok(b)
}

fn robinson_cng(raw: &str, cs: &[char], mut b: Morph) -> Result<Morph, String> {
    match cs {
        [c, n] => {
            b.case_ = code(CASES, "case", *c, raw)?;
            b.number = code(GNUMBERS, "number", *n, raw)?;
            Ok(b)
        }
        [c, n, g] => {
            b.case_ = code(CASES, "case", *c, raw)?;
            b.number = code(GNUMBERS, "number", *n, raw)?;
            b.gender = code(GGENDERS, "gender", *g, raw)?;
            Ok(b)
        }
        _ => Err(format!("bad declension ending {} in {raw}", cs.iter().collect::<String>())),
    }
}

fn with_marks(raw: &str, marks: &[&str], mut b: Morph) -> Result<Morph, String> {
    for m in marks {
        match mark_of(m) {
            Some(name) => b.suffixes.push(name.to_string()),
            None => return Err(format!("unknown mark -{m} in {raw}")),
        }
    }
    Ok(b)
}

/// Pick the parser by the annotation's Strong's ref: `G…` → Robinson, else OSHM.
pub fn parse_for(strong: &str, code: &str) -> Result<Morph, String> {
    if strong.starts_with('G') {
        parse_robinson(code)
    } else {
        parse_morph(code)
    }
}

// ── rendering ─────────────────────────────────────────────────────────────────

fn up_first(t: &str) -> String {
    let mut cs = t.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

fn join_words(parts: &[&Option<String>]) -> String {
    parts.iter().filter_map(|p| p.as_deref()).collect::<Vec<_>>().join(" ")
}

/// Feature phrase in each language's conventional order.
fn pgn_state(m: &Morph) -> String {
    match m.lang {
        MorphLang::Greek => join_words(&[&m.person, &m.case_, &m.number, &m.gender]),
        _ => join_words(&[&m.person, &m.gender, &m.number, &m.state]),
    }
}

fn join_parts(head: &str, rest: &str) -> String {
    if rest.is_empty() {
        head.to_string()
    } else {
        format!("{head}, {rest}")
    }
}

/// A trailing segment as attached description.
fn render_as_suffix(m: &Morph) -> String {
    let join_with = |parts: String, name: &str| {
        if parts.is_empty() {
            name.to_string()
        } else {
            format!("{parts} {name}")
        }
    };
    match (m.pos.as_str(), m.ty.as_deref()) {
        ("suffix", Some("pronominal")) => join_with(pgn_state(m), "pronominal suffix"),
        ("suffix", Some(ty)) => ty.to_string(),
        ("suffix", None) => "suffix".to_string(),
        ("pronoun", _) => join_with(pgn_state(m), "pronominal suffix"),
        ("particle", Some("definite article")) => "definite article".to_string(),
        _ => m.raw.clone(),
    }
}

/// The study-panel phrase, e.g. `"Qal wayyiqtol, 3rd masculine singular"`,
/// `"common noun, feminine singular absolute"`, `"aorist active indicative, 3rd
/// singular"`, `"proper name"`.
pub fn render_morph(m: &Morph) -> String {
    let lang_mark = if m.lang == MorphLang::Aramaic { "Aramaic " } else { "" };
    let st = pgn_state(m);
    let body = match m.pos.as_str() {
        "verb" => {
            let head = match m.lang {
                MorphLang::Greek => join_words(&[&m.conj, &m.voice, &m.mood]),
                _ => {
                    let stem_up = m.stem.as_deref().map(up_first);
                    join_words(&[&stem_up, &m.conj])
                }
            };
            join_parts(&head, &st)
        }
        "noun" => match m.ty.as_deref() {
            Some("proper name") => join_parts("proper name", &st),
            Some(ty) => join_parts(&format!("{ty} noun"), &st),
            None => join_parts("noun", &st),
        },
        "adjective" => match m.ty.as_deref() {
            Some("adjective") => join_parts("adjective", &st),
            Some("possessive") => join_parts("possessive adjective", &st),
            Some("numeral") => join_parts("numeral", &st),
            Some(ty) => join_parts(ty, &st),
            None => join_parts("adjective", &st),
        },
        "pronoun" => {
            let name = m.ty.as_deref().map(|t| format!("{t} pronoun")).unwrap_or_else(|| "pronoun".to_string());
            join_parts(&name, &st)
        }
        "preposition" => match m.ty.as_deref() {
            Some(ty) => format!("preposition ({ty})"),
            None => "preposition".to_string(),
        },
        "particle" => match m.ty.as_deref() {
            Some("direct object marker") => "direct object marker".to_string(),
            Some("definite article") => "definite article".to_string(),
            Some("interjection") => "interjection".to_string(),
            Some(ty) => format!("{ty} particle"),
            None => "particle".to_string(),
        },
        "article" => join_parts("definite article", &st),
        "transliterated word" => {
            let pre = m.ty.as_deref().map(|t| format!("{t} ")).unwrap_or_default();
            format!("{pre}transliterated word")
        }
        "suffix" => render_as_suffix(m),
        pos => join_parts(pos, &st),
    };
    let suffix_tail = if m.suffixes.is_empty() {
        String::new()
    } else {
        // Hebrew trails attached morphemes ("; with …"); Greek trails qualities
        // of the form ("; comparative").
        let connective = if m.lang == MorphLang::Greek { "; " } else { "; with " };
        format!("{connective}{}", m.suffixes.join(" and "))
    };
    format!("{lang_mark}{body}{suffix_tail}")
}

// ── the sidecar ─────────────────────────────────────────────────────────────

/// One projected annotation: KJV token `tok` renders the original word with
/// Strong's `strongs`; `code` is that word's parsing code, `homograph` the OSHB
/// homograph letter Strong's numbering lumps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphEntry {
    pub tok: u32,
    pub strongs: String,
    pub homograph: Option<String>,
    pub code: String,
}

/// A loaded morphology sidecar: the per-verse index plus each distinct code
/// parsed once (a few thousand exist; codes that fail to parse are absent, the
/// projection already counted them).
#[derive(Debug, Clone, Default)]
pub struct MorphData {
    ix: HashMap<VRef, Vec<MorphEntry>>,
    source: String,
    parsed: HashMap<String, Morph>,
}

#[derive(Deserialize)]
struct HeaderWire {
    tokenization: String,
    #[serde(default)]
    source: String,
}

#[derive(Deserialize)]
struct VerseWire {
    b: String,
    c: u16,
    v: u16,
    e: Vec<(u32, String, Option<String>, String)>,
}

impl MorphData {
    /// Provenance line from the sidecar header.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Number of verses carrying morphology annotations.
    pub fn verse_count(&self) -> usize {
        self.ix.len()
    }

    /// The entries for a verse, in token order.
    pub fn entries(&self, vref: &VRef) -> &[MorphEntry] {
        self.ix.get(vref).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The rendered morphology phrase for token `tok` of `vref`, if the sidecar
    /// annotates it and its code parsed.
    pub fn gloss(&self, vref: &VRef, tok: u32) -> Option<String> {
        let entry = self.ix.get(vref)?.iter().find(|e| e.tok == tok)?;
        self.parsed.get(&entry.code).map(render_morph)
    }

    /// Parse from the sidecar text (header line + one JSON object per verse).
    /// `None` on a tokenization mismatch or when the header won't parse.
    pub fn parse(tok_version: &str, text: &str) -> Option<MorphData> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header: HeaderWire = serde_json::from_str(lines.next()?).ok()?;
        if header.tokenization != tok_version {
            return None; // stale: addresses a different tokenization
        }
        let mut ix: HashMap<VRef, Vec<MorphEntry>> = HashMap::new();
        let mut parsed: HashMap<String, Morph> = HashMap::new();
        for line in lines {
            let Ok(w) = serde_json::from_str::<VerseWire>(line) else { continue };
            let vref = VRef::new(&w.b, w.c, w.v);
            let mut entries = Vec::with_capacity(w.e.len());
            for (tok, strongs, homograph, code_str) in w.e {
                if !parsed.contains_key(&code_str) {
                    if let Ok(m) = parse_for(&strongs, &code_str) {
                        parsed.insert(code_str.clone(), m);
                    }
                }
                entries.push(MorphEntry { tok, strongs, homograph, code: code_str });
            }
            ix.insert(vref, entries);
        }
        Some(MorphData { ix, source: header.source, parsed })
    }
}

/// Load `data/morphology.jsonl`. Missing file / stale stamp / parse failure →
/// `None` (the layer is optional).
pub fn load_morph(tok_version: &str, path: impl AsRef<Path>) -> Option<MorphData> {
    let path = path.as_ref();
    // The packed sibling first — same annotations, no 10.4 MB of JSON. A home
    // with only the text (an older pack, a hand-built home) still works, and a
    // packed file we cannot read falls through to the text as well.
    if let Ok(bytes) = std::fs::read(morphb_path(path)) {
        if let Some(m) = parse_morph_bin(tok_version, &bytes) {
            return Some(m);
        }
    }
    let text = std::fs::read_to_string(path).ok()?;
    MorphData::parse(tok_version, &text)
}

/// `data/morphology.jsonl` → `data/morphology.morphb`.
pub fn morphb_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("morphb")
}

// ── the packed form (`.morphb`) ────────────────────────────────────────────────
//
// The sidecar is 10.4 MB of JSONL: 31,091 `serde_json` calls building 355,603
// entries, each allocating three strings. Like the concept vectors, the parsed
// result cannot outlive a browser tab, so a phone repeated the whole thing on
// every launch — and this half cost twice what the vectors did.
//
// The shape that makes it cheap is the data's own repetition: those 355,603
// entries use only 13,990 distinct Strong's numbers, 2,840 codes and 6
// homographs. Interned, an entry is four small integers, so the body is fixed
// -width records and the file lands SMALLER than the text it replaces.
//
//   0..8    magic "PLMORB01"
//   8..12   verse_count u32
//   12..16  entry_count u32
//   16..20  string_count u32   (shared table: books, Strong's, codes)
//   20..24  homograph_count u32
//   then    tokenization, then source: u32 length + bytes, each padded to 4
//   then    string table:    u32 length + bytes per entry, padded to 4
//   then    homograph table: same, index 0 is the empty "none" slot
//   then    verses:  book u16, chapter u16, verse u16, n_entries u16
//   then    entries: tok u16, strongs u16, code u16, homograph u8, pad u8
//
// Indices are u16 on purpose — the encoder REFUSES rather than truncate if the
// data ever outgrows them, and the caller keeps shipping the text.

const MORPHB_MAGIC: &[u8; 8] = b"PLMORB01";
const MORPHB_HEADER: usize = 24;

fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// Pack the sidecar text into [`parse_morph_bin`]'s form. `None` if the text is
/// unreadable, stale, or too large for the packed index widths.
pub fn encode_morph_bin(tok_version: &str, text: &str) -> Option<Vec<u8>> {
    let data = MorphData::parse(tok_version, text)?;

    // Deterministic order — the pack manifest hashes this file, so an unstable
    // traversal would churn the pack version on every build.
    let mut refs: Vec<&VRef> = data.ix.keys().collect();
    refs.sort_by(|a, b| (&a.book, a.chapter, a.verse).cmp(&(&b.book, b.chapter, b.verse)));

    let mut strings: Vec<&str> = Vec::new();
    let mut sx: HashMap<&str, u16> = HashMap::new();
    let mut homs: Vec<&str> = vec![""]; // 0 = no homograph
    let mut hx: HashMap<&str, u8> = HashMap::new();

    macro_rules! intern_str {
        ($s:expr) => {{
            let s: &str = $s;
            match sx.get(s) {
                Some(&i) => i,
                None => {
                    let i = u16::try_from(strings.len()).ok()?;
                    strings.push(s);
                    sx.insert(s, i);
                    i
                }
            }
        }};
    }

    let mut verses: Vec<[u16; 4]> = Vec::with_capacity(refs.len());
    let mut entries: Vec<[u8; 8]> = Vec::new();
    for r in &refs {
        let es = &data.ix[*r];
        let book = intern_str!(r.book.as_str());
        let n = u16::try_from(es.len()).ok()?;
        verses.push([book, r.chapter, r.verse, n]);
        for e in es {
            let strongs = intern_str!(e.strongs.as_str());
            let code = intern_str!(e.code.as_str());
            let hom: u8 = match e.homograph.as_deref() {
                None | Some("") => 0,
                Some(h) => match hx.get(h) {
                    Some(&i) => i,
                    None => {
                        let i = u8::try_from(homs.len()).ok()?;
                        homs.push(h);
                        hx.insert(h, i);
                        i
                    }
                },
            };
            let tok = u16::try_from(e.tok).ok()?;
            let mut rec = [0u8; 8];
            rec[0..2].copy_from_slice(&tok.to_le_bytes());
            rec[2..4].copy_from_slice(&strongs.to_le_bytes());
            rec[4..6].copy_from_slice(&code.to_le_bytes());
            rec[6] = hom;
            entries.push(rec);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(MORPHB_MAGIC);
    out.extend_from_slice(&(verses.len() as u32).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    out.extend_from_slice(&(homs.len() as u32).to_le_bytes());
    push_str(&mut out, tok_version);
    push_str(&mut out, &data.source);
    for s in &strings {
        push_str(&mut out, s);
    }
    for h in &homs {
        push_str(&mut out, h);
    }
    for v in &verses {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    for e in &entries {
        out.extend_from_slice(e);
    }
    Some(out)
}

/// Read the packed form. `None` on a foreign/short/stale file, so the caller
/// falls back to the text exactly as if the packed one weren't there.
pub fn parse_morph_bin(tok_version: &str, bytes: &[u8]) -> Option<MorphData> {
    if bytes.len() < MORPHB_HEADER || &bytes[..8] != MORPHB_MAGIC {
        return None;
    }
    let u32_at =
        |o: usize| -> Option<usize> { Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?) as usize) };
    let verse_count = u32_at(8)?;
    let entry_count = u32_at(12)?;
    let string_count = u32_at(16)?;
    let hom_count = u32_at(20)?;

    let mut at = MORPHB_HEADER;
    let take_str = |at: &mut usize| -> Option<String> {
        let len = u32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?) as usize;
        *at += 4;
        let s = std::str::from_utf8(bytes.get(*at..*at + len)?).ok()?.to_string();
        *at += len;
        while *at % 4 != 0 {
            *at += 1;
        }
        Some(s)
    };

    let stamp = take_str(&mut at)?;
    if stamp != tok_version {
        return None; // stale: addresses a different tokenization
    }
    let source = take_str(&mut at)?;
    let mut strings: Vec<String> = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        strings.push(take_str(&mut at)?);
    }
    let mut homs: Vec<String> = Vec::with_capacity(hom_count);
    for _ in 0..hom_count {
        homs.push(take_str(&mut at)?);
    }

    let verses_at = at;
    let entries_at = verses_at.checked_add(verse_count.checked_mul(8)?)?;
    if bytes.len() < entries_at.checked_add(entry_count.checked_mul(8)?)? {
        return None;
    }

    let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let mut ix: HashMap<VRef, Vec<MorphEntry>> = HashMap::with_capacity(verse_count);
    let mut parsed: HashMap<String, Morph> = HashMap::new();
    let mut e_at = entries_at;
    for i in 0..verse_count {
        let v = verses_at + i * 8;
        let book = strings.get(u16_at(v) as usize)?;
        let vref = VRef::new(book.as_str(), u16_at(v + 2), u16_at(v + 4));
        let n = u16_at(v + 6) as usize;
        let mut es = Vec::with_capacity(n);
        for _ in 0..n {
            let tok = u16_at(e_at) as u32;
            let strongs = strings.get(u16_at(e_at + 2) as usize)?;
            let code = strings.get(u16_at(e_at + 4) as usize)?;
            let hom = bytes[e_at + 6] as usize;
            e_at += 8;
            // Each distinct code parsed once, exactly as the text path does.
            if !parsed.contains_key(code.as_str()) {
                if let Ok(m) = parse_for(strongs, code) {
                    parsed.insert(code.clone(), m);
                }
            }
            es.push(MorphEntry {
                tok,
                strongs: strongs.clone(),
                homograph: homs.get(hom).filter(|h| !h.is_empty()).cloned(),
                code: code.clone(),
            });
        }
        ix.insert(vref, es);
    }
    Some(MorphData { ix, source, parsed })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heb(code: &str) -> String {
        render_morph(&parse_morph(code).unwrap())
    }
    fn grk(code: &str) -> String {
        render_morph(&parse_robinson(code).unwrap())
    }

    #[test]
    fn hebrew_codes_render() {
        // Gen 1:1 tokens: noun fem sing absolute; verb qal wayyiqtol 3ms.
        assert_eq!(heb("HNcfsa"), "common noun, feminine singular absolute");
        assert_eq!(heb("HVqp3ms"), "Qal perfect, 3rd masculine singular");
        assert_eq!(heb("HVqw3ms"), "Qal wayyiqtol, 3rd masculine singular");
        // Object marker particle, and the article preposition.
        assert_eq!(heb("HTo"), "direct object marker");
        // Pronominal suffix on a construct noun.
        assert_eq!(
            heb("HNcmsc/Sp2ms"),
            "common noun, masculine singular construct; with 2nd masculine singular pronominal suffix"
        );
    }

    #[test]
    fn aramaic_is_marked() {
        assert!(heb("ANcmsd/Td").starts_with("Aramaic "));
    }

    #[test]
    fn greek_codes_render() {
        assert_eq!(grk("N-GSF"), "noun, genitive singular feminine");
        assert_eq!(grk("V-AAI-3S"), "aorist active indicative, 3rd singular");
        assert_eq!(grk("V-2AAP-NSM"), "second aorist active participle, nominative singular masculine");
        assert_eq!(grk("T-ASM"), "definite article, accusative singular masculine");
        assert_eq!(grk("A-NSM-S"), "adjective, nominative singular masculine; superlative");
        assert_eq!(grk("N-PRI"), "proper name; indeclinable");
    }

    #[test]
    fn unknown_codes_error_not_panic() {
        assert!(parse_morph("Zxyz").is_err());
        assert!(parse_robinson("V-ZZZ").is_err());
    }

    #[test]
    fn sidecar_parses_and_glosses() {
        let text = "{\"format\":\"overlay-morphology-v1\",\"tokenization\":\"kjv1769-tok2\",\"source\":\"OSHB + TR\"}\n\
                    {\"b\":\"Gen\",\"c\":1,\"v\":1,\"e\":[[2,\"H7225\",null,\"HNcfsa\"],[4,\"H1254\",\"a\",\"HVqp3ms\"]]}\n";
        let md = MorphData::parse("kjv1769-tok2", text).unwrap();
        assert_eq!(md.source(), "OSHB + TR");
        assert_eq!(md.entries(&VRef::new("Gen", 1, 1)).len(), 2);
        assert_eq!(md.gloss(&VRef::new("Gen", 1, 1), 2).as_deref(), Some("common noun, feminine singular absolute"));
        assert_eq!(md.gloss(&VRef::new("Gen", 1, 1), 4).as_deref(), Some("Qal perfect, 3rd masculine singular"));
        assert_eq!(md.gloss(&VRef::new("Gen", 1, 1), 99), None);
        // Stale tokenization refused.
        assert!(MorphData::parse("other", text).is_none());
    }

    // ── the packed `.morphb` form ──────────────────────────────────────────────

    const SIDECAR: &str =
        "{\"format\":\"overlay-morphology-v1\",\"tokenization\":\"kjv1769-tok2\",\"source\":\"OSHB + TR\"}\n\
        {\"b\":\"Gen\",\"c\":1,\"v\":1,\"e\":[[2,\"H7225\",null,\"HNcfsa\"],[4,\"H1254\",\"a\",\"HVqp3ms\"]]}\n\
        {\"b\":\"Gen\",\"c\":1,\"v\":2,\"e\":[[1,\"H776\",null,\"HNcfsa\"]]}\n\
        {\"b\":\"John\",\"c\":3,\"v\":16,\"e\":[[3,\"G2316\",null,\"N-NSM\"]]}\n";

    /// The packed form must be the SAME sidecar: same verses, same entries in
    /// token order, same glosses, same provenance — compared through the API,
    /// since byte-equality with the text would prove nothing about what a reader
    /// gets. Includes a homograph, a Greek code and a Hebrew one.
    #[test]
    fn packed_morphology_loads_identically_to_the_text() {
        let text = MorphData::parse("kjv1769-tok2", SIDECAR).unwrap();
        let bytes = encode_morph_bin("kjv1769-tok2", SIDECAR).expect("encodes");
        let packed = parse_morph_bin("kjv1769-tok2", &bytes).unwrap();

        assert_eq!(packed.source(), text.source());
        assert_eq!(packed.verse_count(), text.verse_count());
        for r in [VRef::new("Gen", 1, 1), VRef::new("Gen", 1, 2), VRef::new("John", 3, 16)] {
            assert_eq!(packed.entries(&r), text.entries(&r), "{r:?} entries differ");
            for tok in 0..6u32 {
                assert_eq!(packed.gloss(&r, tok), text.gloss(&r, tok), "{r:?} tok {tok} gloss differs");
            }
        }
        // The homograph rides along rather than being flattened away.
        assert_eq!(packed.entries(&VRef::new("Gen", 1, 1))[1].homograph.as_deref(), Some("a"));
        // A verse the sidecar never mentions stays empty.
        assert!(packed.entries(&VRef::new("Rev", 1, 1)).is_empty());
    }

    /// Byte-for-byte stable across runs: the pack manifest hashes this file, so
    /// an unstable HashMap traversal would churn the pack version every build.
    #[test]
    fn packing_is_deterministic() {
        let a = encode_morph_bin("kjv1769-tok2", SIDECAR).unwrap();
        for _ in 0..8 {
            assert_eq!(encode_morph_bin("kjv1769-tok2", SIDECAR).unwrap(), a);
        }
    }

    /// A foreign, truncated or stale packed file must read as "absent" so the
    /// caller falls back to the text rather than losing the layer.
    #[test]
    fn a_bad_packed_sidecar_is_none_not_garbage() {
        let good = encode_morph_bin("kjv1769-tok2", SIDECAR).unwrap();
        assert!(parse_morph_bin("kjv1769-tok2", b"not a morphb").is_none());
        assert!(parse_morph_bin("kjv1769-tok2", &good[..good.len() - 8]).is_none());
        assert!(parse_morph_bin("kjv1769-tok2", &[]).is_none());
        // The stamp is carried INSIDE the packed file, so staleness is caught
        // without the text header being present at all.
        assert!(parse_morph_bin("kjv1611-tok1", &good).is_none());
        assert!(encode_morph_bin("kjv1611-tok1", SIDECAR).is_none());
    }
}
