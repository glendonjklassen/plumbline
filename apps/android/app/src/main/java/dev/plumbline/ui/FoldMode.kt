// The fold-aware layout decision. One derived [UiMode] from
// (window width breakpoint + FoldingFeature present? + posture). Per
// docs/ANDROID-BOOTSTRAP.md we NEVER gate two-pane on width alone — the target
// foldable's inner display is ~1:1 and may not clear the 840dp "Expanded"
// breakpoint, so a present (vertical, opened) FoldingFeature is the signal.
//
// Phone shell: the phone is always a single fullscreen
// reader — study/search/library surface on demand as a dismissible bottom sheet,
// never a permanent split with a toggle button. Two side-by-side panes appear
// only when the fold is opened flat with a vertical hinge.
//
// Author D (Compose UI). Depends only on androidx.window + material3-adaptive.

package dev.plumbline.ui

import androidx.compose.material3.adaptive.ExperimentalMaterial3AdaptiveApi
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.window.core.layout.WindowWidthSizeClass
import androidx.window.layout.FoldingFeature

// Wire JSON is decoded through Author B's shared `parseWire` / `PlumblineJson`
// (package dev.plumbline) — this shell adds no second codec.

/** The two fold-aware layouts.
 *  - [FullscreenVertical] a plain phone / closed cover / tabletop posture: one
 *    fullscreen reading pane. Study, search, and libraries open as a dismissible
 *    bottom sheet over it.
 *  - [FoldFullscreen]     device opened flat with a vertical hinge: two panes
 *    side-by-side (Bible∥Bible or Bible∥Study), split at the hinge. */
enum class UiMode { FullscreenVertical, FoldFullscreen }

/**
 * The layout implied by the hardware posture. Opened flat with a vertical hinge
 * → two side-by-side pages ([FoldFullscreen]); anything else → a single
 * fullscreen reader ([FullscreenVertical]).
 */
@OptIn(ExperimentalMaterial3AdaptiveApi::class)
@Composable
fun rememberUiMode(fold: FoldingFeature?): UiMode {
    // Read the width breakpoint so the decision is a function of it too (spec),
    // even though we deliberately do NOT let width alone open two panes.
    val widthClass: WindowWidthSizeClass =
        currentWindowAdaptiveInfo().windowSizeClass.windowWidthSizeClass
    return remember(fold?.state, fold?.orientation, fold?.isSeparating, widthClass) {
        computeUiMode(fold, widthClass)
    }
}

/** Pure decision function (unit-testable, no Compose state). */
internal fun computeUiMode(
    fold: FoldingFeature?,
    @Suppress("UNUSED_PARAMETER") widthClass: WindowWidthSizeClass,
): UiMode {
    if (fold != null) {
        val verticalHinge = fold.orientation == FoldingFeature.Orientation.VERTICAL
        val opened = fold.state == FoldingFeature.State.FLAT ||
            fold.state == FoldingFeature.State.HALF_OPENED
        // Opened flat/book with a vertical hinge → left/right pages, side-by-side.
        if (verticalHinge && opened) return UiMode.FoldFullscreen
    }
    // No separating vertical hinge (plain phone, closed cover, tabletop) → a
    // single fullscreen reader; the study surface is a bottom sheet on demand.
    return UiMode.FullscreenVertical
}
