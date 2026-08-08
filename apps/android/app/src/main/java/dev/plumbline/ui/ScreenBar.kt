// ONE bar for every destination that has one (web twin lib/ScreenBar.svelte).
// Explore, Memorize and the Hymnal each grew their own and drifted, so the same
// heading rendered at different sizes and switching tabs made the chrome jump.
//
// The metrics match the app's TopBar, so a destination's bar reads as the same
// furniture one row down rather than a different screen's idea of a header.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * A destination's header: back arrow, title, and whatever that screen needs on
 * the right. [backLabel] is the arrow's content description — "Back to
 * reading", "Back to the hymn list" — because on a screen with two levels the
 * arrow does not always mean the same thing.
 */
@Composable
fun ScreenBar(
    title: String,
    palette: ReaderPalette,
    onBack: () -> Unit,
    backLabel: String = t("bar.backToReading"),
    actions: @Composable RowScope.() -> Unit = {},
) {
    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 2.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = backLabel, tint = palette.ink)
            }
            Text(
                title,
                color = palette.ink,
                fontSize = 18.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            actions()
        }
    }
    HorizontalDivider(color = palette.rule)
}
