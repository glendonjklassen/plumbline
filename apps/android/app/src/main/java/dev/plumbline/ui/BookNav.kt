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
import androidx.compose.ui.draw.drawBehind
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
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.ReadingBook
import dev.plumbline.ReadingBooks
import dev.plumbline.ReadingChapter
import dev.plumbline.ReadingChapters
import dev.plumbline.StudyEngine
import dev.plumbline.TocBook
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Canon position of the OT/NT divide (Gen..Mal = 39 books). The core's
 *  CanonSegments carries the same figure; the canon is frozen, so the constant
 *  avoids an engine round-trip on every navigator open. */
private const val OT_BOOKS = 39

/**
 * The fullscreen passage navigator. [currentBook] preselects the testament tab.
 * [onGo] fires with the chosen (book id, chapter) — the verse is always null
 * now that the navigator stops at the chapter — and the caller closes it.
 *
 * Both grids carry the **reading map** (core::reading): the tile's hue is where
 * that book or chapter stands (gold unopened, copper partway, sage read through)
 * and its bloom is how long it has been. See [readingTint].
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

    // The reading map. Fetched once per navigator open — it is a whole-canon
    // roll-up, and nothing can change it while the navigator is up.
    var books by remember { mutableStateOf<Map<String, ReadingBook>>(emptyMap()) }
    LaunchedEffect(Unit) {
        books = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.ReadingBooksJson(nowUtc()) } }.getOrNull()
                ?.let { runCatching { parseWire<ReadingBooks>(it).books.associateBy { b -> b.book } }.getOrNull() }
        } ?: emptyMap()
    }
    // Chapters for the book on screen, fetched when one is picked.
    var chapters by remember { mutableStateOf<Map<Int, ReadingChapter>>(emptyMap()) }
    LaunchedEffect(pickedBook?.id) {
        val id = pickedBook?.id
        chapters = if (id == null) {
            emptyMap()
        } else {
            withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.ReadingChaptersJson(id, nowUtc()) } }.getOrNull()
                    ?.let {
                        runCatching {
                            parseWire<ReadingChapters>(it).chapters.associateBy { c -> c.chapter }
                        }.getOrNull()
                    }
            } ?: emptyMap()
        }
    }

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
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = t("bar.back"), tint = palette.ink)
                }
                val crumb = pickedBook?.name ?: t("booknav.goTo")
                Text(crumb, color = palette.ink, fontSize = 17.sp)
                Spacer(Modifier.weight(1f))
                if (pickedBook == null) {
                    TextButton(onClick = { newTestament = false }) {
                        Text(t("booknav.old"), color = if (!newTestament) palette.gold else palette.faded)
                    }
                    TextButton(onClick = { newTestament = true }) {
                        Text(t("booknav.new"), color = if (newTestament) palette.gold else palette.faded)
                    }
                }
            }
        }
        HorizontalDivider(color = palette.rule)

        val book = pickedBook
        if (book == null) {
            BookGrid(
                books = if (newTestament) toc.drop(OT_BOOKS) else toc.take(OT_BOOKS),
                current = currentBook, palette = palette, heat = books,
                onPick = { pickedBook = it },
            )
        } else {
            // Chapter counts ride in on the TOC, so this grid is instant; the
            // reading tint fills in a frame later without moving anything.
            NumberGrid(
                count = book.chapters.toInt(), palette = palette,
                header = "${book.name} — chapter",
                tint = { n ->
                    chapters[n]?.let { readingTint(palette, it.standing, it.pct, it.glow) }
                },
                onPick = { chapter -> onGo(book.id, chapter, null) },
            )
        }
    }
}

/** How a reading-map tile paints: a fill, a border, and how bright the bloom is.
 *  `glow` is the bloom — 0 for something read this month, 1 for a year untouched
 *  (or, for something never read, a year since the reader started). */
internal data class ReadingTint(val fill: Color, val border: Color, val glow: Float)

/**
 * Resolve one chapter's or book's standing into paint.
 *
 * The **hue** says where you stand; the **strength** says how loudly to say so.
 * A chapter read last week is barely tinted at all — that is the whole design,
 * because the map exists to point at what you have drifted away from, and a
 * uniformly loud grid points at nothing. `pct` deepens the amber of a partway
 * chapter so progress within it is visible without a number.
 */
internal fun readingTint(
    palette: ReaderPalette,
    standing: String,
    pct: Float,
    glow: Float,
): ReadingTint {
    val base = when (standing) {
        "read" -> palette.readDone
        "partial" -> palette.readPartial
        else -> palette.readUnread
    }
    // A floor so the hue is legible before any glow, and a partway chapter
    // deepens with its own progress.
    val presence = when (standing) {
        "partial" -> 0.16f + 0.24f * pct.coerceIn(0f, 1f)
        else -> 0.10f
    }
    val strength = (presence + glow * 0.42f).coerceIn(0f, 0.72f)
    return ReadingTint(
        fill = base.copy(alpha = strength * 0.42f),
        border = base.copy(alpha = (0.28f + strength * 0.72f).coerceAtMost(1f)),
        glow = glow,
    )
}

/** The bloom: concentric rounded outlines fading outward, standing in for a
 *  shadow Compose won't tint. Drawn only when there is something to say. */
private fun Modifier.readingGlow(tint: ReadingTint?, radius: Dp): Modifier {
    if (tint == null || tint.glow <= 0.02f) return this
    return drawBehind {
        val rings = 3
        for (i in rings downTo 1) {
            val spread = (i * 2.5f).dp.toPx()
            val alpha = tint.glow * 0.26f / i
            drawRoundRect(
                color = tint.border.copy(alpha = alpha),
                topLeft = Offset(-spread, -spread),
                size = Size(size.width + spread * 2, size.height + spread * 2),
                cornerRadius = CornerRadius(radius.toPx() + spread),
                style = Stroke(width = 1.5f.dp.toPx()),
            )
        }
    }
}

@Composable
private fun BookGrid(
    books: List<TocBook>,
    current: String,
    palette: ReaderPalette,
    heat: Map<String, ReadingBook>,
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
            val h = heat[b.id]
            val tint = h?.let { readingTint(palette, it.standing, it.pct, it.glow) }
            Box(
                Modifier
                    .readingGlow(tint, 8.dp)
                    // The gold "you are here" border always wins: where the
                    // reader IS matters more than where they have been.
                    .border(
                        1.dp,
                        if (active) palette.gold else tint?.border ?: palette.rule,
                        RoundedCornerShape(8.dp),
                    )
                    .background(
                        if (active) palette.band else tint?.fill ?: palette.paper,
                        RoundedCornerShape(8.dp),
                    )
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

/** A tappable grid of 1..[count] (chapters). [tint] supplies the reading-map
 *  paint per number, or null before it has loaded. */
@Composable
private fun NumberGrid(
    count: Int,
    palette: ReaderPalette,
    header: String,
    tint: (Int) -> ReadingTint? = { null },
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
                NumberCell(n.toString(), palette, tint(n)) { onPick(n) }
            }
        }
    }
}

@Composable
private fun NumberCell(
    label: String,
    palette: ReaderPalette,
    tint: ReadingTint? = null,
    onClick: () -> Unit,
) {
    Box(
        Modifier
            .height(52.dp)
            .readingGlow(tint, 8.dp)
            .border(1.dp, tint?.border ?: palette.rule, RoundedCornerShape(8.dp))
            .background(tint?.fill ?: Color.Transparent, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = palette.ink, fontSize = 16.sp)
    }
}
