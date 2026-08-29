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
        }
    }

    /// Whether this face ships a real italic. False means added words are told
    /// apart by the palette's `added` tone alone — see the module note.
    pub fn has_italic(self) -> bool {
        // Amiri ships an italic, and Arabic has no italic tradition to use it
        // for — the Van Dyck marks no translator-supplied words, so the axis
        // this exists for is empty in Arabic anyway. Not bundled.
        !matches!(self, Font::FiraCode | Font::Amiri)
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
    pub fn offered_for(self, lang: crate::i18n::Lang) -> bool {
        matches!(self, Font::Amiri) == lang.is_rtl()
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
        }
    }

    /// Every face, in the order the pickers offer them: the default first, then
    /// the alternatives. One list, so a face added to the enum cannot be
    /// forgotten by a shell — [`tests::every_variant_is_in_all`] holds it.
    pub const ALL: [Font; 6] =
        [Font::EbGaramond, Font::Literata, Font::Inter, Font::FiraCode, Font::AtkinsonHyperlegible, Font::Amiri];
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
        for f in
            [Font::EbGaramond, Font::Literata, Font::Inter, Font::FiraCode, Font::AtkinsonHyperlegible, Font::Amiri]
        {
            assert!(Font::ALL.contains(&f), "{} is missing from Font::ALL", f.name());
        }
        assert_eq!(Font::ALL.len(), 6);
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
        for f in Font::ALL {
            let s = f.scale();
            assert!(s > 0.73 && s < 1.13, "{} scales by {}", f.name(), s);
        }
    }

    #[test]
    fn only_fira_code_lacks_an_italic() {
        // Pinned rather than assumed: a shell asks this before it styles added
        // words, and a wrong answer either loses the KJV's italics or asks a
        // font for a face it does not have.
        assert!(!Font::FiraCode.has_italic());
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
        for f in [Font::EbGaramond, Font::Literata, Font::Inter, Font::FiraCode] {
            assert!(!f.static_bold(), "{} is a variable family", f.name());
        }
    }
}
