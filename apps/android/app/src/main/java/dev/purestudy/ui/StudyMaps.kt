// The study panel's embedded maps (Glendon's feedback, 2026-07-24): the concept
// map and the across-the-canon heatmap as first-class cards INSIDE the word
// study, scaled down — not just links to fullscreen overlays. The radial card
// taps through to the fullscreen (zoomable) concept map; tapping a book column
// on the heatmap jumps the reader there. Painting is shared with Maps.kt
// (drawConceptRadial / drawDispersionStrip), so the geometry matches the
// fullscreen views and the desktop shells exactly.
//
// Author D (Compose UI).

package dev.purestudy.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.purestudy.ConceptMapData
import dev.purestudy.StudyEngine
import dev.purestudy.TocBook
import dev.purestudy.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.max

/**
 * The two embedded map cards for [code], rendered inside the study pane's block
 * flow. [onOpenFull] opens the fullscreen concept map; [onGoBook] jumps the
 * reader to a tapped book (heatmap column).
 */
@Composable
fun StudyMapCards(
    engine: StudyEngine,
    code: String,
    palette: ReaderPalette,
    books: List<TocBook>,
    onOpenFull: () -> Unit,
    onGoBook: (bookId: String) -> Unit,
) {
    val paint = rememberMapPaint()
    var data by remember(code) { mutableStateOf<ConceptMapData?>(null) }

    // First call builds the analytics engine (~seconds, warmed at startup);
    // always off the main thread.
    LaunchedEffect(code) {
        data = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) { engine.ConceptMapJson(code) }
                    ?.let { parseWire<ConceptMapData>(it) }
            }.getOrNull()
        }
    }

    val map = data ?: return
    Column(Modifier.fillMaxWidth()) {
        SectionLabel("CONCEPT MAP · tap to expand", palette)
        Canvas(
            Modifier.fillMaxWidth().height(210.dp)
                .padding(top = 4.dp)
                .clickable(onClick = onOpenFull),
        ) {
            drawConceptRadial(map, paint, palette, size.height)
        }

        if (map.byBook.any { it > 0 } || map.bridge?.byBook?.any { it > 0 } == true) {
            SectionLabel("ACROSS THE BIBLE · tap a book", palette)
            Canvas(
                Modifier.fillMaxWidth().height(64.dp)
                    .padding(top = 4.dp)
                    .pointerInput(map, books) {
                        detectTapGestures { pos ->
                            val bc = max(1, map.bookCount)
                            val idx = (pos.x / size.width * bc).toInt().coerceIn(0, bc - 1)
                            if (idx in books.indices) onGoBook(books[idx].id)
                        }
                    },
            ) {
                drawDispersionStrip(map, paint, palette, 0f, size.height, caption = null)
            }
            val partners = map.bridge?.partners.orEmpty()
            if (partners.isNotEmpty()) {
                Text(
                    "↔ across testaments: " + partners.joinToString(", ") { partnerName(it.label) },
                    color = palette.faded, fontSize = 11.sp,
                    modifier = Modifier.padding(top = 3.dp),
                )
            }
        }
    }
}

/** The pane's section-header voice (mirrors StudyPane.SectionBlock). */
@Composable
private fun SectionLabel(text: String, palette: ReaderPalette) {
    Text(
        text, color = palette.sectionGold, fontWeight = FontWeight.Bold,
        fontSize = 11.sp, letterSpacing = 1.2.sp,
        modifier = Modifier.padding(top = 10.dp),
    )
}
