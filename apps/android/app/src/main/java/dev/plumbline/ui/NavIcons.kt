// The bottom navigation bar's five role icons as hand-built ImageVectors
// (standard Material Symbols path data). material-icons-core doesn't carry
// these glyphs and material-icons-extended would bloat the unminified release
// APK, so the paths live here instead. The web's NAV table carries the same
// paths — parity is glyph-for-glyph, not just label-for-label.
//
// Author D (Compose UI).

package dev.plumbline.ui

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

/** Material "share" — the Share tab. */
val NavIconShare: ImageVector by lazy {
    navIcon(
        "nav-share",
        "M18 16.08c-.76 0-1.44.3-1.96.77L8.91 12.7c.05-.23.09-.46.09-.7s-.04-.47-.09-.7l7.05-4.11" +
            "c.54.5 1.25.81 2.04.81 1.66 0 3-1.34 3-3s-1.34-3-3-3-3 1.34-3 3c0 .24.04.47.09.7L8.04 9.81" +
            "C7.5 9.31 6.79 9 6 9c-1.66 0-3 1.34-3 3s1.34 3 3 3c.79 0 1.5-.31 2.04-.81l7.12 4.16" +
            "c-.05.21-.08.43-.08.65 0 1.61 1.31 2.92 2.92 2.92s2.92-1.31 2.92-2.92-1.31-2.92-2.92-2.92z",
    )
}

/** Material "present_to_all" — the Preach tab. */
val NavIconPresent: ImageVector by lazy {
    navIcon(
        "nav-present",
        "M21 3H3c-1.11 0-2 .89-2 2v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2z" +
            "m0 16.02H3V4.98h18v14.04zM10 12H8l4-4 4 4h-2v4h-4v-4z",
    )
}

/** Material "school" — the Study tab. */
val NavIconStudy: ImageVector by lazy {
    navIcon(
        "nav-study",
        "M5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82zM12 3L1 9l11 6 11-6-11-6z",
    )
}

/** Material "music_note" — the Sing tab. */
val NavIconHymnal: ImageVector by lazy {
    navIcon(
        "nav-hymnal",
        "M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z",
    )
}
