// The passage navigator (product feedback, 2026-07-24): the book dropdown
// replaced by tap stages — Testament (OT | NT) → book grid → chapter grid →
// verse grid — every step a big touch target. Tapping a verse jumps the reader
// straight to it (ReaderPane scrolls the verse into view); "Whole chapter"
// skips the verse stage. Fullscreen overlay; back steps a stage, then closes.
//
// Verse counts aren't a core endpoint; a chapter's count is resolved by binary
// search over VerseJson existence (≤ ~9 probes even for Psalm 119) off the main
// thread.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.StudyEngine
import dev.plumbline.TocBook
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Canon position of the OT/NT divide (Gen..Mal = 39 books). The core's
 *  CanonSegments carries the same figure; the canon is frozen, so the constant
 *  avoids an engine round-trip on every navigator open. */
private const val OT_BOOKS = 39

/**
 * The fullscreen passage navigator. [currentBook] preselects the testament tab.
 * [onGo] fires with the chosen (book id, chapter, verse or null for the whole
 * chapter) and the caller closes the overlay.
 */
@Composable
fun BookNavScreen(
    engine: StudyEngine,
    toc: List<TocBook>,
    palette: ReaderPalette,
    currentBook: String,
    onGo: (book: String, chapter: Int, verse: Int?) -> Unit,
    onClose: () -> Unit,
) {
    val currentIdx = toc.indexOfFirst { it.id == currentBook }
    var newTestament by remember { mutableStateOf(currentIdx >= OT_BOOKS) }
    var pickedBook by remember { mutableStateOf<TocBook?>(null) }
    var pickedChapter by remember { mutableStateOf<Int?>(null) }

    fun stepBack() {
        when {
            pickedChapter != null -> pickedChapter = null
            pickedBook != null -> pickedBook = null
            else -> onClose()
        }
    }
    BackHandler(onBack = ::stepBack)

    Column(Modifier.fillMaxSize().background(palette.paper)) {
        Surface(color = palette.paneNavBg) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 2.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = ::stepBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = palette.ink)
                }
                val crumb = pickedBook?.let { b ->
                    b.name + (pickedChapter?.let { " $it" } ?: "")
                } ?: "Go to…"
                Text(crumb, color = palette.ink, fontSize = 17.sp)
                Spacer(Modifier.weight(1f))
                if (pickedBook == null) {
                    TextButton(onClick = { newTestament = false }) {
                        Text("Old Testament", color = if (!newTestament) palette.gold else palette.faded)
                    }
                    TextButton(onClick = { newTestament = true }) {
                        Text("New", color = if (newTestament) palette.gold else palette.faded)
                    }
                }
            }
        }
        HorizontalDivider(color = palette.rule)

        val book = pickedBook
        val chapter = pickedChapter
        when {
            book == null -> BookGrid(
                books = if (newTestament) toc.drop(OT_BOOKS) else toc.take(OT_BOOKS),
                current = currentBook, palette = palette,
                onPick = { pickedBook = it },
            )
            chapter == null -> NumberGrid(
                count = book.chapters.toInt(), palette = palette,
                header = "${book.name} — chapter",
                onPick = { pickedChapter = it },
            )
            else -> VerseGrid(
                engine = engine, book = book, chapter = chapter, palette = palette,
                onPick = { v -> onGo(book.id, chapter, v) },
            )
        }
    }
}

@Composable
private fun BookGrid(
    books: List<TocBook>,
    current: String,
    palette: ReaderPalette,
    onPick: (TocBook) -> Unit,
) {
    LazyVerticalGrid(
        columns = GridCells.Fixed(3),
        modifier = Modifier.fillMaxSize().padding(horizontal = 10.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(books) { b ->
            val active = b.id == current
            Box(
                Modifier
                    .border(1.dp, if (active) palette.gold else palette.rule, RoundedCornerShape(8.dp))
                    .background(if (active) palette.band else palette.paper, RoundedCornerShape(8.dp))
                    .clickable { onPick(b) }
                    .padding(vertical = 14.dp, horizontal = 4.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    b.name, color = palette.ink, fontSize = 14.sp,
                    textAlign = TextAlign.Center, maxLines = 2,
                )
            }
        }
    }
}

/** A tappable grid of 1..[count] (chapters). */
@Composable
private fun NumberGrid(
    count: Int,
    palette: ReaderPalette,
    header: String,
    onPick: (Int) -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Text(
            header, color = palette.faded, fontSize = 13.sp,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
        )
        LazyVerticalGrid(
            columns = GridCells.Adaptive(minSize = 56.dp),
            modifier = Modifier.fillMaxSize().padding(horizontal = 10.dp),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items((1..count).toList()) { n ->
                NumberCell(n.toString(), palette) { onPick(n) }
            }
        }
    }
}

/** The verse stage: "Whole chapter" first, then every verse as a tap target. */
@Composable
private fun VerseGrid(
    engine: StudyEngine,
    book: TocBook,
    chapter: Int,
    palette: ReaderPalette,
    onPick: (verse: Int?) -> Unit,
) {
    var count by remember(book.id, chapter) { mutableStateOf<Int?>(null) }
    LaunchedEffect(book.id, chapter) {
        count = withContext(Dispatchers.Default) {
            fun exists(v: Int): Boolean = runCatching {
                synchronized(engine) { engine.VerseJson("${book.id} $chapter:$v") }
            }.getOrNull() != null
            if (!exists(1)) {
                0
            } else {
                var lo = 1          // known to exist
                var hi = 200        // > Psalm 119's 176 — known not to exist
                while (hi - lo > 1) {
                    val mid = (lo + hi) / 2
                    if (exists(mid)) lo = mid else hi = mid
                }
                lo
            }
        }
    }

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("${book.name} $chapter — verse", color = palette.faded, fontSize = 13.sp)
            Spacer(Modifier.weight(1f))
            TextButton(onClick = { onPick(null) }) { Text("Whole chapter", color = palette.gold) }
        }
        val n = count
        if (n == null) {
            Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
                Text("…", color = palette.faded)
            }
        } else {
            LazyVerticalGrid(
                columns = GridCells.Adaptive(minSize = 56.dp),
                modifier = Modifier.fillMaxSize().padding(horizontal = 10.dp),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items((1..n).toList()) { v ->
                    NumberCell(v.toString(), palette) { onPick(v) }
                }
            }
        }
    }
}

@Composable
private fun NumberCell(label: String, palette: ReaderPalette, onClick: () -> Unit) {
    Box(
        Modifier
            .height(52.dp)
            .border(1.dp, palette.rule, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = palette.ink, fontSize = 16.sp)
    }
}
