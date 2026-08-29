//! The type axes: which face paints scripture, and which paints the chrome.
//!
//! Colour and type are INDEPENDENT. A theme picks the palette
//! ([`crate::theme`]); these pick the faces, and every combination is legal —
//! there is no pairing table anywhere, on purpose. A reader who wants Fira Code
//! scripture under Synthwave, or Garamond scripture under a system-font UI, is
//! not choosing something exotic; they are choosing two of three orthogonal
//! things.
//!
//! ## What the core owns, and what it does not
//!
//! The core owns the VOCABULARY — the tokens, which faces exist, and what each
//! one can do — so the two shells cannot drift on the list the way they once did
//! on themes. It owns no font FILES: those are shell assets (a `.ttf` in the
//! APK, a subset `.woff2` on the web), because the delivery of a font is a
//! platform concern and the two shells do it very differently.
//!
//! ## Why [`Font::has_italic`] is part of the vocabulary
//!
//! The reader paints translator-supplied words ([`crate::corpus::FLAG_ADDED`],
//! the KJV's italics) in the palette's `added` tone AND in italic. The tone is
//! always available; the italic is not — Fira Code ships no italic face at all.
//! Synthesising one (a shear applied to the upright) is the kind of thing that
//! looks exactly like what it is, so a face without an italic simply does not
//! get one, and the `added` tone carries the distinction by itself. That is a
//! property of the FACE, so it belongs here rather than being rediscovered by
//! each shell.

use crate::i18n::Script;
use serde::{Deserialize, Serialize};

/// A face the reader can choose, for either type axis.
///
/// Every variant is bundled by both shells and is under the SIL Open Font
/// License — a reader is never sent to a network for type, and nothing here
/// encumbers a redistributable build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Font {
    /// The shipped default on both axes: Claude Garamont's 16th-century
    /// humanist letter, as Octavio Pardo's revival. A printed-Bible face.
    #[default]
    EbGaramond,
    /// A contemporary reading serif drawn for screens — a taller x-height and
    /// sturdier stems than Garamond at the same size.
    Literata,
    /// A neutral, tightly-fitted UI sans.
    Inter,
    /// A monospace. The only bundled face with NO italic (see the module note).
    FiraCode,
    /// The Braille Institute's low-vision face: exaggerated letterform
    /// distinctions (unambiguous I/l/1, footed capitals, a slashed zero) so
    /// characters cannot be mistaken for one another. The accessibility
    /// option, and it pairs naturally with the High Contrast theme — though,
    /// as ever, type and colour are independent axes.
    AtkinsonHyperlegible,
    /// The naskh face that carries Arabic, and the ONLY bundled face that
    /// contains a single Arabic glyph.
    ///
    /// Bundled for everyone and offered to nobody who is not reading Arabic —
    /// see [`Font::offered_for`]. It is in every other family's CSS fallback
    /// stack on the web, so a reader on Garamond gets Garamond for English and
    /// Amiri for Arabic out of one stack and nothing ever renders from a system
    /// font the engine did not measure with.
    Amiri,
    /// The Gurmukhi face, and the only bundled face with a Gurmukhi glyph.
    /// Serif because this is a Bible: Gurmukhi has no serif tradition of its
    /// own, and Noto Serif Gurmukhi is the closest thing to the weight and
    /// finish of a printed ਪਵਿੱਤਰ ਬਾਈਬਲ.
    NotoSerifGurmukhi,
    /// The Devanagari face. Also the face Marathi and Urdu-Devanagari would
    /// read, if either is ever added — the script is the unit here, not the
    /// language, which is the whole reason [`Font::script`] exists.
    NotoSerifDevanagari,
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
            _ => None,
        }
    }

    /// The face's own name, as its designers spell it.
    ///
    /// NOT translated and not in the i18n catalogue: a typeface name is a proper
    /// noun, and "Fira Code" is "Fira Code" in every language the app will ever
    /// speak. The shells label the pickers with this.
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
        }
    }

    /// Whether this face ships a real italic. False means added words are told
    /// apart by the palette's `added` tone alone — see the module note.
    pub fn has_italic(self) -> bool {
        // Amiri ships an italic, and Arabic has no italic tradition to use it
        // for — the Van Dyck marks no translator-supplied words, so the axis
        // this exists for is empty in Arabic anyway. Not bundled.
        //
        // Same for the two Indic faces, and for the same reason twice over:
        // neither ships an italic, and neither corpus marks a supplied word.
        !matches!(self, Font::FiraCode | Font::Amiri | Font::NotoSerifGurmukhi | Font::NotoSerifDevanagari)
    }

    /// Whether this face ships a REAL bold, as separate static files rather
    /// than a `wght` axis. The four original families are variable fonts (one
    /// file covers 400–700); Atkinson Hyperlegible is static, so its bold is
    /// its own file and the web's @font-face declarations must say so — a
    /// static 400 declared as `font-weight: 400 700` would paint bold text
    /// regular. Shell-informative, like [`Font::has_italic`].
    pub fn static_bold(self) -> bool {
        matches!(self, Font::AtkinsonHyperlegible)
    }

    /// The script this face can set. The five original families are Latin;
    /// each of the other three exists because no Latin face has a glyph of its
    /// script.
    ///
    /// A COLUMN, like [`crate::i18n::LangSpec::script`], and the two are read
    /// together by [`Font::offered_for`]. Adding a script means adding a face
    /// and a row, not editing a condition.
    pub fn script(self) -> Script {
        match self {
            Font::EbGaramond | Font::Literata | Font::Inter | Font::FiraCode | Font::AtkinsonHyperlegible => {
                Script::Latin
            }
            Font::Amiri => Script::Arabic,
            Font::NotoSerifGurmukhi => Script::Gurmukhi,
            Font::NotoSerifDevanagari => Script::Devanagari,
        }
    }

    /// Whether this face is offered to a reader of `lang`.
    ///
    /// THE PICKERS MUST FILTER ON THIS. Amiri is the only bundled face with any
    /// Arabic in it, so offering an Arabic reader the other five is offering
    /// five ways to read nothing: per-glyph fallback would render their
    /// scripture in Amiri regardless, and the only thing their choice would
    /// actually change is the SIZE — `scale()` is applied from the selected
    /// token, so picking Inter would render Amiri at 0.87, a ratio calibrated
    /// to Inter's x-height and meaningless for naskh. The picker would be a
    /// mislabelled size slider.
    ///
    /// And the converse: Amiri has Latin, but it is a naskh face whose Latin is
    /// not why anyone chose it, so it stays out of the Latin pickers.
    ///
    /// THIS USED TO ASK `is_rtl()`, and it was right for exactly as long as
    /// Arabic was the only non-Latin language: "reads right to left" and
    /// "cannot be set in a Latin face" had the same answer. Gurmukhi and
    /// Devanagari are left to right and no Latin face has a glyph of either, so
    /// direction was never the question — the script was.
    pub fn offered_for(self, lang: crate::i18n::Lang) -> bool {
        self.script() == lang.script()
    }

    /// The faces offered to a reader of `lang`, in picker order.
    pub fn all_for(lang: crate::i18n::Lang) -> Vec<Font> {
        Font::ALL.into_iter().filter(|f| f.offered_for(lang)).collect()
    }

    /// The face's optical size multiplier: what a shell multiplies the reader's
    /// chosen px size by before measuring or painting this face, so switching
    /// faces changes the voice of the text without changing its apparent size.
    ///
    /// The bundled faces have very different x-heights (as a fraction of the
    /// em, measured from the shipped files): EB Garamond 0.400, Literata 0.507,
    /// Fira Code 0.525, Inter 0.546 — so at the same px size Inter reads over a
    /// third larger than Garamond. These are a HALF correction toward equal
    /// x-height, not the full one: full equalisation would render Inter at
    /// 13.2px when the slider says 18, which reads as the app ignoring the
    /// setting.
    ///
    /// A RENDER-TIME factor only. It is never written into `bodySize` — mutating
    /// the stored setting would make the reader's size drift on every face
    /// switch — and the default face is exactly 1.0, so nothing moves for a
    /// reader who never opens the picker.
    pub fn scale(self) -> f32 {
        match self {
            Font::EbGaramond => 1.0,
            Font::Literata => 0.89,
            Font::Inter => 0.87,
            Font::FiraCode => 0.88,
            // x-height 0.496 (OS/2 sxHeight 496 / 1000 em, measured from the
            // shipped file) — all but Literata's 0.507, so the same half
            // correction lands one point higher.
            Font::AtkinsonHyperlegible => 0.90,
            // Arabic has no x-height, so the analogue is the body height of the
            // letters that sit ON the baseline without descending: ه and د,
            // mean 0.361 em, measured from the shipped file the same way as the
            // others. Against Garamond's 0.405 that is a full correction of
            // 1.123, and the same half correction lands here.
            Font::Amiri => 1.06,
            // Devanagari and Gurmukhi have no x-height either. The analogue is
            // the BODY OF A BASE CONSONANT — baseline to headstroke, where the
            // matras attach — measured from the shipped files the same way as
            // Amiri's: क स न प and ਕ ਸ ਨ ਪ, 0.623 and 0.622 em, within a
            // thousandth of each other, which is why one number serves both.
            // Against Garamond's 0.400 that is a full correction of 0.64, the
            // largest on this list in either direction, and the same half
            // correction lands here.
            //
            // NOT the OS/2 sxHeight these two files carry (0.623 and 0.536):
            // that field measures a Noto face's LATIN subset, which is not the
            // script anyone selects it for, and for Gurmukhi it disagrees with
            // the letters by a sixth of an em.
            Font::NotoSerifGurmukhi | Font::NotoSerifDevanagari => 0.82,
        }
    }

    /// Every face, in the order the pickers offer them: the default first, then
    /// the alternatives. One list, so a face added to the enum cannot be
    /// forgotten by a shell — [`tests::every_variant_is_in_all`] holds it.
    pub const ALL: [Font; 8] = [
        Font::EbGaramond,
        Font::Literata,
        Font::Inter,
        Font::FiraCode,
        Font::AtkinsonHyperlegible,
        Font::Amiri,
        Font::NotoSerifGurmukhi,
        Font::NotoSerifDevanagari,
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
        // The caller decides what to do with a face it does not ship (config
        // keeps the reader's default); parse must not quietly answer Garamond,
        // or a typo would be indistinguishable from a deliberate choice.
        assert_eq!(Font::parse("helvetica"), None);
        assert_eq!(Font::parse(""), None);
        assert_eq!(Font::parse("EB Garamond"), None, "the NAME is not the token");
    }

    #[test]
    fn every_variant_is_in_all() {
        // ALL is what both shells enumerate. A variant missing from it is a face
        // the reader can hold in their config and never pick in the UI.
        for f in [
            Font::EbGaramond,
            Font::Literata,
            Font::Inter,
            Font::FiraCode,
            Font::AtkinsonHyperlegible,
            Font::Amiri,
            Font::NotoSerifGurmukhi,
            Font::NotoSerifDevanagari,
        ] {
            assert!(Font::ALL.contains(&f), "{} is missing from Font::ALL", f.name());
        }
        assert_eq!(Font::ALL.len(), 8);
    }

    /// EVERY LANGUAGE MUST BE OFFERED A FACE, and this is the test the `is_rtl`
    /// version of `offered_for` would have failed.
    ///
    /// A reader whose picker is empty has no way to set a size that means
    /// anything and no way to change the voice of their text; worse, the shells
    /// fall back to the default token, so a Punjabi reader would be measured
    /// with Garamond and painted with whatever the system found for Gurmukhi —
    /// the two contexts disagreeing, which is the wrapping bug the subsetter's
    /// header is about.
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

    /// And the converse: no face is offered to a language it cannot set.
    ///
    /// Stated over the whole cross product rather than as a list of which
    /// languages get which faces, so a language added tomorrow is covered by
    /// the test written today.
    #[test]
    fn a_face_is_offered_exactly_where_it_can_set_the_text() {
        for f in Font::ALL {
            for lang in crate::i18n::Lang::ALL {
                assert_eq!(f.offered_for(lang), f.script() == lang.script(), "{} for {}", f.name(), lang.code());
            }
        }
        // The concrete anchor, because the rule above is satisfied by a broken
        // `script()` that answers the same wrong thing on both sides.
        assert!(Font::NotoSerifGurmukhi.offered_for(crate::i18n::Lang::Pa));
        assert!(!Font::NotoSerifGurmukhi.offered_for(crate::i18n::Lang::Hi));
        assert!(!Font::EbGaramond.offered_for(crate::i18n::Lang::Pa));
        assert!(!Font::Amiri.offered_for(crate::i18n::Lang::Hi));
    }

    /// Exactly one face per non-Latin script, and it is not a tidiness rule.
    ///
    /// The web bundles the script faces UNCONDITIONALLY — they sit in every
    /// family's CSS fallback stack so the engine worker and the document agree
    /// on what renders a codepoint no Latin face has. Two faces for one script
    /// means two answers to "which one is in the stack", and the two contexts
    /// can pick differently.
    #[test]
    fn each_non_latin_script_has_exactly_one_face() {
        for script in [Script::Arabic, Script::Gurmukhi, Script::Devanagari] {
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
        // The optical correction must be invisible to a reader who never opens
        // the picker: the shipped default IS the baseline the other faces are
        // corrected toward.
        assert_eq!(Font::default().scale(), 1.0);
    }

    #[test]
    fn scales_are_a_partial_correction_not_an_equalisation() {
        // The correction moves a face TOWARD Garamond's apparent size and
        // stops half way; it never reaches parity, in either direction, because
        // a full equalisation would render Inter at 13.2px when the slider says
        // 18 and the setting would stop meaning anything.
        //
        // Both directions, and that is new: every Latin face here has a taller
        // x-height than Garamond and so corrects DOWN, which made "< 1.0" look
        // like the invariant until Amiri arrived. Arabic has no x-height at all
        // — the analogue is the baseline body of ه and د, and Amiri's is
        // SMALLER than Garamond's x-height, so it is the first face corrected
        // UP. The floor and ceiling are the full-equalisation ratios: Garamond
        // 0.400 over Inter's 0.546 is ~0.73 below, and over Amiri's 0.361 is
        // ~1.12 above.
        //
        // The floor moved when the Indic faces arrived: Garamond's 0.400 over
        // their 0.622 base-consonant body is ~0.64, the largest full correction
        // on this list in either direction, and the half of it (0.82) has to sit
        // strictly above it.
        for f in Font::ALL {
            let s = f.scale();
            assert!(s > 0.64 && s < 1.13, "{} scales by {}", f.name(), s);
        }
    }

    #[test]
    fn only_the_latin_reading_faces_carry_an_italic() {
        // Pinned rather than assumed: a shell asks this before it styles added
        // words, and a wrong answer either loses the KJV's italics or asks a
        // font for a face it does not have.
        for f in [Font::FiraCode, Font::Amiri, Font::NotoSerifGurmukhi, Font::NotoSerifDevanagari] {
            assert!(!f.has_italic(), "{} ships no italic", f.name());
        }
        for f in [Font::EbGaramond, Font::Literata, Font::Inter, Font::AtkinsonHyperlegible] {
            assert!(f.has_italic(), "{} should have an italic", f.name());
        }
    }

    #[test]
    fn only_atkinson_carries_a_static_bold() {
        // The variable families cover 400–700 in one file; Atkinson's bold is
        // its own file. A shell that gets this wrong either paints bold text
        // regular (static declared as a range) or ships a file it never uses.
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
