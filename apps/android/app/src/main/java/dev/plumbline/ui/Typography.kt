// The app's type: EB Garamond everywhere, chrome included.
//
// Font parity with the web shell. Both shells ship the SAME two
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
// site naming it. Existing `fontSize` values are left ALONE on purpose.
// Face-to-face consistency is [FontSpec.scale]'s job now — [serifTypography]
// multiplies every role by it, so a chrome face switch changes voice, not
// apparent size. Garamond is that scale's 1.0 baseline, so it still reads a
// touch smaller than Roboto did; if that is ever re-tuned, it is re-tuned by
// the drawn sizes (or the baseline), not per call site.
//
// ONCE PER PROCESS, not once per composition. `Font(path, assets)`
// is not a description of a font — `AndroidAssetFont` parses the TTF in its
// CONSTRUCTOR, on whatever thread builds the family — and this was a
// `remember`, which is scoped to one composition. Five call sites build a
// theme (MainActivity's loading + error screens, StudyScreen, FirstRun,
// Present), the boot path crosses two of them, and every Activity recreate —
// a theme change, a rotation, the restore-from-backup path — starts over. The
// platform's own Typeface cache is nine entries shared with everything
// ReaderPane and Maps build, so "cached" is not a promise. Hold the family and
// the Typography over it in a process-wide [Once] instead, and let
// [warmSerifType] pay the parse on a background thread before anything
// composes.
//
// Author D (Compose UI).

package dev.plumbline.ui

import android.content.Context
import android.content.res.AssetManager
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
 * A value built at most once per process, by whichever thread asks first.
 *
 * Double-checked: the common path — every call after the first — is one
 * volatile field read and no monitor at all, while a race between the startup
 * warm and the first composition still builds exactly one value and hands both
 * threads the same instance. Building twice would not be *wrong* here, but it
 * is the whole cost this cache exists to remove, so the lock is the point
 * rather than an afterthought.
 *
 * Lives in this file because the type cache is its only user; unit-tested on
 * its own in TypeCacheTest because the identity and the once-ness are the
 * load-bearing part and neither needs an Android framework to check.
 */
internal class Once<T : Any> {
    @Volatile
    private var held: T? = null

    fun get(build: () -> T): T {
        held?.let { return it }
        return synchronized(this) { held ?: build().also { held = it } }
    }
}

// Keyed by FACE, not a bare singleton: the chrome face is a setting now, so a
// process can legitimately be asked for two of them (a reader switching in
// Settings) and each must be parsed at most once. Still process-wide, still
// holding no Activity — see the note on [serifFamily].
/**
 * [Once], per key: build-at-most-once for each distinct face, held for the
 * process. The reader can switch faces, so a plain singleton would hand back
 * the first family ever built for every later one.
 *
 * The map only ever grows to the number of BUNDLED faces, which is four, so
 * there is nothing to evict.
 */
internal class Keyed<T : Any> {
    private val held = java.util.concurrent.ConcurrentHashMap<String, T>()

    fun get(key: String, build: () -> T): T = held[key] ?: synchronized(this) {
        held[key] ?: build().also { held[key] = it }
    }
}

private val familyCache = Keyed<FontFamily>()
private val typographyCache = Keyed<Typography>()

/**
 * The bundled EB Garamond as a Compose [FontFamily], falling back to the
 * platform serif if the assets are missing. Built once per process.
 *
 * Bold comes off the variable font's `wght` axis rather than the synthetic
 * smear Compose applies when a family has no bold face — the same 700 the web
 * gets from `font-weight: 400 700` on one `@font-face`, and the same axis
 * ReaderPane drives with `setFontVariationSettings("'wght' 700")`.
 *
 * NO ACTIVITY IS RETAINED. What the cached family holds is an [AssetManager],
 * taken from `applicationContext` — the fonts themselves keep no Context (the
 * variable axis here is a plain weight, so Compose never needs a density to
 * resolve it), and an Activity's own AssetManager can be swapped out under a
 * configuration change, which would leave this cache holding a closed one.
 */
fun serifFamily(context: Context, token: String? = null): FontFamily {
    val spec = fontFor(token)
    return familyCache.get(spec.token) { buildSerifFamily(context.applicationContext.assets, spec) }
}

private fun buildSerifFamily(assets: AssetManager, spec: FontSpec): FontFamily = runCatching {
    val faces = mutableListOf<Font>()
    // The regular weight is pinned to 400 EXPLICITLY rather than left to the
    // file's default instance. Fira Code's `wght` axis runs 300–700 and defaults
    // to 300, so a face taken as-shipped would render the Light instance as body
    // text — and it would do it only for that one family, which is the kind of
    // bug that reads as "this font just looks thin".
    faces += Font(
        spec.regular,
        assets,
        weight = FontWeight.Normal,
        variationSettings = FontVariation.Settings(FontVariation.weight(400)),
    )
    // A STATIC family (Atkinson) ships bold as its own file — driving a file
    // with no wght axis to 700 changes nothing, and bold text would paint
    // regular. Variable families keep riding the axis.
    faces += spec.bold?.let { Font(it, assets, weight = FontWeight.Bold) }
        ?: Font(
            spec.regular,
            assets,
            weight = FontWeight.Bold,
            variationSettings = FontVariation.Settings(FontVariation.weight(700)),
        )
    // Only when the family HAS an italic. Compose would otherwise synthesise one
    // for FontStyle.Italic; see FontSpec.
    spec.italic?.let { italic ->
        faces += Font(
            italic,
            assets,
            style = FontStyle.Italic,
            variationSettings = FontVariation.Settings(FontVariation.weight(400)),
        )
        faces += spec.boldItalic?.let { Font(it, assets, weight = FontWeight.Bold, style = FontStyle.Italic) }
            ?: Font(
                italic,
                assets,
                weight = FontWeight.Bold,
                style = FontStyle.Italic,
                variationSettings = FontVariation.Settings(FontVariation.weight(700)),
            )
    }
    FontFamily(faces)
}.getOrElse { FontFamily.Serif }

/** Material 3's own type scale with [serif] substituted into every role, and
 *  every size and line height multiplied by the face's optical [scale]
 *  ([FontSpec.scale]) — so switching the chrome face changes its voice, not its
 *  apparent size. Tracking stays Material's. */
fun serifTypography(serif: FontFamily, scale: Float = 1f): Typography {
    val base = Typography()
    fun TextStyle.on() = copy(fontFamily = serif, fontSize = fontSize * scale, lineHeight = lineHeight * scale)
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

/** [serifTypography] over the bundled family, built once per process — what
 *  every `MaterialTheme` in the app passes as its `typography`. */
fun serifTypography(context: Context, token: String? = null): Typography {
    val spec = fontFor(token)
    return typographyCache.get(spec.token) { serifTypography(serifFamily(context, spec.token), spec.scale) }
}

/** Parse the fonts and build the type scale NOW, off the caller's thread's
 *  own schedule — call it from a background coroutine at startup so the first
 *  composition finds both already built instead of parsing 1.6 MB of TTF on
 *  the main thread. Safe to call from anywhere: the underlying
 *  `Typeface.Builder` is thread-safe, and [Once] makes a lost race free. */
fun warmSerifType(context: Context, chromeFont: String? = null, textFont: String? = null) {
    serifTypography(context, chromeFont)
    // The CANVAS faces too (ReaderPane's three `android.graphics.Typeface`s,
    // which the map popups share). They are a separate parse of the same files
    // — Compose's FontFamily and the platform Typeface do not share a cache —
    // and they are on the path to first paint, which the chrome's typography is
    // not. Warming only half of it would leave the reader waiting for the half
    // that matters.
    readerTypefaces(context, textFont)
}

/** The bundled family for a call site that styles its own text (FirstRun,
 *  Present). A process-wide lookup behind a `remember`, so a recomposition
 *  does not even pay the volatile read. */
@Composable
fun rememberSerifFamily(token: String? = null): FontFamily {
    val context = LocalContext.current
    return remember(context, token) { serifFamily(context, token) }
}

/** The app's `MaterialTheme` typography. */
@Composable
fun rememberSerifTypography(token: String? = null): Typography {
    val context = LocalContext.current
    return remember(context, token) { serifTypography(context, token) }
}
