// The bundled faces, and which asset files each one is.
//
// The TOKENS are `plumbline_core::font::Font`'s — the same strings the config
// stores (`textFont` / `chromeFont`) and the same ones the web's generated
// registry uses, so a face cannot be called one thing here and another there.
// The core owns the vocabulary; a shell owns only the files, because how a font
// is delivered is a platform concern (an asset in the APK, a subset woff2 on
// the web).
//
// Type and colour are INDEPENDENT axes: nothing here consults the theme, and
// [Palette] knows nothing about faces. Every combination is legal.
package dev.plumbline.ui

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * One bundled family.
 *
 * [italic] is null for a face that ships none — Fira Code. The reader still
 * tells translator-supplied words apart, by the palette's `added` tone; what it
 * must NOT get is a synthesised italic, which is a sheared upright and looks
 * like one. This mirrors `Font::has_italic` in the core.
 */
internal data class FontSpec(
    val token: String,
    val displayName: String,
    val regular: String,
    val italic: String?,
)

/**
 * Every face the pickers offer, in the order they offer them: the shipped
 * default first, then the alternatives — the same order as `Font::ALL`.
 *
 * [displayName] is the typeface's own name and is deliberately NOT in the i18n
 * catalogue: a typeface name is a proper noun, identical in every language the
 * app will speak.
 */
internal val BUNDLED_FONTS: List<FontSpec> = listOf(
    FontSpec("eb-garamond", "EB Garamond", "fonts/EBGaramond-Regular.ttf", "fonts/EBGaramond-Italic.ttf"),
    FontSpec("literata", "Literata", "fonts/Literata-Regular.ttf", "fonts/Literata-Italic.ttf"),
    FontSpec("inter", "Inter", "fonts/Inter-Regular.ttf", "fonts/Inter-Italic.ttf"),
    // No italic entry: the file does not exist, and asking for one would get a shear.
    FontSpec("fira-code", "Fira Code", "fonts/FiraCode-Regular.ttf", null),
)

/** The face everything falls back to — the shipped default. */
internal val DEFAULT_FONT: FontSpec = BUNDLED_FONTS.first()

/**
 * A config token to its face. An unknown token resolves to the default rather
 * than to nothing: it means a hand-edited config, or one written by a LATER
 * build that shipped a face this APK does not have — and a sideloaded APK never
 * auto-updates, so that is a real case rather than a theoretical one. The reader
 * is owed type they can read.
 */
internal fun fontFor(token: String?): FontSpec =
    BUNDLED_FONTS.firstOrNull { it.token == token } ?: DEFAULT_FONT

/**
 * The SCRIPTURE face for everything under the current theme.
 *
 * A composition local rather than a parameter threaded through ReaderPane,
 * Present, Memorize and the maps: those are many call sites for one value that
 * never varies within a screen, and a defaulted parameter that a positional
 * call forgets is exactly the failure StudyScreen's own "NAMED, not positional"
 * note warns about. Provided once, beside the palette.
 *
 * Static, because a face change recreates the Activity anyway — nothing needs
 * the fine-grained invalidation a dynamic local would pay for.
 */
internal val LocalTextFont = staticCompositionLocalOf { DEFAULT_FONT.token }
