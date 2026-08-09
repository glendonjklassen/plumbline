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
}

impl Font {
    /// The wire/config token. Frozen once written into a reader's config.
    pub fn token(self) -> &'static str {
        match self {
            Font::EbGaramond => "eb-garamond",
            Font::Literata => "literata",
            Font::Inter => "inter",
            Font::FiraCode => "fira-code",
        }
    }

    /// A token back to a face; `None` for anything this build does not ship.
    pub fn parse(t: &str) -> Option<Font> {
        match t {
            "eb-garamond" => Some(Font::EbGaramond),
            "literata" => Some(Font::Literata),
            "inter" => Some(Font::Inter),
            "fira-code" => Some(Font::FiraCode),
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
        }
    }

    /// Whether this face ships a real italic. False means added words are told
    /// apart by the palette's `added` tone alone — see the module note.
    pub fn has_italic(self) -> bool {
        !matches!(self, Font::FiraCode)
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
        }
    }

    /// Every face, in the order the pickers offer them: the default first, then
    /// the alternatives. One list, so a face added to the enum cannot be
    /// forgotten by a shell — [`tests::every_variant_is_in_all`] holds it.
    pub const ALL: [Font; 4] = [Font::EbGaramond, Font::Literata, Font::Inter, Font::FiraCode];
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
        for f in [Font::EbGaramond, Font::Literata, Font::Inter, Font::FiraCode] {
            assert!(Font::ALL.contains(&f), "{} is missing from Font::ALL", f.name());
        }
        assert_eq!(Font::ALL.len(), 4);
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
        // Every non-default face is corrected DOWN (all have taller x-heights
        // than Garamond) but never below the full-equalisation floor —
        // Garamond's 0.400 x-height over Inter's 0.546 is ~0.73, and a scale at
        // or under it would mean the slider's number stops meaning anything.
        for f in Font::ALL {
            let s = f.scale();
            assert!(s > 0.73 && s <= 1.0, "{} scales by {}", f.name(), s);
        }
    }

    #[test]
    fn only_fira_code_lacks_an_italic() {
        // Pinned rather than assumed: a shell asks this before it styles added
        // words, and a wrong answer either loses the KJV's italics or asks a
        // font for a face it does not have.
        assert!(!Font::FiraCode.has_italic());
        for f in [Font::EbGaramond, Font::Literata, Font::Inter] {
            assert!(f.has_italic(), "{} should have an italic", f.name());
        }
    }
}
