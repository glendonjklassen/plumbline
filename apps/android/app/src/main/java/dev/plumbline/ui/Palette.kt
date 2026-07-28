// The reader's colour palette, resolved from the core (single source of truth:
// core::theme, fetched as JSON via StudyEngine.PaletteJson). Mirrors the WinUI
// `Palette` (apps/windows/PureStudyWin/ReaderView.cs): the same semantic roles,
// the same derived alpha variants (goldFaint α0.30, band α0.12, gutterDot α0.75,
// pinBand from Pin α0.22), and the same `wash()` alpha. Light is the default so
// the reader looks right before a theme is applied; dark/night resolve on demand.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.ui.graphics.Color
import dev.plumbline.PaletteData
import dev.plumbline.StudyEngine
import dev.plumbline.parseWire

/** A fully-resolved reader palette as Compose [Color]s. Immutable; rebuild on a
 *  theme change (`ReaderPalette.forTheme("dark")`). */
data class ReaderPalette(
    val dark: Boolean,
    val paper: Color,
    val ink: Color,
    val inkFaded: Color,   // added-word gray (maps the wire `added` role)
    val faded: Color,
    val divine: Color,
    val titleInk: Color,
    val gold: Color,
    val goldFaint: Color,  // α0.30 of gold — study-panel rules (was the
                           // Strong's underline, removed 2026-07-28)
    val band: Color,       // search-hit / goto band (α0.12 of gold)
    val gutterDot: Color,  // weave-partner gutter mark (α0.75 of gold)
    val pinBand: Color,    // pinned span (blue Pin α0.22)
    val panelBg: Color,
    val paneNavBg: Color,
    val stripBg: Color,
    val rule: Color,
    val sectionGold: Color,
    val disputed: Color,
    val mono: Color,
    val morph: Color,
    val lemma: Color,
    val tierGod: Color,
    val tierHuman: Color,
    val tierMachine: Color,
    val tierResearch: Color,
) {
    /** A verse-highlight wash: the tag tone at a soft alpha behind the text
     *  (mirrors WinUI `Palette.Wash`: α64 dark / α92 light). */
    fun wash(tone: Color): Color = tone.copy(alpha = (if (dark) 64 else 92) / 255f)

    /** A semantic panel-run role → a palette colour (mirrors StudyPanel.ColorOf).
     *  Every shell maps these identically so the study panel reads the same. */
    fun role(name: String?): Color = when (name) {
        "faded" -> faded
        "gold" -> gold
        "section" -> sectionGold
        "tierGod" -> tierGod
        "tierHuman" -> tierHuman
        "tierMachine" -> tierMachine
        "tierResearch" -> tierResearch
        "mono" -> mono
        "morph" -> morph
        "lemma" -> lemma
        else -> ink // "ink" and unknown roles inherit the body ink
    }

    companion object {
        /** Parse `#rrggbb` (opaque); falls back to ink on a malformed value. */
        fun hex(h: String): Color = runCatching {
            val s = h.trimStart('#')
            Color(
                red = s.substring(0, 2).toInt(16),
                green = s.substring(2, 4).toInt(16),
                blue = s.substring(4, 6).toInt(16),
                alpha = 255,
            )
        }.getOrElse { Color(red = 0x21, green = 0x1F, blue = 0x1A) }

        private fun withAlpha(c: Color, a: Int): Color = c.copy(alpha = a / 255f)

        /** Build from the core's theme palette JSON (`StudyEngine.PaletteJson`). */
        fun fromJson(json: String): ReaderPalette {
            val p = parseWire<PaletteData>(json)
            val gold = hex(p.gold)
            return ReaderPalette(
                dark = p.dark,
                paper = hex(p.paper),
                ink = hex(p.ink),
                inkFaded = hex(p.added),
                faded = hex(p.faded),
                divine = hex(p.divine),
                titleInk = hex(p.titleInk),
                gold = gold,
                goldFaint = withAlpha(gold, 77),
                band = withAlpha(gold, 31),
                gutterDot = withAlpha(gold, 191),
                pinBand = withAlpha(hex(p.pin), 56),
                panelBg = hex(p.popupPaper),
                paneNavBg = hex(p.paneNavBg),
                stripBg = hex(p.stripBg),
                rule = hex(p.rule),
                sectionGold = hex(p.section),
                disputed = hex(p.tierResearch),
                mono = hex(p.mono),
                morph = hex(p.morph),
                lemma = hex(p.lemma),
                tierGod = hex(p.tierGod),
                tierHuman = hex(p.tierHuman),
                tierMachine = hex(p.tierMachine),
                tierResearch = hex(p.tierResearch),
            )
        }

        /** Resolve the palette for a theme token (`light`/`dark`/`night`). */
        fun forTheme(theme: String): ReaderPalette =
            runCatching { fromJson(StudyEngine.PaletteJson(theme)) }.getOrElse { default() }

        /** The shipped light defaults, matching WinUI's field initialisers — so
         *  the reader renders correctly even before the core palette is fetched. */
        fun default(): ReaderPalette {
            fun c(r: Int, g: Int, b: Int) = Color(red = r, green = g, blue = b)
            val gold = c(0x9E, 0x7D, 0x38)
            return ReaderPalette(
                dark = false,
                paper = c(0xFC, 0xF9, 0xF4),
                ink = c(0x21, 0x1F, 0x1A),
                inkFaded = c(0x6B, 0x68, 0x62),
                faded = c(0x8A, 0x82, 0x76),
                divine = c(0x4D, 0x33, 0x26),
                titleInk = c(0x66, 0x5C, 0x4D),
                gold = gold,
                goldFaint = withAlpha(gold, 77),
                band = withAlpha(gold, 31),
                gutterDot = withAlpha(gold, 191),
                pinBand = withAlpha(c(0x40, 0x73, 0xBF), 56),
                panelBg = c(0xF2, 0xEE, 0xE6),
                paneNavBg = c(0xEF, 0xEA, 0xE1),
                stripBg = c(0xEB, 0xE6, 0xDB),
                rule = c(0xD8, 0xCB, 0xA8),
                sectionGold = c(0xA0, 0x89, 0x4A),
                disputed = c(0xB0, 0x4A, 0x3A),
                mono = c(0x88, 0x88, 0x88),
                morph = c(0x6A, 0x5A, 0x2A),
                lemma = c(0x8A, 0x7A, 0x52),
                tierGod = gold,
                tierHuman = c(0x6F, 0x8F, 0x6A),
                tierMachine = c(0x99, 0x99, 0x99),
                tierResearch = c(0xB0, 0x4A, 0x3A),
            )
        }
    }
}
