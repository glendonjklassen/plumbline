// The fold-aware layout decision. One derived [UiMode] from
// (window width breakpoint + FoldingFeature present? + posture), mapping to the
// three modes in docs/ANDROID-BOOTSTRAP.md. Per that doc we NEVER gate two-pane
// on width alone — the Pixel 9 Pro Fold's inner display is ~1:1 and may not clear
// the 840dp "Expanded" breakpoint, so a present FoldingFeature is the signal.
//
// Author D (Compose UI). Depends only on androidx.window + material3-adaptive.

package dev.purestudy.ui

import androidx.compose.material3.adaptive.ExperimentalMaterial3AdaptiveApi
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.window.core.layout.WindowWidthSizeClass
import androidx.window.layout.FoldingFeature

// Wire JSON is decoded through Author B's shared `parseWire` / `PureJson`
// (package dev.purestudy) — this shell adds no second codec.

/** The three fold-aware layouts (docs/ANDROID-BOOTSTRAP.md §"The three fold modes").
 *  - [SplitVertical]      folded/portrait, stacked halves: Bible over Study.
 *  - [FullscreenVertical] folded/portrait, one pane; a Bible↔Study toggle.
 *  - [FoldFullscreen]     device opened flat with a vertical hinge: two panes
 *                         side-by-side (Bible∥Bible or Bible∥Study). */
enum class UiMode { SplitVertical, FullscreenVertical, FoldFullscreen }

/**
 * The *base* mode implied by the hardware posture. The choice between
 * [SplitVertical] and [FullscreenVertical] is a user preference layered on top by
 * [StudyScreen]; this function only decides whether the device is opened flat
 * (→ [FoldFullscreen]) or not (→ [SplitVertical] baseline).
 *
 * A missing FoldingFeature ("closed cover screen" / a plain phone) is a compact
 * window → modes 1/2, never the two-pane fold mode.
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
        // A horizontal hinge (tabletop posture) → stacked halves above/below it.
        return UiMode.SplitVertical
    }
    // No hinge reported. Baseline is the stacked reader; the user can collapse it
    // to a single fullscreen pane (mode 2) via the top-bar toggle.
    return UiMode.SplitVertical
}
