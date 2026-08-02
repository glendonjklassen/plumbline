//! The hymnal (`hymnal-v1`): public domain hymns, with chords inline.
//!
//! `data/hymnal.json` is one object — `{format:"hymnal-v1", hymns:[...]}` —
//! assembled offline by `scripts/build-hymnal.mjs` from the per-hymn source
//! files in `data-prep/hymnal/` (which carry sourcing URLs and maintainer
//! notes that never ship). Like every on-disk format here, the tag is frozen:
//! additive evolution only.
//!
//! A hymn is one entry with a stable book `number` and one text per language
//! (`texts: {"en": ..., "de": ...}`) — a translation is a second text on the
//! SAME hymn, not a second hymn, because the language toggle is the seed of
//! full multi-language support (decision 2026-08-01) and a hymn split across
//! entries would need stitching back together the day that lands.
//!
//! Chords ride ChordPro-style inside the text — `A[G]mazing [C]grace` — so a
//! stanza is a plain string a human can read and diff. The parser here turns a
//! line into (chord?, text) segments for the shells to paint, and transposition
//! rewrites chord roots by semitone offset. An UNPARSEABLE bracket stays in the
//! text as literal characters rather than vanishing: the file is
//! maintainer-authored data, and swallowing a typo would hide it from exactly
//! the person who can fix it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::Error;

/// The frozen format tag. Loaders refuse anything else.
pub const FORMAT: &str = "hymnal-v1";

/// One language's text of a hymn.
#[derive(Debug, Clone, Deserialize)]
pub struct HymnText {
    pub title: String,
    pub author: String,
    #[serde(default)]
    pub translator: Option<String>,
    #[serde(default)]
    pub year: Option<u32>,
    /// One string per stanza; lines joined with `\n`, chords in brackets.
    pub stanzas: Vec<String>,
    /// The refrain, stanza-shaped, sung after every stanza. Chords are on the
    /// first stanza and the chorus only — a songbook chart, not a score.
    #[serde(default)]
    pub chorus: Option<String>,
}

/// One hymn: a stable number, a tune, and a text per language.
#[derive(Debug, Clone, Deserialize)]
pub struct Hymn {
    pub id: String,
    pub number: u32,
    pub tune: String,
    pub meter: String,
    /// The key the charts are written in (e.g. `"G"`, `"Em"`). One per hymn:
    /// every language shares the tune, so it shares the chart.
    pub key: String,
    /// Language code → text. A BTreeMap so iteration order is stable across
    /// runs (golden wire samples pin it).
    pub texts: BTreeMap<String, HymnText>,
}

#[derive(Deserialize)]
struct HymnalDoc {
    format: String,
    hymns: Vec<Hymn>,
}

/// The loaded book, in `number` order.
#[derive(Debug, Clone, Default)]
pub struct Hymnal {
    pub hymns: Vec<Hymn>,
}

impl Hymnal {
    pub fn get(&self, id: &str) -> Option<&Hymn> {
        self.hymns.iter().find(|h| h.id == id)
    }
}

/// Load `data/hymnal.json`. A missing file is an EMPTY hymnal, not an error —
/// an old pack or a trimmed home simply has no hymn tab content — but a file
/// that exists and does not parse, or carries the wrong format tag, is a real
/// error: data is present and unusable, and pretending otherwise hides it.
pub fn load(path: impl AsRef<Path>) -> Result<Hymnal, Error> {
    let path = path.as_ref();
    match std::fs::read_to_string(path) {
        Ok(raw) => from_str(&raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Hymnal::default()),
        Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
    }
}

/// Parse a hymnal document. Refuses a format-tag mismatch (frozen contract).
pub fn from_str(raw: &str) -> Result<Hymnal, Error> {
    let doc: HymnalDoc = serde_json::from_str(raw).map_err(|e| Error::Parse(format!("hymnal: {e}")))?;
    if doc.format != FORMAT {
        return Err(Error::Parse(format!("hymnal: format {:?}, expected {FORMAT:?}", doc.format)));
    }
    let mut hymns = doc.hymns;
    hymns.sort_by_key(|h| h.number);
    Ok(Hymnal { hymns })
}

// ── ChordPro lines ────────────────────────────────────────────────────────────

/// A run of text with the chord (if any) struck at its first syllable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub chord: Option<String>,
    pub text: String,
}

/// Split one line into segments at its `[chord]` brackets.
///
/// Text before the first bracket becomes a chordless segment, so lyrics keep
/// their exact spelling when a chord lands mid-word (`A[G]mazing`). A bracket
/// pair whose content is not a chord by [`is_chord`] is kept verbatim in the
/// text (see the module docs for why), and an unclosed `[` is likewise
/// literal.
pub fn parse_line(line: &str) -> Vec<Segment> {
    let mut segs: Vec<Segment> = Vec::new();
    let mut text = String::new();
    let mut chord: Option<String> = None;
    let mut rest = line;
    loop {
        match rest.find('[') {
            None => {
                text.push_str(rest);
                break;
            }
            Some(at) => {
                text.push_str(&rest[..at]);
                let after = &rest[at + 1..];
                match after.find(']') {
                    Some(end) if is_chord(&after[..end]) => {
                        // A real chord starts the next segment; flush this one
                        // (even if its text is empty and it only carried the
                        // previous chord — `[G][C]` is two zero-width strikes).
                        if chord.is_some() || !text.is_empty() {
                            segs.push(Segment { chord: chord.take(), text: std::mem::take(&mut text) });
                        }
                        chord = Some(after[..end].to_string());
                        rest = &after[end + 1..];
                    }
                    Some(end) => {
                        // Not a chord: the brackets are lyrics.
                        text.push('[');
                        text.push_str(&after[..end]);
                        text.push(']');
                        rest = &after[end + 1..];
                    }
                    None => {
                        text.push('[');
                        text.push_str(after);
                        break;
                    }
                }
            }
        }
    }
    if chord.is_some() || !text.is_empty() || segs.is_empty() {
        segs.push(Segment { chord, text });
    }
    segs
}

/// Parse a whole stanza (lines joined with `\n`) into painted lines, with
/// every chord transposed by `semis` and spelled for a `flats` key.
pub fn stanza_lines(stanza: &str, semis: i32, flats: bool) -> Vec<Vec<Segment>> {
    stanza
        .split('\n')
        .map(|line| {
            let mut segs = parse_line(line);
            // At 0 semitones the chords pass through untouched — transposition
            // never rewrites the author's own spelling (F# stays F#).
            if semis != 0 {
                for s in &mut segs {
                    if let Some(c) = &s.chord {
                        s.chord = Some(transpose_chord(c, semis, flats));
                    }
                }
            }
            segs
        })
        .collect()
}

// ── chords and keys ───────────────────────────────────────────────────────────

/// Quality tokens the grammar accepts. Concatenations of these are valid
/// (`m7b5` is itself a token; `madd9` is `m` + `add9`). ORDER MATTERS: the
/// scan takes the first prefix match, so every token must come before its own
/// prefixes (`maj7` before `m`, `add9` before `9`, `dim7` before `dim`).
const QUALITIES: [&str; 23] = [
    "mmaj7", "7sus4", "add11", "add13", "m7b5", "maj7", "dim7", "aug7", "sus2", "sus4", "add9", "add2", "maj",
    "min", "dim", "aug", "m7", "11", "13", "m", "6", "7", "9",
];

/// Note letter → pitch class.
fn letter_pitch(c: u8) -> Option<i32> {
    Some(match c {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    })
}

/// Parse `A`, `F#`, `Bb` at the head of `s`: (pitch class, rest).
fn parse_root(s: &str) -> Option<(i32, &str)> {
    let mut pitch = letter_pitch(*s.as_bytes().first()?)?;
    let mut rest = &s[1..];
    match rest.as_bytes().first() {
        Some(b'#') => {
            pitch += 1;
            rest = &rest[1..];
        }
        Some(b'b') => {
            pitch -= 1;
            rest = &rest[1..];
        }
        _ => {}
    }
    Some((pitch.rem_euclid(12), rest))
}

/// Whether a quality string is built from the accepted tokens.
fn valid_quality(mut q: &str) -> bool {
    'outer: while !q.is_empty() {
        for t in QUALITIES {
            if let Some(rest) = q.strip_prefix(t) {
                q = rest;
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// Whether bracket content is a chord: root, optional quality, optional
/// `/bass`. This is the grammar `data-prep/hymnal/FORMAT.md` promises.
pub fn is_chord(s: &str) -> bool {
    let Some((_, rest)) = parse_root(s) else { return false };
    let (quality, bass) = match rest.split_once('/') {
        Some((q, b)) => (q, Some(b)),
        None => (rest, None),
    };
    if !valid_quality(quality) {
        return false;
    }
    match bass {
        Some(b) => matches!(parse_root(b), Some((_, ""))),
        None => true,
    }
}

const SHARPS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLATS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

fn spell(pitch: i32, flats: bool) -> &'static str {
    let table = if flats { &FLATS } else { &SHARPS };
    table[pitch.rem_euclid(12) as usize]
}

/// Transpose one chord by `semis` semitones, spelling roots with flats when
/// the TARGET key is a flat key. The quality is carried through untouched —
/// only the root and the slash bass move.
///
/// A non-chord comes back unchanged: `parse_line` never labels one as a chord,
/// so this is pure defence in depth.
pub fn transpose_chord(chord: &str, semis: i32, flats: bool) -> String {
    if !is_chord(chord) {
        return chord.to_string();
    }
    let (pitch, rest) = parse_root(chord).expect("is_chord verified the root");
    let (quality, bass) = match rest.split_once('/') {
        Some((q, b)) => (q, Some(b)),
        None => (rest, None),
    };
    let mut out = String::new();
    out.push_str(spell(pitch + semis, flats));
    out.push_str(quality);
    if let Some(b) = bass {
        let (bp, _) = parse_root(b).expect("is_chord verified the bass");
        out.push('/');
        out.push_str(spell(bp + semis, flats));
    }
    out
}

/// Whether `key` (a major tonic like `"Eb"`, or minor like `"Em"`) is spelled
/// with flats. Minor keys answer for their relative major: E minor carries
/// G major's one sharp.
pub fn key_uses_flats(key: &str) -> bool {
    let Some((pitch, rest)) = parse_root(key) else { return false };
    let major = if rest == "m" { pitch + 3 } else { pitch };
    matches!(major.rem_euclid(12), 1 | 3 | 5 | 6 | 8 | 10)
}

/// The key `semis` above `key`, spelled by its own signature; `"?"` never —
/// an unparseable key transposes to itself.
pub fn transpose_key(key: &str, semis: i32) -> String {
    let Some((pitch, rest)) = parse_root(key) else { return key.to_string() };
    if !(rest.is_empty() || rest == "m") {
        return key.to_string();
    }
    let target = pitch + semis;
    let minor = rest == "m";
    let flats = {
        let major = if minor { target + 3 } else { target };
        matches!(major.rem_euclid(12), 1 | 3 | 5 | 6 | 8 | 10)
    };
    let mut out = spell(target, flats).to_string();
    if minor {
        out.push('m');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
      "format": "hymnal-v1",
      "hymns": [
        {
          "id": "amazing-grace", "number": 14, "tune": "NEW BRITAIN",
          "meter": "8.6.8.6", "key": "G",
          "texts": {
            "en": {
              "title": "Amazing Grace", "author": "John Newton", "year": 1779,
              "stanzas": ["A[G]mazing grace! how [C]sweet the [G]sound,\nThat saved a wretch like [D]me!"],
              "chorus": null
            }
          }
        },
        {
          "id": "ein-feste-burg", "number": 3, "tune": "EIN FESTE BURG",
          "meter": "8.7.8.7.6.6.6.6.7", "key": "C",
          "texts": {
            "de": {
              "title": "Ein feste Burg ist unser Gott", "author": "Martin Luther",
              "stanzas": ["[C]Ein feste [G]Burg ist [C]unser Gott"], "chorus": null
            },
            "en": {
              "title": "A Mighty Fortress Is Our God", "author": "Martin Luther",
              "translator": "Frederick H. Hedge",
              "stanzas": ["[C]A mighty [G]fortress [C]is our God"], "chorus": null
            }
          }
        }
      ]
    }"##;

    #[test]
    fn loads_and_sorts_by_number() {
        let h = from_str(SAMPLE).unwrap();
        assert_eq!(h.hymns.len(), 2);
        assert_eq!(h.hymns[0].id, "ein-feste-burg"); // number 3 before 14
        assert_eq!(h.get("amazing-grace").unwrap().number, 14);
        let burg = h.get("ein-feste-burg").unwrap();
        assert_eq!(burg.texts.len(), 2);
        assert_eq!(burg.texts["en"].translator.as_deref(), Some("Frederick H. Hedge"));
    }

    #[test]
    fn wrong_format_tag_refused() {
        let err = from_str(r#"{"format":"hymnal-v2","hymns":[]}"#).unwrap_err();
        assert!(err.to_string().contains("hymnal-v2"));
    }

    #[test]
    fn missing_file_is_empty_not_error() {
        assert!(load("/no/such/hymnal.json").unwrap().hymns.is_empty());
    }

    #[test]
    fn parse_line_splits_at_chords() {
        let segs = parse_line("A[G]mazing grace! how [C7]sweet the [G]sound,");
        assert_eq!(
            segs,
            vec![
                Segment { chord: None, text: "A".into() },
                Segment { chord: Some("G".into()), text: "mazing grace! how ".into() },
                Segment { chord: Some("C7".into()), text: "sweet the ".into() },
                Segment { chord: Some("G".into()), text: "sound,".into() },
            ]
        );
    }

    #[test]
    fn plain_line_is_one_chordless_segment() {
        assert_eq!(parse_line("Was blind, but now I see."), vec![Segment {
            chord: None,
            text: "Was blind, but now I see.".into()
        }]);
        // The empty line still paints as a line.
        assert_eq!(parse_line(""), vec![Segment { chord: None, text: "".into() }]);
    }

    #[test]
    fn non_chord_brackets_stay_literal() {
        // "[Selah]" is lyrics, not a chord; an unclosed bracket likewise.
        let segs = parse_line("Sing praise [Selah] and [G]rejoice [always");
        assert_eq!(segs[0].text, "Sing praise [Selah] and ");
        assert_eq!(segs[1].chord.as_deref(), Some("G"));
        assert_eq!(segs[1].text, "rejoice [always");
    }

    #[test]
    fn back_to_back_chords_keep_both() {
        let segs = parse_line("[G][C]Go");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], Segment { chord: Some("G".into()), text: "".into() });
        assert_eq!(segs[1], Segment { chord: Some("C".into()), text: "Go".into() });
    }

    #[test]
    fn chord_grammar() {
        for ok in ["G", "F#", "Bb", "Em", "C7", "Gmaj7", "Dsus4", "Am7", "D/F#", "G7/B", "Cm7b5", "Baug7", "Fadd9", "C7sus4", "Emadd9"] {
            assert!(is_chord(ok), "{ok} should parse");
        }
        for bad in ["", "H", "Gx", "G#b", "Selah", "G/", "G/H", "Gmaj7x", "Gm7b5x", "7"] {
            assert!(!is_chord(bad), "{bad} should not parse");
        }
    }

    #[test]
    fn transposition_moves_root_and_bass_only() {
        assert_eq!(transpose_chord("G", 2, false), "A");
        assert_eq!(transpose_chord("Em7", 3, false), "Gm7");
        assert_eq!(transpose_chord("D/F#", -2, false), "C/E");
        // Spelling follows the target key's signature.
        assert_eq!(transpose_chord("G", 1, true), "Ab");
        assert_eq!(transpose_chord("G", 1, false), "G#");
        assert_eq!(transpose_chord("Bb7", 2, false), "C7");
    }

    #[test]
    fn transposition_round_trips_every_offset() {
        for semis in -11..=11 {
            let up = transpose_chord("Gmaj7", semis, key_uses_flats(&transpose_key("G", semis)));
            let back = transpose_chord(&up, -semis, false);
            assert_eq!(back, "Gmaj7", "offset {semis} went {up} then {back}");
        }
    }

    #[test]
    fn keys_spell_by_their_own_signature() {
        assert_eq!(transpose_key("G", 3), "Bb"); // not A#
        assert_eq!(transpose_key("G", -3), "E");
        assert_eq!(transpose_key("C", 6), "Gb"); // not F#: Gb major is the flat spelling
        assert_eq!(transpose_key("Em", 1), "Fm");
        assert!(key_uses_flats("F"));
        assert!(key_uses_flats("Dm")); // relative major F
        assert!(!key_uses_flats("Em")); // relative major G
        assert!(!key_uses_flats("A"));
    }

    #[test]
    fn stanza_lines_transpose_in_place() {
        let lines = stanza_lines("A[G]mazing [C]grace\nThat [D7]saved", 2, false);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][1].chord.as_deref(), Some("A"));
        assert_eq!(lines[0][2].chord.as_deref(), Some("D"));
        assert_eq!(lines[1][1].chord.as_deref(), Some("E7"));
        // Zero transposition keeps the author's spelling byte-for-byte.
        let same = stanza_lines("[F#]x", 0, true);
        assert_eq!(same[0][0].chord.as_deref(), Some("F#"));
    }
}
