//! The type axes: which face paints scripture, and which paints the chrome.
//!
//! Colour and type are independent axes — a theme picks the palette
//! ([`crate::theme`]), these pick the faces, and every combination is legal.
//! The core owns the vocabulary (which faces exist, what each can do); the font
//! files themselves are shell assets, because delivery is a platform concern.
//!
//! [`Font::has_italic`] is part of that vocabulary because the reader paints
//! translator-supplied words ([`crate::corpus::FLAG_ADDED`]) in the palette's
//! `added` tone AND in italic. A face without a real italic never gets a
//! synthesised one; the tone carries the distinction alone.

use crate::i18n::Script;
use serde::{Deserialize, Serialize};

/// A face the reader can choose, for either type axis.
///
/// Every variant is bundled and under the SIL Open Font License — type is never
/// fetched from a network, and nothing here encumbers a redistributable build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Font {
    /// The shipped default on both axes: Octavio Pardo's Garamond revival.
    #[default]
    EbGaramond,
    /// A contemporary reading serif drawn for screens.
    Literata,
    /// A neutral, tightly-fitted UI sans.
    Inter,
    /// A monospace. The only Latin face with no italic (see the module note).
    FiraCode,
    /// The Braille Institute's low-vision face: exaggerated letterform
    /// distinctions (unambiguous I/l/1, slashed zero). The accessibility option.
    AtkinsonHyperlegible,
    /// The naskh face, and the only bundled face with an Arabic glyph.
    ///
    /// Bundled for everyone, offered only to Arabic readers ([`Font::offered_for`]).
    /// It sits in every other family's CSS fallback stack on the web, so nothing
    /// ever renders from a system font the engine did not measure with.
    Amiri,
    /// The Gurmukhi face, and the only bundled face with a Gurmukhi glyph.
    /// Serif because this is a Bible, though Gurmukhi has no serif tradition of
    /// its own.
    NotoSerifGurmukhi,
    /// The Devanagari face — and the face any other Devanagari language would
    /// read, since [`Font::script`] makes the script the unit, not the language.
    NotoSerifDevanagari,
    /// The Han face, serving both Chinese rows: traditional and simplified are
    /// repertoires of one script, and the shipped subset covers both corpora and
    /// both catalogues (asserted by the subsetter). The TC cut is deliberate —
    /// the 1919 和合本 is a traditional-character text.
    NotoSerifTC,
}

impl Font {
    /// The wire/config token. Frozen once written into a reader's config.
    pub fn token(self) -> &'static str {
        match self {
            Font::EbGaramond => "eb-garamond",
            Font::Literata => "literata",
            Font::Inter => "inter",
            Font::FiraCode => "fira-code",
            Font::AtkinsonHyperlegible => "atkinson-hyperlegible",
            Font::Amiri => "amiri",
            Font::NotoSerifGurmukhi => "noto-serif-gurmukhi",
            Font::NotoSerifDevanagari => "noto-serif-devanagari",
            Font::NotoSerifTC => "noto-serif-tc",
        }
    }

    /// A token back to a face; `None` for anything this build does not ship.
    pub fn parse(t: &str) -> Option<Font> {
        match t {
            "eb-garamond" => Some(Font::EbGaramond),
            "literata" => Some(Font::Literata),
            "inter" => Some(Font::Inter),
            "fira-code" => Some(Font::FiraCode),
            "atkinson-hyperlegible" => Some(Font::AtkinsonHyperlegible),
            "amiri" => Some(Font::Amiri),
            "noto-serif-gurmukhi" => Some(Font::NotoSerifGurmukhi),
            "noto-serif-devanagari" => Some(Font::NotoSerifDevanagari),
            "noto-serif-tc" => Some(Font::NotoSerifTC),
            _ => None,
        }
    }

    /// The face's own name, as its designers spell it. Deliberately not in the
    /// i18n catalogue — a typeface name is a proper noun in every language.
    pub fn name(self) -> &'static str {
        match self {
            Font::EbGaramond => "EB Garamond",
            Font::Literata => "Literata",
            Font::Inter => "Inter",
            Font::FiraCode => "Fira Code",
            Font::AtkinsonHyperlegible => "Atkinson Hyperlegible",
            Font::Amiri => "Amiri",
            Font::NotoSerifGurmukhi => "Noto Serif Gurmukhi",
            Font::NotoSerifDevanagari => "Noto Serif Devanagari",
            Font::NotoSerifTC => "Noto Serif TC",
        }
    }

    /// Whether this face ships a real italic. False means added words are told
    /// apart by the palette's `added` tone alone — see the module note.
    pub fn has_italic(self) -> bool {
        // The non-Latin faces bundle no italic: none of those scripts has an
        // italic tradition, and none of their corpora mark a supplied word.
        !matches!(
            self,
            Font::FiraCode | Font::Amiri | Font::NotoSerifGurmukhi | Font::NotoSerifDevanagari | Font::NotoSerifTC
        )
    }

    /// Whether this face ships a real bold as separate static files rather than
    /// a `wght` axis. The variable families cover 400–700 in one file; Atkinson
    /// Hyperlegible does not, and the web's @font-face must say so — a static
    /// 400 declared `font-weight: 400 700` paints bold text regular.
    pub fn static_bold(self) -> bool {
        matches!(self, Font::AtkinsonHyperlegible)
    }

    /// The script this face can set — a column, like
    /// [`crate::i18n::LangSpec::script`]; [`Font::offered_for`] reads the two
    /// together, so adding a script means adding a face and a row, not a
    /// condition.
    pub fn script(self) -> Script {
        match self {
            Font::EbGaramond | Font::Literata | Font::Inter | Font::FiraCode | Font::AtkinsonHyperlegible => {
                Script::Latin
            }
            Font::Amiri => Script::Arabic,
            Font::NotoSerifGurmukhi => Script::Gurmukhi,
            Font::NotoSerifDevanagari => Script::Devanagari,
            Font::NotoSerifTC => Script::Han,
        }
    }

    /// Whether this face is offered to a reader of `lang`. Pickers must filter
    /// on this: a face that cannot set the reader's script would fall back
    /// per-glyph to the one that can, leaving the choice to change only the
    /// size (`scale()` is applied from the selected token) — a mislabelled
    /// slider. The test is the script, not direction: Gurmukhi and Devanagari
    /// read left to right and still have no Latin face.
    pub fn offered_for(self, lang: crate::i18n::Lang) -> bool {
        self.script() == lang.script()
    }

    /// The faces offered to a reader of `lang`, in picker order.
    pub fn all_for(lang: crate::i18n::Lang) -> Vec<Font> {
        Font::ALL.into_iter().filter(|f| f.offered_for(lang)).collect()
    }

    /// The face's optical size multiplier: a shell multiplies the reader's
    /// chosen px size by this before measuring or painting, so switching faces
    /// changes the voice of the text without changing its apparent size.
    ///
    /// x-heights as a fraction of the em, measured from the shipped files:
    /// Garamond 0.400, Literata 0.507, Fira Code 0.525, Inter 0.546. These
    /// numbers are a HALF correction toward equal x-height — full equalisation
    /// would render Inter at 13.2px when the slider says 18.
    ///
    /// Render-time only: never written into `bodySize` (the stored size would
    /// drift on every face switch), and the default face is exactly 1.0.
    pub fn scale(self) -> f32 {
        match self {
            Font::EbGaramond => 1.0,
            Font::Literata => 0.89,
            Font::Inter => 0.87,
            Font::FiraCode => 0.88,
            // x-height 0.496, a hair under Literata's.
            Font::AtkinsonHyperlegible => 0.90,
            // Arabic has no x-height; the analogue is the baseline body of ه and
            // د, mean 0.361 em — smaller than Garamond's, so this is the one
            // face corrected upward.
            Font::Amiri => 1.06,
            // Devanagari and Gurmukhi: the analogue is the base-consonant body,
            // baseline to headstroke, 0.623 and 0.622 em (क स न प / ਕ ਸ ਨ ਪ) —
            // within a thousandth, so one number serves both. Deliberately NOT
            // the OS/2 sxHeight these files carry: that field measures a Noto
            // face's Latin subset, and for Gurmukhi it is off by a sixth of an em.
            Font::NotoSerifGurmukhi | Font::NotoSerifDevanagari => 0.82,
            // Han is where the half-correction runs out: an ideograph fills its
            // em box, which is not an x-height analogue, and mixed CJK/Latin
            // setting conventionally uses equal point size. Halving toward
            // Garamond would land at 0.73 and read far too small, so this is a
            // light trim from parity instead.
            Font::NotoSerifTC => 0.95,
        }
    }

    /// Every face in picker order, default first. The one list a shell
    /// enumerates, so a new variant cannot be forgotten.
    pub const ALL: [Font; 9] = [
        Font::EbGaramond,
        Font::Literata,
        Font::Inter,
        Font::FiraCode,
        Font::AtkinsonHyperlegible,
        Font::Amiri,
        Font::NotoSerifGurmukhi,
        Font::NotoSerifDevanagari,
        Font::NotoSerifTC,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip() {
        for f in Font::ALL {
            assert_eq!(Font::parse(f.token()), Some(f), "{} does not round-trip", f.name());
        }
    }

    #[test]
    fn an_unknown_token_is_none_rather_than_a_default() {
        // The caller decides what to do with a face it does not ship; a quiet
        // Garamond would make a typo indistinguishable from a real choice.
        assert_eq!(Font::parse("helvetica"), None);
        assert_eq!(Font::parse(""), None);
        assert_eq!(Font::parse("EB Garamond"), None, "the NAME is not the token");
    }

    #[test]
    fn every_variant_is_in_all() {
        // A variant missing from ALL is a face the reader can hold in their
        // config and never pick in the UI.
        for f in [
            Font::EbGaramond,
            Font::Literata,
            Font::Inter,
            Font::FiraCode,
            Font::AtkinsonHyperlegible,
            Font::Amiri,
            Font::NotoSerifGurmukhi,
            Font::NotoSerifDevanagari,
            Font::NotoSerifTC,
        ] {
            assert!(Font::ALL.contains(&f), "{} is missing from Font::ALL", f.name());
        }
        assert_eq!(Font::ALL.len(), 9);
    }

    /// An empty picker falls the shell back to the default token, so the text
    /// would be measured with Garamond and painted with whatever the system
    /// found for the script — the two contexts disagreeing, which mis-wraps.
    #[test]
    fn every_shipped_language_has_at_least_one_face() {
        for lang in crate::i18n::Lang::ALL {
            let offered = Font::all_for(lang);
            assert!(!offered.is_empty(), "{} is offered no face at all", lang.code());
            for f in &offered {
                assert_eq!(f.script(), lang.script(), "{} is offered {}", lang.code(), f.name());
            }
        }
    }

    /// The converse: no face is offered to a language it cannot set. Stated over
    /// the whole cross product, so a language added later is already covered.
    #[test]
    fn a_face_is_offered_exactly_where_it_can_set_the_text() {
        for f in Font::ALL {
            for lang in crate::i18n::Lang::ALL {
                assert_eq!(f.offered_for(lang), f.script() == lang.script(), "{} for {}", f.name(), lang.code());
            }
        }
        // Concrete anchors: the rule above is also satisfied by a broken
        // `script()` answering the same wrong thing on both sides.
        assert!(Font::NotoSerifGurmukhi.offered_for(crate::i18n::Lang::Pa));
        assert!(!Font::NotoSerifGurmukhi.offered_for(crate::i18n::Lang::Hi));
        assert!(!Font::EbGaramond.offered_for(crate::i18n::Lang::Pa));
        assert!(!Font::Amiri.offered_for(crate::i18n::Lang::Hi));
    }

    /// Exactly one face per non-Latin script: the web puts these in every
    /// family's CSS fallback stack, and two faces for one script means the
    /// engine worker and the document can pick differently for a codepoint.
    #[test]
    fn each_non_latin_script_has_exactly_one_face() {
        for script in [Script::Arabic, Script::Gurmukhi, Script::Devanagari, Script::Han] {
            let n = Font::ALL.iter().filter(|f| f.script() == script).count();
            assert_eq!(n, 1, "{script:?} has {n} faces");
        }
    }

    #[test]
    fn tokens_and_names_are_distinct() {
        let mut tokens: Vec<&str> = Font::ALL.iter().map(|f| f.token()).collect();
        let mut names: Vec<&str> = Font::ALL.iter().map(|f| f.name()).collect();
        tokens.sort_unstable();
        tokens.dedup();
        names.sort_unstable();
        names.dedup();
        assert_eq!(tokens.len(), Font::ALL.len(), "two faces share a token");
        assert_eq!(names.len(), Font::ALL.len(), "two faces share a name");
    }

    #[test]
    fn the_default_is_the_shipped_face() {
        assert_eq!(Font::default(), Font::EbGaramond);
    }

    #[test]
    fn the_default_face_scales_by_exactly_one() {
        // The shipped default is the baseline every other face is corrected
        // toward, so the correction is invisible to a reader who never picks.
        assert_eq!(Font::default().scale(), 1.0);
    }

    #[test]
    fn scales_are_a_partial_correction_not_an_equalisation() {
        // Every scale must stay strictly inside the full-equalisation ratios, in
        // both directions: Garamond's 0.400 over the Indic base-consonant body
        // (0.622) is ~0.64 below, and over Amiri's 0.361 baseline body ~1.12
        // above. Reaching either would mean the size slider stopped meaning
        // anything — Inter at full correction renders 13.2px when it says 18.
        for f in Font::ALL {
            let s = f.scale();
            assert!(s > 0.64 && s < 1.13, "{} scales by {}", f.name(), s);
        }
    }

    #[test]
    fn only_the_latin_reading_faces_carry_an_italic() {
        // A shell asks this before styling added words; a wrong answer either
        // loses the KJV's italics or asks a font for a face it does not have.
        for f in [Font::FiraCode, Font::Amiri, Font::NotoSerifGurmukhi, Font::NotoSerifDevanagari, Font::NotoSerifTC] {
            assert!(!f.has_italic(), "{} ships no italic", f.name());
        }
        for f in [Font::EbGaramond, Font::Literata, Font::Inter, Font::AtkinsonHyperlegible] {
            assert!(f.has_italic(), "{} should have an italic", f.name());
        }
    }

    #[test]
    fn only_atkinson_carries_a_static_bold() {
        // Wrong either way: a static declared as a range paints bold text
        // regular, and a variable family shipped as two files wastes one.
        assert!(Font::AtkinsonHyperlegible.static_bold());
        for f in [
            Font::EbGaramond,
            Font::Literata,
            Font::Inter,
            Font::FiraCode,
            Font::NotoSerifGurmukhi,
            Font::NotoSerifDevanagari,
        ] {
            assert!(!f.static_bold(), "{} is a variable family", f.name());
        }
    }
}
