// The app's type: EB Garamond everywhere, chrome included.
//
// Font parity with the web shell (2026-07-28). Both shells ship the SAME two
// files — byte-identical variable TTFs, wght 400–700 — and both already used
// them for scripture. The chrome had drifted: the web sets
// `body { font-family: "EB Garamond" }` (apps/web/src/app.css), so every
// control there is Garamond, while Android called a bare `MaterialTheme { }`
// and inherited Material 3's default typography — Roboto — for BookNav,
// StudyPane, Settings, VerseActions, Notes, TagWeave, Memorize, Church and
// Maps. Only FirstRun and Present had opted in by hand.
//
// The fix is one Typography, applied at the theme, because Material 3's
// `MaterialTheme` provides `typography.bodyLarge` as `LocalTextStyle`: a bare
// `Text("…", fontSize = 15.sp)` picks up the family from there without the call
// site naming it. Existing `fontSize` values are left ALONE on purpose —
// Garamond's x-height is much smaller than Roboto's, so the chrome will read a
// touch smaller until sizes are re-tuned on-device.
//
// No perf cost worth the name: ReaderPane already holds these assets open as
// android.graphics.Typeface for the canvas, so this is a second handle on a
// resident file, not a second load.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight

/**
 * The bundled EB Garamond as a Compose [FontFamily], falling back to the
 * platform serif if the assets are missing.
 *
 * Bold comes off the variable font's `wght` axis rather than the synthetic
 * smear Compose applies when a family has no bold face — the same 700 the web
 * gets from `font-weight: 400 700` on one `@font-face`, and the same axis
 * ReaderPane drives with `setFontVariationSettings("'wght' 700")`.
 */
@Composable
fun rememberSerifFamily(): FontFamily {
    val assets = LocalContext.current.assets
    return remember(assets) {
        runCatching {
            FontFamily(
                Font("fonts/EBGaramond-Regular.ttf", assets, weight = FontWeight.Normal),
                Font(
                    "fonts/EBGaramond-Regular.ttf",
                    assets,
                    weight = FontWeight.Bold,
                    variationSettings = FontVariation.Settings(FontVariation.weight(700)),
                ),
                Font("fonts/EBGaramond-Italic.ttf", assets, style = FontStyle.Italic),
                Font(
                    "fonts/EBGaramond-Italic.ttf",
                    assets,
                    weight = FontWeight.Bold,
                    style = FontStyle.Italic,
                    variationSettings = FontVariation.Settings(FontVariation.weight(700)),
                ),
            )
        }.getOrElse { FontFamily.Serif }
    }
}

/** Material 3's own type scale with [serif] substituted into every role. Sizes,
 *  line heights and tracking are Material's — only the family changes. */
fun serifTypography(serif: FontFamily): Typography {
    val base = Typography()
    fun TextStyle.on() = copy(fontFamily = serif)
    return Typography(
        displayLarge = base.displayLarge.on(),
        displayMedium = base.displayMedium.on(),
        displaySmall = base.displaySmall.on(),
        headlineLarge = base.headlineLarge.on(),
        headlineMedium = base.headlineMedium.on(),
        headlineSmall = base.headlineSmall.on(),
        titleLarge = base.titleLarge.on(),
        titleMedium = base.titleMedium.on(),
        titleSmall = base.titleSmall.on(),
        bodyLarge = base.bodyLarge.on(),
        bodyMedium = base.bodyMedium.on(),
        bodySmall = base.bodySmall.on(),
        labelLarge = base.labelLarge.on(),
        labelMedium = base.labelMedium.on(),
        labelSmall = base.labelSmall.on(),
    )
}

/** [serifTypography] over the bundled family — what every `MaterialTheme` in
 *  the app passes as its `typography`. */
@Composable
fun rememberSerifTypography(): Typography {
    val serif = rememberSerifFamily()
    return remember(serif) { serifTypography(serif) }
}
