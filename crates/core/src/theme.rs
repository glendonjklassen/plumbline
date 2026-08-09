//! The shared colour palette — the one place every shell's colours are defined,
//! so light/dark/night can't drift between GTK, WinUI, and (later) Compose.
//! Tier 0 #5.
//!
//! A [`Palette`] carries a hex string per semantic role. The reader, chrome, and
//! the study-panel's [`crate::panel::Color`] roles all resolve through it. GTK
//! reads the struct directly; the non-Rust shells fetch it as JSON
//! (`plumbline_theme_palette_json`) and apply it. Shells own translucency: a search
//! band, a Strong's underline, a weave connector are all drawn by applying alpha
//! to `gold` / `pin`, so those follow the theme for free.
//!
//! Every role a shell paints as *text* has to clear WCAG AA (4.5:1) against
//! every surface a shell paints text *on* — in all three themes. That is a test
//! (`contrast::every_text_role_clears_aa_on_every_surface`), not a convention, and
//! it is why the light muted tones are deeper than the ones the shells first
//! shipped with: `faded`, `gold`, `section`, the tiers, `mono` and `lemma` all sat
//! between 2.5:1 and 4.0:1 on the warm paper, which is a muted tone you can't
//! quite read. The hues are the same; only the lightness moved.

use serde::{Deserialize, Serialize};

/// A concrete, resolved theme (what actually paints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    Night,
    /// Named presets, inspired by well-known editor themes and tuned to clear
    /// the same WCAG-AA bar the built-ins do (so they read as this app, not as a
    /// pixel-copy). Dark unless noted.
    Darcula,
    SolarizedLight,
    SolarizedDark,
    Gruvbox,
    Nord,
    OneDark,
    Sepia,
    CatppuccinMocha,
    CatppuccinLatte,
    TokyoNight,
    RosePine,
    Synthwave,
}

impl Theme {
    pub fn token(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::Night => "night",
            Theme::Darcula => "darcula",
            Theme::SolarizedLight => "solarized-light",
            Theme::SolarizedDark => "solarized-dark",
            Theme::Gruvbox => "gruvbox",
            Theme::Nord => "nord",
            Theme::OneDark => "one-dark",
            Theme::Sepia => "sepia",
            Theme::CatppuccinMocha => "catppuccin-mocha",
            Theme::CatppuccinLatte => "catppuccin-latte",
            Theme::TokyoNight => "tokyo-night",
            Theme::RosePine => "rose-pine",
            Theme::Synthwave => "synthwave",
        }
    }
    pub fn parse(t: &str) -> Option<Theme> {
        match t {
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            "night" => Some(Theme::Night),
            "darcula" => Some(Theme::Darcula),
            "solarized-light" => Some(Theme::SolarizedLight),
            "solarized-dark" => Some(Theme::SolarizedDark),
            "gruvbox" => Some(Theme::Gruvbox),
            "nord" => Some(Theme::Nord),
            "one-dark" => Some(Theme::OneDark),
            "sepia" => Some(Theme::Sepia),
            "catppuccin-mocha" => Some(Theme::CatppuccinMocha),
            "catppuccin-latte" => Some(Theme::CatppuccinLatte),
            "tokyo-night" => Some(Theme::TokyoNight),
            "rose-pine" => Some(Theme::RosePine),
            "synthwave" => Some(Theme::Synthwave),
            _ => None,
        }
    }
    /// Whether the system chrome (scrollbars, dialogs) should be dark.
    pub fn is_dark(self) -> bool {
        !matches!(self, Theme::Light | Theme::SolarizedLight | Theme::Sepia | Theme::CatppuccinLatte)
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
    Darcula,
    SolarizedLight,
    SolarizedDark,
    Gruvbox,
    Nord,
    OneDark,
    Sepia,
    CatppuccinMocha,
    CatppuccinLatte,
    TokyoNight,
    RosePine,
    Synthwave,
}

impl ThemeChoice {
    pub fn token(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
            ThemeChoice::Night => "night",
            ThemeChoice::Darcula => "darcula",
            ThemeChoice::SolarizedLight => "solarized-light",
            ThemeChoice::SolarizedDark => "solarized-dark",
            ThemeChoice::Gruvbox => "gruvbox",
            ThemeChoice::Nord => "nord",
            ThemeChoice::OneDark => "one-dark",
            ThemeChoice::Sepia => "sepia",
            ThemeChoice::CatppuccinMocha => "catppuccin-mocha",
            ThemeChoice::CatppuccinLatte => "catppuccin-latte",
            ThemeChoice::TokyoNight => "tokyo-night",
            ThemeChoice::RosePine => "rose-pine",
            ThemeChoice::Synthwave => "synthwave",
        }
    }
    pub fn parse(t: &str) -> Option<ThemeChoice> {
        match t {
            "system" => Some(ThemeChoice::System),
            "light" => Some(ThemeChoice::Light),
            "dark" => Some(ThemeChoice::Dark),
            "night" => Some(ThemeChoice::Night),
            "darcula" => Some(ThemeChoice::Darcula),
            "solarized-light" => Some(ThemeChoice::SolarizedLight),
            "solarized-dark" => Some(ThemeChoice::SolarizedDark),
            "gruvbox" => Some(ThemeChoice::Gruvbox),
            "nord" => Some(ThemeChoice::Nord),
            "one-dark" => Some(ThemeChoice::OneDark),
            "sepia" => Some(ThemeChoice::Sepia),
            "catppuccin-mocha" => Some(ThemeChoice::CatppuccinMocha),
            "catppuccin-latte" => Some(ThemeChoice::CatppuccinLatte),
            "tokyo-night" => Some(ThemeChoice::TokyoNight),
            "rose-pine" => Some(ThemeChoice::RosePine),
            "synthwave" => Some(ThemeChoice::Synthwave),
            _ => None,
        }
    }
    /// Resolve to a concrete theme; `System` uses `system_dark`. The named
    /// presets are already concrete — they map straight through.
    pub fn resolve(self, system_dark: bool) -> Theme {
        match self {
            ThemeChoice::Light => Theme::Light,
            ThemeChoice::Dark => Theme::Dark,
            ThemeChoice::Night => Theme::Night,
            ThemeChoice::Darcula => Theme::Darcula,
            ThemeChoice::SolarizedLight => Theme::SolarizedLight,
            ThemeChoice::SolarizedDark => Theme::SolarizedDark,
            ThemeChoice::Gruvbox => Theme::Gruvbox,
            ThemeChoice::Nord => Theme::Nord,
            ThemeChoice::OneDark => Theme::OneDark,
            ThemeChoice::Sepia => Theme::Sepia,
            ThemeChoice::CatppuccinMocha => Theme::CatppuccinMocha,
            ThemeChoice::CatppuccinLatte => Theme::CatppuccinLatte,
            ThemeChoice::TokyoNight => Theme::TokyoNight,
            ThemeChoice::RosePine => Theme::RosePine,
            ThemeChoice::Synthwave => Theme::Synthwave,
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
    /// system → light). The named presets are Settings-only and sit outside the
    /// cycle — cycling out of one returns to `System`.
    pub fn next(self) -> ThemeChoice {
        match self {
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::Night,
            ThemeChoice::Night => ThemeChoice::System,
            ThemeChoice::System => ThemeChoice::Light,
            _ => ThemeChoice::System,
        }
    }
    /// A short human label for the toggle button.
    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Theme: system",
            ThemeChoice::Light => "Theme: light",
            ThemeChoice::Dark => "Theme: dark",
            ThemeChoice::Night => "Theme: night",
            ThemeChoice::Darcula => "Theme: Darcula",
            ThemeChoice::SolarizedLight => "Theme: Solarized Light",
            ThemeChoice::SolarizedDark => "Theme: Solarized Dark",
            ThemeChoice::Gruvbox => "Theme: Gruvbox",
            ThemeChoice::Nord => "Theme: Nord",
            ThemeChoice::OneDark => "Theme: One Dark",
            ThemeChoice::Sepia => "Theme: Sepia",
            ThemeChoice::CatppuccinMocha => "Theme: Catppuccin Mocha",
            ThemeChoice::CatppuccinLatte => "Theme: Catppuccin Latte",
            ThemeChoice::TokyoNight => "Theme: Tokyo Night",
            ThemeChoice::RosePine => "Theme: Rosé Pine",
            ThemeChoice::Synthwave => "Theme: Synthwave",
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
    /// The reading map's three hues (see `crate::reading::Standing`) — the
    /// navigator's book/chapter tiles. Shells own the bloom: the glow is these
    /// same colours at rising alpha and spread, so it follows the theme for free.
    ///
    /// A chapter never read. GOLD, and glowing from the first launch: unopened
    /// scripture should read as treasure worth going after, not as a gap in a
    /// checklist.
    pub read_unread: String,
    /// A chapter partway through — copper, clearly darker than the unread gold so
    /// "started" never reads as "untouched".
    pub read_partial: String,
    /// A chapter read all the way through — sage, settled.
    pub read_done: String,
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
            faded: "#6c665d".into(),
            added: "#696661".into(),
            divine: "#4d3326".into(),
            title_ink: "#665c4d".into(),
            gold: "#7d632c".into(),
            section: "#776537".into(),
            tier_god: "#7d632c".into(),
            tier_human: "#556d51".into(),
            // Both greys land on #666666: that is the lightest neutral grey that
            // clears AA on `strip_bg`, so the old #999999/#888888 pair had nowhere
            // left to differ. The tier is told apart by where it sits, not by tone.
            tier_machine: "#666666".into(),
            tier_research: "#aa4838".into(),
            mono: "#666666".into(),
            morph: "#6a5a2a".into(),
            lemma: "#736544".into(),
            rule: "#d8cba8".into(),
            popup_paper: "#f2eee6".into(),
            pane_nav_bg: "#efeae1".into(),
            strip_bg: "#ebe6db".into(),
            pin: "#4073bf".into(),
            read_unread: "#c9a227".into(),
            read_partial: "#a8642c".into(),
            read_done: "#6f8f6a".into(),
        },
        // Candlelight-warm dark: a dark brown-charcoal paper, warm off-white ink,
        // a brighter gold that holds contrast on the dark ground.
        Theme::Dark => Palette {
            dark: true,
            paper: "#1f1b16".into(),
            ink: "#e8e0d0".into(),
            faded: "#9a9385".into(),
            // Barely below the ink: on dark paper a dimmed added-word grey read
            // as "darkened" and broke the reading flow —
            // the italic slant carries the distinction, the tint only whispers.
            added: "#d9cfba".into(),
            divine: "#d8b48c".into(),
            title_ink: "#b8aa90".into(),
            gold: "#c8a24e".into(),
            section: "#b8975a".into(),
            tier_god: "#c8a24e".into(),
            tier_human: "#8fb389".into(),
            tier_machine: "#9a9a9a".into(),
            // A hair brighter than it was: #d0705e is 4.46:1 on this theme's
            // popup paper — the only dark/night pair that missed AA.
            tier_research: "#d17261".into(),
            mono: "#9a9a9a".into(),
            morph: "#b0a06a".into(),
            lemma: "#c0a878".into(),
            rule: "#4a4234".into(),
            popup_paper: "#2a251e".into(),
            pane_nav_bg: "#262019".into(),
            strip_bg: "#262019".into(),
            pin: "#6a9bd8".into(),
            read_unread: "#e0bb45".into(),
            read_partial: "#c07a3c".into(),
            read_done: "#8fb389".into(),
        },
        // True-black night (OLED): pure-black paper, everything else tuned for it.
        Theme::Night => Palette {
            dark: true,
            paper: "#000000".into(),
            ink: "#d8d2c6".into(),
            faded: "#8a857a".into(),
            added: "#c8c1b0".into(), // see Dark — italics carry the distinction

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
            read_unread: "#d8b23e".into(),
            read_partial: "#b87338".into(),
            read_done: "#86ac82".into(),
        },
        // ── named presets ─────────────────────────────────────────────────────
        // Inspired by well-known editor themes; muted roles are pushed lighter
        // (dark themes) or darker (light) than the originals so every text role
        // clears WCAG AA on every surface — the originals' comment/base tones do
        // not. Tune the hex to taste; the contrast test is the floor.
        //
        // The reading-map tiles (`read_*`) reuse each theme's own gold/divine/
        // tier_human so the navigator is unmistakably part of the active theme —
        // never a fixed gold/copper/sage that ignores the palette.
        //
        // JetBrains IDEA "Darcula": brown-grey ground, its orange keyword / green
        // string / yellow function accents.
        Theme::Darcula => Palette {
            dark: true,
            paper: "#2b2b2b".into(),
            ink: "#a9b7c6".into(),
            faded: "#9ba1a8".into(),
            added: "#c1c8cf".into(),
            divine: "#d1935f".into(),
            title_ink: "#a6adb4".into(),
            gold: "#e0b060".into(),
            section: "#c99a55".into(),
            tier_god: "#e0b060".into(),
            tier_human: "#82a866".into(),
            tier_machine: "#9ba1a8".into(),
            tier_research: "#e28a7c".into(),
            mono: "#9ba1a8".into(),
            morph: "#cdae7a".into(),
            lemma: "#c3b48f".into(),
            rule: "#4b4b4b".into(),
            popup_paper: "#313335".into(),
            pane_nav_bg: "#323232".into(),
            strip_bg: "#2f2f2f".into(),
            pin: "#4a88c7".into(),
            read_unread: "#e0b060".into(),
            read_partial: "#d1935f".into(),
            read_done: "#82a866".into(),
        },
        Theme::SolarizedDark => Palette {
            dark: true,
            paper: "#002b36".into(),
            ink: "#cdd6d6".into(),
            faded: "#9fadad".into(),
            added: "#b9c6c6".into(),
            divine: "#e0a060".into(),
            title_ink: "#a9b7b7".into(),
            gold: "#d0ab4d".into(),
            section: "#c0a55a".into(),
            tier_god: "#d0ab4d".into(),
            tier_human: "#93c37a".into(),
            tier_machine: "#9fadad".into(),
            tier_research: "#f2857f".into(),
            mono: "#9fadad".into(),
            morph: "#bfae76".into(),
            lemma: "#c2b487".into(),
            rule: "#0e3b47".into(),
            popup_paper: "#073642".into(),
            pane_nav_bg: "#073642".into(),
            strip_bg: "#062f3a".into(),
            pin: "#268bd2".into(),
            read_unread: "#d0ab4d".into(),
            read_partial: "#e0a060".into(),
            read_done: "#93c37a".into(),
        },
        Theme::SolarizedLight => Palette {
            dark: false,
            paper: "#fdf6e3".into(),
            ink: "#3f4d52".into(),
            faded: "#465458".into(),
            added: "#4c5a56".into(),
            divine: "#6e3f22".into(),
            title_ink: "#4c5a50".into(),
            gold: "#6e5410".into(),
            section: "#66531f".into(),
            tier_god: "#6e5410".into(),
            tier_human: "#425720".into(),
            tier_machine: "#4c5654".into(),
            tier_research: "#9c2f1c".into(),
            mono: "#4c5654".into(),
            morph: "#544b22".into(),
            lemma: "#564f2b".into(),
            rule: "#d8d2bf".into(),
            popup_paper: "#eee8d5".into(),
            pane_nav_bg: "#eee8d5".into(),
            strip_bg: "#e8e1cd".into(),
            pin: "#1c6aa8".into(),
            read_unread: "#6e5410".into(),
            read_partial: "#6e3f22".into(),
            read_done: "#425720".into(),
        },
        Theme::Gruvbox => Palette {
            dark: true,
            paper: "#282828".into(),
            ink: "#ebdbb2".into(),
            faded: "#b0a189".into(),
            added: "#d5c9a5".into(),
            divine: "#fe8019".into(),
            title_ink: "#cabfa0".into(),
            gold: "#fabd2f".into(),
            section: "#d6a94a".into(),
            tier_god: "#fabd2f".into(),
            tier_human: "#b8bb26".into(),
            tier_machine: "#b0a99c".into(),
            tier_research: "#fb6a5a".into(),
            mono: "#b0a99c".into(),
            morph: "#d5be7a".into(),
            lemma: "#cdbf94".into(),
            rule: "#3c3836".into(),
            popup_paper: "#32302f".into(),
            pane_nav_bg: "#32302f".into(),
            strip_bg: "#1d2021".into(),
            pin: "#83a598".into(),
            read_unread: "#fabd2f".into(),
            read_partial: "#fe8019".into(),
            read_done: "#b8bb26".into(),
        },
        Theme::Nord => Palette {
            dark: true,
            paper: "#2e3440".into(),
            ink: "#eceff4".into(),
            faded: "#9aa5b8".into(),
            added: "#d8dee9".into(),
            divine: "#dc9a80".into(),
            title_ink: "#c2cad8".into(),
            gold: "#ebcb8b".into(),
            section: "#d0b978".into(),
            tier_god: "#ebcb8b".into(),
            tier_human: "#a3be8c".into(),
            tier_machine: "#aab4c4".into(),
            tier_research: "#e08a94".into(),
            mono: "#aab4c4".into(),
            morph: "#d6c48c".into(),
            lemma: "#cdbf9a".into(),
            rule: "#434c5e".into(),
            popup_paper: "#292e39".into(),
            pane_nav_bg: "#292e39".into(),
            strip_bg: "#272b34".into(),
            pin: "#81a1c1".into(),
            read_unread: "#ebcb8b".into(),
            read_partial: "#dc9a80".into(),
            read_done: "#a3be8c".into(),
        },
        // Atom / VS Code "One Dark": blue-grey ground, soft red/green/yellow/blue.
        Theme::OneDark => Palette {
            dark: true,
            paper: "#282c34".into(),
            ink: "#abb2bf".into(),
            faded: "#959cab".into(),
            added: "#c2c7d0".into(),
            divine: "#d19a66".into(),
            title_ink: "#9aa1ad".into(),
            gold: "#e5c07b".into(),
            section: "#cbab6e".into(),
            tier_god: "#e5c07b".into(),
            tier_human: "#98c379".into(),
            tier_machine: "#969eac".into(),
            tier_research: "#ef858d".into(),
            mono: "#969eac".into(),
            morph: "#cdb37a".into(),
            lemma: "#c3b892".into(),
            rule: "#3b4048".into(),
            popup_paper: "#21252b".into(),
            pane_nav_bg: "#2c313a".into(),
            strip_bg: "#21252b".into(),
            pin: "#61afef".into(),
            read_unread: "#e5c07b".into(),
            read_partial: "#d19a66".into(),
            read_done: "#98c379".into(),
        },
        // Sepia: a warm-paper light theme for long reading — browns and muted
        // greens on aged paper.
        Theme::Sepia => Palette {
            dark: false,
            paper: "#f4ecd8".into(),
            ink: "#433422".into(),
            faded: "#6b5a44".into(),
            added: "#5c4e3c".into(),
            divine: "#7a3f1c".into(),
            title_ink: "#5a4a36".into(),
            gold: "#7a5c1e".into(),
            section: "#74601f".into(),
            tier_god: "#7a5c1e".into(),
            tier_human: "#4a6030".into(),
            tier_machine: "#5c554a".into(),
            tier_research: "#9c3a1e".into(),
            mono: "#5c554a".into(),
            morph: "#5a4a20".into(),
            lemma: "#574f2b".into(),
            rule: "#d8caa8".into(),
            popup_paper: "#efe6cf".into(),
            pane_nav_bg: "#ece2c9".into(),
            strip_bg: "#e8ddc2".into(),
            pin: "#1c6aa8".into(),
            read_unread: "#7a5c1e".into(),
            read_partial: "#7a3f1c".into(),
            read_done: "#4a6030".into(),
        },
        // Catppuccin "Mocha": pastel dark — lavender ground, peach/yellow/green/pink.
        Theme::CatppuccinMocha => Palette {
            dark: true,
            paper: "#1e1e2e".into(),
            ink: "#cdd6f4".into(),
            faded: "#a6adc8".into(),
            added: "#bcc3e0".into(),
            divine: "#fab387".into(),
            title_ink: "#b8c0dc".into(),
            gold: "#f9e2af".into(),
            section: "#d9c48f".into(),
            tier_god: "#f9e2af".into(),
            tier_human: "#a6e3a1".into(),
            tier_machine: "#a0a6bf".into(),
            tier_research: "#f38ba8".into(),
            mono: "#a0a6bf".into(),
            morph: "#e0cf9a".into(),
            lemma: "#d0c9a8".into(),
            rule: "#45475a".into(),
            popup_paper: "#181825".into(),
            pane_nav_bg: "#26263a".into(),
            strip_bg: "#181825".into(),
            pin: "#89b4fa".into(),
            read_unread: "#f9e2af".into(),
            read_partial: "#fab387".into(),
            read_done: "#a6e3a1".into(),
        },
        // Catppuccin "Latte": pastel light — the Mocha family on a bright ground,
        // accents darkened to clear AA on paper.
        Theme::CatppuccinLatte => Palette {
            dark: false,
            paper: "#eff1f5".into(),
            ink: "#4c4f69".into(),
            faded: "#5c5f77".into(),
            added: "#55586f".into(),
            divine: "#a03e08".into(),
            title_ink: "#565972".into(),
            gold: "#705515".into(),
            section: "#6a5518".into(),
            tier_god: "#705515".into(),
            tier_human: "#2f6a1c".into(),
            tier_machine: "#57596e".into(),
            tier_research: "#c20f36".into(),
            mono: "#57596e".into(),
            morph: "#6a5a20".into(),
            lemma: "#5f5730".into(),
            rule: "#ccd0da".into(),
            popup_paper: "#e6e9ef".into(),
            pane_nav_bg: "#dce0e8".into(),
            strip_bg: "#e6e9ef".into(),
            pin: "#1657cc".into(),
            read_unread: "#705515".into(),
            read_partial: "#a03e08".into(),
            read_done: "#2f6a1c".into(),
        },
        // "Tokyo Night": deep indigo ground, orange/yellow/green/blue accents.
        Theme::TokyoNight => Palette {
            dark: true,
            paper: "#1a1b26".into(),
            ink: "#c0caf5".into(),
            faded: "#8189b3".into(),
            added: "#aab2dd".into(),
            divine: "#ff9e64".into(),
            title_ink: "#a2abd4".into(),
            gold: "#e0af68".into(),
            section: "#c99f61".into(),
            tier_god: "#e0af68".into(),
            tier_human: "#9ece6a".into(),
            tier_machine: "#9aa0c4".into(),
            tier_research: "#f7768e".into(),
            mono: "#9aa0c4".into(),
            morph: "#cfa96a".into(),
            lemma: "#c4b48a".into(),
            rule: "#2a2e42".into(),
            popup_paper: "#16161e".into(),
            pane_nav_bg: "#20222e".into(),
            strip_bg: "#16161e".into(),
            pin: "#7aa2f7".into(),
            read_unread: "#e0af68".into(),
            read_partial: "#ff9e64".into(),
            read_done: "#9ece6a".into(),
        },
        // "Rosé Pine": muted rose/pine on a soft-black ground; "read" borrows the
        // foam cyan, which reads clearly apart from the gold and rose.
        Theme::RosePine => Palette {
            dark: true,
            paper: "#191724".into(),
            ink: "#e0def4".into(),
            faded: "#a7a3c4".into(),
            added: "#cbc8e6".into(),
            divine: "#ebbcba".into(),
            title_ink: "#c8c4e0".into(),
            gold: "#f6c177".into(),
            section: "#d9a862".into(),
            tier_god: "#f6c177".into(),
            tier_human: "#9ccfd8".into(),
            tier_machine: "#a7a3c4".into(),
            tier_research: "#eb6f92".into(),
            mono: "#a7a3c4".into(),
            morph: "#e2c58a".into(),
            lemma: "#d4c49a".into(),
            rule: "#403d52".into(),
            popup_paper: "#1f1d2e".into(),
            pane_nav_bg: "#232135".into(),
            strip_bg: "#1f1d2e".into(),
            pin: "#c4a7e7".into(),
            read_unread: "#f6c177".into(),
            read_partial: "#ebbcba".into(),
            read_done: "#9ccfd8".into(),
        },
        // Synthwave: deep indigo-violet night sky, hot-pink accent, cyan reserved
        // for selection, amber for the divine name.
        Theme::Synthwave => Palette {
            dark: true,
            paper: "#1a1033".into(),
            ink: "#f4ecff".into(),
            faded: "#b0a3dc".into(),
            added: "#d0a8e8".into(),
            divine: "#ffd75e".into(),
            title_ink: "#c4b4ee".into(),
            // The accent is ELECTRIC CYAN, not the genre's hot pink: this role
            // paints verse numbers, every link, the connectors and the search
            // band (at alpha), so it is the colour the reader sees most of, and
            // pink at that volume reads as a highlighter over the text. Pink
            // stays as detail — `lemma`, and the `pin` selection.
            gold: "#4fd6ff".into(),
            section: "#86c9ee".into(),
            tier_god: "#ffcf6b".into(),
            tier_human: "#66e2a8".into(),
            tier_machine: "#b4b0d8".into(),
            tier_research: "#ff7d92".into(),
            mono: "#b2aed2".into(),
            // Violet and pink, moved off the blues they used to sit in: with a
            // cyan accent, a cyan gloss and a blue lemma read as links.
            morph: "#c49cff".into(),
            lemma: "#ff9fe0".into(),
            rule: "#45307a".into(),
            popup_paper: "#221443".into(),
            pane_nav_bg: "#271950".into(),
            strip_bg: "#2d1d5c".into(),
            // Magenta, for the same reason: a cyan selection band under cyan
            // links is one signal painted twice.
            pin: "#ff5fd2".into(),
            read_unread: "#ffc94d".into(),
            read_partial: "#ff9e64".into(),
            read_done: "#4ecca3".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every concrete theme — the one list the roundtrip and contrast tests
    /// share, so a new theme can't be added to `palette()` and quietly skip
    /// either check.
    pub(super) const ALL_THEMES: [Theme; 15] = [
        Theme::Light,
        Theme::Dark,
        Theme::Night,
        Theme::Darcula,
        Theme::SolarizedLight,
        Theme::SolarizedDark,
        Theme::Gruvbox,
        Theme::Nord,
        Theme::OneDark,
        Theme::Sepia,
        Theme::CatppuccinMocha,
        Theme::CatppuccinLatte,
        Theme::TokyoNight,
        Theme::RosePine,
        Theme::Synthwave,
    ];

    #[test]
    fn theme_tokens_roundtrip() {
        for t in ALL_THEMES {
            assert_eq!(Theme::parse(t.token()), Some(t));
        }
        for c in [
            ThemeChoice::System,
            ThemeChoice::Light,
            ThemeChoice::Dark,
            ThemeChoice::Night,
            ThemeChoice::Darcula,
            ThemeChoice::SolarizedLight,
            ThemeChoice::SolarizedDark,
            ThemeChoice::Gruvbox,
            ThemeChoice::Nord,
            ThemeChoice::OneDark,
            ThemeChoice::Sepia,
            ThemeChoice::CatppuccinMocha,
            ThemeChoice::CatppuccinLatte,
            ThemeChoice::TokyoNight,
            ThemeChoice::RosePine,
            ThemeChoice::Synthwave,
        ] {
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

    /// The light anchors that must not drift: the paper and ink the whole look
    /// is built on, the tier-God/gold identity the panel relies on, and the
    /// unread gold (deliberately treasure-bright). Everything else in
    /// the light palette is governed by the contrast test below, not by a hex.
    #[test]
    fn light_anchors_hold() {
        let p = palette(Theme::Light);
        assert!(!p.dark);
        assert_eq!(p.paper, "#fcf9f4");
        assert_eq!(p.ink, "#211f1a");
        assert_eq!(p.tier_god, p.gold);
        assert_eq!(p.read_unread, "#c9a227");
    }

    #[test]
    fn panel_color_maps_and_ink_inherits() {
        let p = palette(Theme::Light);
        assert_eq!(p.panel_color(crate::panel::Color::Ink), None);
        assert_eq!(p.panel_color(crate::panel::Color::Gold), Some(p.gold.as_str()));
        assert_eq!(p.panel_color(crate::panel::Color::TierHuman), Some(p.tier_human.as_str()));
    }
}

/// The palette's accessibility floor, measured rather than eyeballed.
///
/// A muted tone that can't be read isn't muted, it's missing. Light theme shipped
/// with six roles between 2.5:1 and 4.0:1 on the warm paper; this module is the
/// guard that stops the next palette tweak putting them back.
#[cfg(test)]
mod contrast {
    use super::*;

    /// WCAG AA for body text. Nothing in the palette is large-text-only —
    /// `faded` paints 11 px canon-strip labels, `mono` and `lemma` the panel's
    /// smallest type — so every text role is held to the body threshold, not the
    /// 3:1 large-text one.
    const AA_BODY: f64 = 4.5;

    /// One sRGB channel → linear light (WCAG relative-luminance, step 1).
    fn linear(c: u8) -> f64 {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// `#rrggbb` → WCAG relative luminance.
    fn luminance(hex: &str) -> f64 {
        let h = hex.strip_prefix('#').unwrap_or_else(|| panic!("{hex} is not #rrggbb"));
        assert_eq!(h.len(), 6, "{hex} is not #rrggbb");
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or_else(|_| panic!("{hex} is not hex"));
        0.2126 * linear(byte(0)) + 0.7152 * linear(byte(2)) + 0.0722 * linear(byte(4))
    }

    /// The WCAG contrast ratio between two colours (order does not matter).
    fn ratio(a: &str, b: &str) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Every role a shell paints as text, paired with its wire name so a failure
    /// says which field to edit.
    fn text_roles(p: &Palette) -> Vec<(&'static str, &str)> {
        vec![
            ("ink", &p.ink),
            ("faded", &p.faded),
            ("added", &p.added),
            ("divine", &p.divine),
            ("titleInk", &p.title_ink),
            ("gold", &p.gold),
            ("section", &p.section),
            ("tierGod", &p.tier_god),
            ("tierHuman", &p.tier_human),
            ("tierMachine", &p.tier_machine),
            ("tierResearch", &p.tier_research),
            ("mono", &p.mono),
            ("morph", &p.morph),
            ("lemma", &p.lemma),
        ]
    }

    /// Every surface a shell paints text on. Checking all four beats reasoning
    /// about which role lands where: a role that clears the darkest light surface
    /// clears the reader, the panel, the chrome and the canon strip at once.
    fn surfaces(p: &Palette) -> Vec<(&'static str, &str)> {
        vec![
            ("paper", &p.paper),
            ("popupPaper", &p.popup_paper),
            ("paneNavBg", &p.pane_nav_bg),
            ("stripBg", &p.strip_bg),
        ]
    }

    /// Roles that carry no text, with the reason. Listed so the exhaustiveness
    /// test below can prove nothing slipped past a contrast decision.
    const NOT_TEXT: &[(&str, &str)] = &[
        ("dark", "a flag, not a colour"),
        ("pin", "the weave-authoring selection band; shells draw it with alpha"),
        // 1.5:1 on paper. A 3:1 hairline would turn every divider in a
        // paper-and-ink reader into a hard line, and the controls that draw it as a
        // border all carry their own legible label — but a text field's border IS
        // its only affordance, so this one wants the maintainer's eye.
        ("rule", "a decorative hairline, never an only-affordance boundary"),
        // Tile paint, not type: the shells composite these at ≤0.30 alpha for the
        // fill and ≤0.80 for the border, so the raw hex never reaches the screen,
        // and every tile also states its standing in its own tooltip.
        ("readUnread", "reading-map tile paint, composited with alpha"),
        ("readPartial", "reading-map tile paint, composited with alpha"),
        ("readDone", "reading-map tile paint, composited with alpha"),
    ];

    #[test]
    fn every_text_role_clears_aa_on_every_surface() {
        for theme in super::tests::ALL_THEMES {
            let p = palette(theme);
            for (role, fg) in text_roles(&p) {
                for (surface, bg) in surfaces(&p) {
                    let r = ratio(fg, bg);
                    assert!(
                        r >= AA_BODY,
                        "{} theme: {role} {fg} on {surface} {bg} is {r:.2}:1 — \
                         WCAG AA body text needs {AA_BODY}:1",
                        theme.token(),
                    );
                }
            }
        }
    }

    /// A new palette field has to declare itself: either it carries text (and the
    /// test above then holds it to AA) or it says why it doesn't.
    #[test]
    fn no_role_escapes_the_contrast_decision() {
        let p = palette(Theme::Light);
        let json = serde_json::to_value(&p).expect("palette serializes");
        let keys = json.as_object().expect("palette is an object");
        let text: Vec<&str> = text_roles(&p).into_iter().map(|(n, _)| n).collect();
        let surf: Vec<&str> = surfaces(&p).into_iter().map(|(n, _)| n).collect();
        for key in keys.keys() {
            let k = key.as_str();
            let known = text.contains(&k) || surf.contains(&k) || NOT_TEXT.iter().any(|(n, _)| *n == k);
            assert!(
                known,
                "palette role `{k}` is new: add it to text_roles() — where it must \
                 clear WCAG AA on every surface — or to NOT_TEXT with the reason it \
                 carries no text"
            );
        }
        assert_eq!(
            keys.len(),
            text.len() + surf.len() + NOT_TEXT.len(),
            "a role is listed twice, or one was removed from Palette but not from these lists"
        );
    }

    /// The maths itself, against values whose ratios are fixed by the spec —
    /// otherwise a broken `linear()` would quietly pass everything.
    #[test]
    fn ratio_matches_the_spec_extremes() {
        assert!((ratio("#000000", "#ffffff") - 21.0).abs() < 0.001);
        assert!((ratio("#ffffff", "#000000") - 21.0).abs() < 0.001);
        assert!((ratio("#777777", "#777777") - 1.0).abs() < 0.001);
        // The canonical AA-boundary grey: #767676 on white is 4.54:1.
        assert!((ratio("#767676", "#ffffff") - 4.54).abs() < 0.01);
    }
}
