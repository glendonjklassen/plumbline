// The passage navigator (product feedback, 2026-07-24): the book dropdown
// replaced by tap stages — Testament (OT | NT) → book grid → chapter grid —
// every step a big touch target. Fullscreen overlay; back steps a stage, then
// closes.
//
// The verse stage was dropped 2026-07-26: book and chapter is the navigation
// people actually use, and verse counts aren't a core endpoint — the stage had
// to binary-search VerseJson existence to size its grid, so every chapter tap
// showed a "…" while the probes ran. Verse targeting still arrives through
// links, cross-references and search, which already carry a verse.
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
import dev.plumbline.TocBook

/** Canon position of the OT/NT divide (Gen..Mal = 39 books). The core's
 *  CanonSegments carries the same figure; the canon is frozen, so the constant
 *  avoids an engine round-trip on every navigator open. */
private const val OT_BOOKS = 39

/**
 * The fullscreen passage navigator. [currentBook] preselects the testament tab.
 * [onGo] fires with the chosen (book id, chapter) — the verse is always null
 * now that the navigator stops at the chapter — and the caller closes it.
 */
@Composable
fun BookNavScreen(
    toc: List<TocBook>,
    palette: ReaderPalette,
    currentBook: String,
    onGo: (book: String, chapter: Int, verse: Int?) -> Unit,
    onClose: () -> Unit,
) {
    val currentIdx = toc.indexOfFirst { it.id == currentBook }
    var newTestament by remember { mutableStateOf(currentIdx >= OT_BOOKS) }
    var pickedBook by remember { mutableStateOf<TocBook?>(null) }

    fun stepBack() {
        when {
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
                val crumb = pickedBook?.name ?: "Go to…"
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
        if (book == null) {
            BookGrid(
                books = if (newTestament) toc.drop(OT_BOOKS) else toc.take(OT_BOOKS),
                current = currentBook, palette = palette,
                onPick = { pickedBook = it },
            )
        } else {
            // Chapter counts ride in on the TOC, so this grid is instant.
            NumberGrid(
                count = book.chapters.toInt(), palette = palette,
                header = "${book.name} — chapter",
                onPick = { chapter -> onGo(book.id, chapter, null) },
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
