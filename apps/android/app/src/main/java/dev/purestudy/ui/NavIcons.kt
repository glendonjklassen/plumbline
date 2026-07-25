// The bottom navigation bar's four icons as hand-built ImageVectors (standard
// Material Symbols path data). material-icons-core doesn't carry these glyphs
// and material-icons-extended would bloat the unminified release APK, so the
// four paths live here instead.
//
// Author D (Compose UI).

package dev.purestudy.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

private fun navIcon(name: String, pathData: String): ImageVector =
    ImageVector.Builder(
        name = name,
        defaultWidth = 24.dp, defaultHeight = 24.dp,
        viewportWidth = 24f, viewportHeight = 24f,
    ).addPath(pathData = addPathNodes(pathData), fill = SolidColor(Color.Black)).build()

/** Material "book" — the Read tab. */
val NavIconRead: ImageVector by lazy {
    navIcon(
        "nav-read",
        "M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" +
            "M6 4h5v8l-2.5-1.5L6 12V4z",
    )
}

/** Material "explore" (compass) — the Explore tab. */
val NavIconExplore: ImageVector by lazy {
    navIcon(
        "nav-explore",
        "M12 10.9c-.61 0-1.1.49-1.1 1.1s.49 1.1 1.1 1.1c.61 0 1.1-.49 1.1-1.1s-.49-1.1-1.1-1.1z" +
            "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z" +
            "m2.19 12.19L6 18l3.81-8.19L18 6l-3.81 8.19z",
    )
}

/** Material "present_to_all" — the Present tab. */
val NavIconPresent: ImageVector by lazy {
    navIcon(
        "nav-present",
        "M21 3H3c-1.11 0-2 .89-2 2v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2z" +
            "m0 16.02H3V4.98h18v14.04zM10 12H8l4-4 4 4h-2v4h-4v-4z",
    )
}

/** Material "school" — the Memorize tab. */
val NavIconMemorize: ImageVector by lazy {
    navIcon(
        "nav-memorize",
        "M5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82zM12 3L1 9l11 6 11-6-11-6z",
    )
}
