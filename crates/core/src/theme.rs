//! The shared colour palette — the one place every shell's colours are defined,
//! so light/dark/night can't drift between GTK, WinUI, and (later) Compose.
//! Tier 0 #5.
//!
//! A [`Palette`] carries a hex string per semantic role. The reader, chrome, and
//! the study-panel's [`crate::panel::Color`] roles all resolve through it. GTK
//! reads the struct directly; the non-Rust shells fetch it as JSON
//! (`plumbline_theme_palette_json`) and apply it. Shells own translucency: a search
//! band, a Strong's underline, a weave connector, a highlight wash are all drawn
//! by applying alpha to `gold` / `pin` / a highlight tone, so those follow the
//! theme for free.
//!
//! The light values are exactly the ones the shells shipped with, so switching
//! to the palette does not change the light look. Dark and night are new and
//! meant to be tuned in the reader (the maintainer owns the final values).

use serde::{Deserialize, Serialize};

/// A concrete, resolved theme (what actually paints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    Night,
}

impl Theme {
    pub fn token(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::Night => "night",
        }
    }
    pub fn parse(t: &str) -> Option<Theme> {
        match t {
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            "night" => Some(Theme::Night),
            _ => None,
        }
    }
    /// Whether the system chrome (scrollbars, dialogs) should be dark.
    pub fn is_dark(self) -> bool {
        !matches!(self, Theme::Light)
    }
}

/// What the *user* chose. `System` follows the OS light/dark preference; the
/// shell resolves it to a concrete [`Theme`] (choosing `Dark`, not `Night`, for
/// a dark system — night is an explicit opt-in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
    Night,
}

impl ThemeChoice {
    pub fn token(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
            ThemeChoice::Night => "night",
        }
    }
    pub fn parse(t: &str) -> Option<ThemeChoice> {
        match t {
            "system" => Some(ThemeChoice::System),
            "light" => Some(ThemeChoice::Light),
            "dark" => Some(ThemeChoice::Dark),
            "night" => Some(ThemeChoice::Night),
            _ => None,
        }
    }
    /// Resolve to a concrete theme; `System` uses `system_dark`.
    pub fn resolve(self, system_dark: bool) -> Theme {
        match self {
            ThemeChoice::Light => Theme::Light,
            ThemeChoice::Dark => Theme::Dark,
            ThemeChoice::Night => Theme::Night,
            ThemeChoice::System => {
                if system_dark {
                    Theme::Dark
                } else {
                    Theme::Light
                }
            }
        }
    }
    /// The next choice when cycling the header toggle (light → dark → night →
    /// system → light).
    pub fn next(self) -> ThemeChoice {
        match self {
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::Night,
            ThemeChoice::Night => ThemeChoice::System,
            ThemeChoice::System => ThemeChoice::Light,
        }
    }
    /// A short human label for the toggle button.
    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Theme: system",
            ThemeChoice::Light => "Theme: light",
            ThemeChoice::Dark => "Theme: dark",
            ThemeChoice::Night => "Theme: night",
        }
    }
}

/// Every themed colour, as a `#rrggbb` hex string. Serialized camelCase for the
/// non-Rust shells; consumed field-by-field by GTK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    /// Whether this is a dark-ish theme (drives system-chrome choice).
    pub dark: bool,
    /// Reader canvas background (warm paper).
    pub paper: String,
    /// Primary body ink.
    pub ink: String,
    /// Muted secondary text (panel `Faded`).
    pub faded: String,
    /// Translator-supplied (italic) words in the reader.
    pub added: String,
    /// The divine name.
    pub divine: String,
    /// Psalm-superscription ink.
    pub title_ink: String,
    /// The gold accent (verse numbers, links, connectors, bands — via alpha).
    pub gold: String,
    /// A section header (spaced muted gold).
    pub section: String,
    /// Authority tiers.
    pub tier_god: String,
    pub tier_human: String,
    pub tier_machine: String,
    pub tier_research: String,
    /// Neutral grey (pronunciations, kind labels).
    pub mono: String,
    /// Morphology gloss tint.
    pub morph: String,
    /// A lemma shown small beside a chip.
    pub lemma: String,
    /// A horizontal rule / hairline.
    pub rule: String,
    /// Popup / study-panel paper.
    pub popup_paper: String,
    /// Pane-nav strip background.
    pub pane_nav_bg: String,
    /// The canon-overview strip background.
    pub strip_bg: String,
    /// Weave-authoring pin selection (blue base; drawn with alpha).
    pub pin: String,
}

impl Palette {
    /// A panel [`crate::panel::Color`] → this palette's hex, or `None` for
    /// `Ink` (the shell inherits the panel's themed body ink). Every shell maps
    /// these identically, so the panel reads the same everywhere.
    pub fn panel_color(&self, c: crate::panel::Color) -> Option<&str> {
        use crate::panel::Color::*;
        Some(match c {
            Ink => return None,
            Faded => &self.faded,
            Gold => &self.gold,
            Section => &self.section,
            TierGod => &self.tier_god,
            TierHuman => &self.tier_human,
            TierMachine => &self.tier_machine,
            TierResearch => &self.tier_research,
            Mono => &self.mono,
            Morph => &self.morph,
            Lemma => &self.lemma,
        })
    }
}

/// The palette for a concrete theme.
pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            dark: false,
            paper: "#fcf9f4".into(),
            ink: "#211f1a".into(),
            faded: "#8a8276".into(),
            added: "#6b6862".into(),
            divine: "#4d3326".into(),
            title_ink: "#665c4d".into(),
            gold: "#9e7d38".into(),
            section: "#a0894a".into(),
            tier_god: "#9e7d38".into(),
            tier_human: "#6f8f6a".into(),
            tier_machine: "#999999".into(),
            tier_research: "#b04a3a".into(),
            mono: "#888888".into(),
            morph: "#6a5a2a".into(),
            lemma: "#8a7a52".into(),
            rule: "#d8cba8".into(),
            popup_paper: "#f2eee6".into(),
            pane_nav_bg: "#efeae1".into(),
            strip_bg: "#ebe6db".into(),
            pin: "#4073bf".into(),
        },
        // Candlelight-warm dark: a dark brown-charcoal paper, warm off-white ink,
        // a brighter gold that holds contrast on the dark ground.
        Theme::Dark => Palette {
            dark: true,
            paper: "#1f1b16".into(),
            ink: "#e8e0d0".into(),
            faded: "#9a9385".into(),
            added: "#8f8778".into(),
            divine: "#d8b48c".into(),
            title_ink: "#b8aa90".into(),
            gold: "#c8a24e".into(),
            section: "#b8975a".into(),
            tier_god: "#c8a24e".into(),
            tier_human: "#8fb389".into(),
            tier_machine: "#9a9a9a".into(),
            tier_research: "#d0705e".into(),
            mono: "#9a9a9a".into(),
            morph: "#b0a06a".into(),
            lemma: "#c0a878".into(),
            rule: "#4a4234".into(),
            popup_paper: "#2a251e".into(),
            pane_nav_bg: "#262019".into(),
            strip_bg: "#262019".into(),
            pin: "#6a9bd8".into(),
        },
        // True-black night (OLED): pure-black paper, everything else tuned for it.
        Theme::Night => Palette {
            dark: true,
            paper: "#000000".into(),
            ink: "#d8d2c6".into(),
            faded: "#8a857a".into(),
            added: "#7d786e".into(),
            divine: "#d0ac86".into(),
            title_ink: "#a89e88".into(),
            gold: "#c9a24e".into(),
            section: "#b0925a".into(),
            tier_god: "#c9a24e".into(),
            tier_human: "#86ac82".into(),
            tier_machine: "#8f8f8f".into(),
            tier_research: "#cf6e5c".into(),
            mono: "#8f8f8f".into(),
            morph: "#a89a66".into(),
            lemma: "#b8a06f".into(),
            rule: "#33302a".into(),
            popup_paper: "#0d0d0b".into(),
            pane_nav_bg: "#0a0a08".into(),
            strip_bg: "#0a0a08".into(),
            pin: "#5f92cf".into(),
        },
    }
}

/// The fixed highlight-wash tones for the tags-as-highlights feature (Tier 0
/// #4): `(name, hex)`, drawn behind a verse at low alpha. A small, muted set
/// tuned to sit on the warm paper without shouting; the same tones read as soft
/// washes over the dark themes too.
pub const HIGHLIGHT_TONES: [(&str, &str); 6] = [
    ("amber", "#f6e0a0"),
    ("rose", "#f3cfcf"),
    ("sky", "#cfe0f3"),
    ("sage", "#d2e7cf"),
    ("lilac", "#e6d8f2"),
    ("sand", "#ece1c9"),
];

/// The hex for a named highlight tone, if it's one of [`HIGHLIGHT_TONES`].
pub fn highlight_hex(name: &str) -> Option<&'static str> {
    HIGHLIGHT_TONES.iter().find(|(n, _)| *n == name).map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_tokens_roundtrip() {
        for t in [Theme::Light, Theme::Dark, Theme::Night] {
            assert_eq!(Theme::parse(t.token()), Some(t));
        }
        for c in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::Night] {
            assert_eq!(ThemeChoice::parse(c.token()), Some(c));
        }
        assert_eq!(Theme::parse("nope"), None);
    }

    #[test]
    fn system_resolves_and_cycle_visits_all() {
        assert_eq!(ThemeChoice::System.resolve(false), Theme::Light);
        assert_eq!(ThemeChoice::System.resolve(true), Theme::Dark);
        assert_eq!(ThemeChoice::Night.resolve(false), Theme::Night);
        // Cycling touches every choice.
        let mut seen = std::collections::HashSet::new();
        let mut c = ThemeChoice::Light;
        for _ in 0..4 {
            seen.insert(c.token());
            c = c.next();
        }
        assert_eq!(seen.len(), 4);
        assert_eq!(c, ThemeChoice::Light); // back to start
    }

    #[test]
    fn light_palette_is_unchanged_from_shipped() {
        let p = palette(Theme::Light);
        assert!(!p.dark);
        assert_eq!(p.paper, "#fcf9f4");
        assert_eq!(p.gold, "#9e7d38");
        assert_eq!(p.section, "#a0894a");
        assert_eq!(p.morph, "#6a5a2a");
        assert_eq!(p.lemma, "#8a7a52");
    }

    #[test]
    fn panel_color_maps_and_ink_inherits() {
        let p = palette(Theme::Light);
        assert_eq!(p.panel_color(crate::panel::Color::Ink), None);
        assert_eq!(p.panel_color(crate::panel::Color::Gold), Some("#9e7d38"));
        assert_eq!(p.panel_color(crate::panel::Color::TierHuman), Some("#6f8f6a"));
    }

    #[test]
    fn highlight_tones_resolve() {
        assert_eq!(highlight_hex("amber"), Some("#f6e0a0"));
        assert_eq!(highlight_hex("nope"), None);
        assert_eq!(HIGHLIGHT_TONES.len(), 6);
    }
}
