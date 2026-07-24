// One reading column: a Compose Canvas that lets the Rust core do the layout.
// This is the Android mirror of apps/windows/PureStudyWin/ReaderView.cs — the
// core's line-breaker measures text through our Paint-backed callback and hands
// back a display list + per-word hit regions; this composable paints the items
// and forwards tap coordinates back for hit-testing. Same thin-shell contract
// the GTK (Pango) and WinUI (DirectWrite) shells follow. Constants mirror the
// feature manifest (MARGIN 28, MAX_COLUMN 720, line_height = textH·1.35).
//
// Author D (Compose UI).

package dev.purestudy.ui

import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberScrollableState
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.sp
import dev.purestudy.Chapter
import dev.purestudy.ChapterHighlights
import dev.purestudy.DisplayItem
import dev.purestudy.DisplayList
import dev.purestudy.Hit
import dev.purestudy.PureFlags
import dev.purestudy.StudyEngine
import dev.purestudy.core.PureLayoutConfig
import dev.purestudy.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

private const val MARGIN = 28f      // GTK MARGIN — all sides
private const val MAX_COLUMN = 720f // GTK MAX_COLUMN

/** A pinned word span (single tap sets a one-word anchor; parity-lite with the
 *  desktop pin — full widen-on-second-tap is a TODO). */
private data class PinSpan(val verse: String, val lo: Int, val hi: Int)

/** A word-precise wash run resolved to a colour. */
private data class HighlightRunUi(val verse: String, val lo: Int, val hi: Int, val color: Color)

private data class ReaderTypefaces(val regular: Typeface, val italic: Typeface, val bold: Typeface)

private fun loadTypefaces(context: Context): ReaderTypefaces {
    fun asset(path: String): Typeface? =
        runCatching { Typeface.createFromAsset(context.assets, path) }.getOrNull()
    // Bundled EB Garamond (like the desktop shells); fall back to the platform serif.
    val regular = asset("fonts/EBGaramond-Regular.ttf") ?: Typeface.SERIF
    val italic = asset("fonts/EBGaramond-Italic.ttf")
        ?: Typeface.create(Typeface.SERIF, Typeface.ITALIC)
    val bold = asset("fonts/EBGaramond-Bold.ttf")
        ?: Typeface.create(regular, Typeface.BOLD)
    return ReaderTypefaces(regular, italic, bold)
}

/** A verse-number item's refKey ("Book c:v"), matching ReaderView.RefOf. */
private fun DisplayItem.refOf(book: String, chapter: Int): String =
    verse ?: (verseNumber?.let { "$book $chapter:${it.toInt()}" } ?: "")

/**
 * A single reading pane over [engine]'s layout of [book] [chapter].
 *
 * @param onWordTap fired with the hit-tested word (opens the study pane).
 * @param onVerseLongPress fired with a verse refKey (the desktop context menu).
 * @param searchHits verses to band (search results), painted like the desktop.
 */
@Composable
fun ReaderPane(
    engine: StudyEngine,
    book: String,
    chapter: Int,
    palette: ReaderPalette,
    modifier: Modifier = Modifier,
    fontSizeSp: Float = 18f,
    versePerLine: Boolean = false,
    searchHits: Set<String> = emptySet(),
    onWordTap: (Hit) -> Unit = {},
    onVerseLongPress: (String) -> Unit = {},
    // Bump to force a highlight re-fetch after an add/trim/remove that didn't
    // change book/chapter (the verse-action sheet edits highlights in place).
    highlightEpoch: Int = 0,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    val fontPx = with(density) { fontSizeSp.sp.toPx() }

    val typefaces = remember { loadTypefaces(context) }
    val paints = remember(fontPx) {
        fun p(tf: Typeface) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = tf; textSize = fontPx
        }
        Triple(p(typefaces.regular), p(typefaces.italic), p(typefaces.bold))
    }
    val (regular, italic, bold) = paints

    // Font metrics → the layout's vertical rhythm (mirrors EnsureFormats).
    val fm = remember(fontPx) { regular.fontMetrics }
    val textH = fm.descent - fm.ascent
    val lineH = textH * 1.35f
    // A single space's advance, measured as the desktop does ("n n" − "nn").
    val space = max(1f, regular.measureText("n n") - regular.measureText("nn"))

    var dl by remember { mutableStateOf<DisplayList?>(null) }
    var chapterHandle by remember { mutableStateOf<Chapter?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var highlights by remember { mutableStateOf<Map<String, Color>>(emptyMap()) }
    var runs by remember { mutableStateOf<List<HighlightRunUi>>(emptyList()) }
    var pin by remember { mutableStateOf<PinSpan?>(null) }

    var scrollY by remember { mutableStateOf(0f) }
    var viewportH by remember { mutableStateOf(0f) }

    // Free the native display list when this pane leaves the tree.
    DisposableEffect(Unit) { onDispose { chapterHandle?.close() } }

    BoxWithConstraints(modifier.fillMaxSize()) {
        val widthPx = with(density) { maxWidth.toPx() }
        val column = min(widthPx - 2 * MARGIN, MAX_COLUMN)
        val originX = (widthPx - column) / 2f

        // (Re)lay out the chapter whenever an input that affects it changes.
        LaunchedEffect(book, chapter, widthPx, fontPx, versePerLine) {
            if (widthPx < 60f) return@LaunchedEffect
            val cfg = PureLayoutConfig.ByValue().apply {
                width = column
                lineHeight = lineH
                spaceWidth = space
                verseNumGap = space * 1.4f
                paraIndent = lineH * 0.9f
                paraSpacing = lineH * 0.45f
                verseBreak = if (versePerLine) 1 else 0
            }
            // A dedicated measure Paint, so the background layout never touches the
            // draw paints (Paint is not thread-safe) mutated on the main thread.
            val measurePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                typeface = typefaces.regular; textSize = fontPx
            }
            val result = withContext(Dispatchers.Default) {
                runCatching {
                    // Serialise engine access: two panes may lay out concurrently.
                    synchronized(engine) {
                        val chap = engine.LayoutChapter(book, chapter, cfg) { t ->
                            measurePaint.measureText(t)
                        }
                        chap to parseWire<DisplayList>(chap.Json())
                    }
                }
            }
            chapterHandle?.close()
            result.onSuccess { (chap, parsed) ->
                chapterHandle = chap; dl = parsed; problem = null; scrollY = 0f; pin = null
            }.onFailure {
                chapterHandle = null; dl = null; problem = it.message ?: "layout failed"
            }
        }

        // Fetch this chapter's highlight washes (whole-verse members + word runs).
        LaunchedEffect(book, chapter, highlightEpoch) {
            val hj = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.ChapterHighlightsJson(book, chapter) } }
                    .getOrNull()
            }
            if (hj != null) {
                val ch = runCatching { parseWire<ChapterHighlights>(hj) }.getOrNull()
                highlights = ch?.verses?.associate { it.verse to ReaderPalette.hex(it.color) } ?: emptyMap()
                runs = ch?.runs?.map {
                    HighlightRunUi(it.verse, it.lo.toInt(), it.hi.toInt(), ReaderPalette.hex(it.color))
                } ?: emptyList()
            } else {
                highlights = emptyMap(); runs = emptyList()
            }
        }

        val docHeight = (dl?.height ?: 0f) + 2 * MARGIN
        val maxScroll = max(0f, docHeight - viewportH)
        scrollY = scrollY.coerceIn(0f, maxScroll)

        // Live-updated snapshots for the tap gesture (so the gesture detector is
        // keyed on the chapter handle only, not restarted on every scroll frame).
        val scrollYNow = rememberUpdatedState(scrollY)
        val originXNow = rememberUpdatedState(originX)

        val scrollState = rememberScrollableState { delta ->
            val newY = (scrollY - delta).coerceIn(0f, maxScroll)
            val consumed = scrollY - newY
            scrollY = newY
            consumed
        }

        Canvas(
            modifier = Modifier
                .fillMaxSize()
                // Confine all painting to the pane. The washes/text are drawn in a
                // scroll-translated space, so without this an item scrolled just
                // above the pane paints upward over the top bar (which sits earlier
                // in the Column, i.e. underneath in draw order). Belt to the
                // per-item viewport cull below.
                .clipToBounds()
                .onSizeChanged { viewportH = it.height.toFloat() }
                .scrollable(scrollState, Orientation.Vertical)
                .pointerInput(chapterHandle) {
                    detectTapGestures(
                        onTap = { pos ->
                            val chap = chapterHandle ?: return@detectTapGestures
                            val x = pos.x - originXNow.value
                            val y = pos.y - MARGIN + scrollYNow.value
                            val hj = runCatching {
                                synchronized(engine) { chap.HitTestJson(x, y) }
                            }.getOrNull() ?: return@detectTapGestures
                            val hit = runCatching { parseWire<Hit>(hj) }.getOrNull()
                                ?: return@detectTapGestures
                            pin = PinSpan(hit.verse, hit.tokenIndex.toInt(), hit.tokenIndex.toInt())
                            onWordTap(hit)
                        },
                        onLongPress = { pos ->
                            val verse = verseAt(dl, pos, originXNow.value, scrollYNow.value, book, chapter, engine, chapterHandle)
                            if (verse != null) onVerseLongPress(verse)
                        },
                    )
                },
        ) {
            drawRect(palette.paper, size = size)

            val list = dl
            if (list == null) {
                // "loading…" / an error, at the top margin.
                drawContext.canvas.nativeCanvas.drawText(
                    problem ?: "loading…", MARGIN, MARGIN - fm.ascent,
                    Paint(regular).apply { color = palette.inkFaded.toArgbInt() },
                )
                return@Canvas
            }

            val top = scrollY - MARGIN
            val viewH = size.height

            translate(left = originX, top = MARGIN - scrollY) {
                // Same viewport cull the text uses (§5): a wash/band for an item
                // scrolled off-screen must not paint — otherwise it lands over the
                // chrome above the pane on scroll. clipToBounds is the safety net.
                fun onScreen(item: DisplayItem) = item.y + item.h >= top && item.y <= top + viewH

                // 1. Whole-verse highlight washes — underneath everything.
                if (highlights.isNotEmpty()) {
                    list.items.filter { highlights.containsKey(it.refOf(book, chapter)) && onScreen(it) }
                        .groupBy { it.refOf(book, chapter) }
                        .forEach { (rk, items) ->
                            val wash = palette.wash(highlights.getValue(rk))
                            items.groupBy { it.y }.forEach { (y, line) ->
                                drawRect(wash, Offset(-6f, y), Size(column + 12f, line.first().h))
                            }
                        }
                }

                // 2. Word-precise cross-verse runs — per-word rects.
                for (run in runs) {
                    val wash = palette.wash(run.color)
                    list.items.filter {
                        it.verse == run.verse && onScreen(it) &&
                            it.tokenIndex?.toInt()?.let { t -> t in run.lo..run.hi } == true
                    }.forEach { drawRect(wash, Offset(it.x - 1.5f, it.y), Size(it.w + 3f, it.h)) }
                }

                // 3. Search hits — a soft band per line.
                if (searchHits.isNotEmpty()) {
                    list.items.filter { it.refOf(book, chapter) in searchHits && onScreen(it) }
                        .groupBy { it.y }
                        .forEach { (y, line) ->
                            drawRect(palette.band, Offset(-6f, y), Size(column + 12f, line.first().h))
                        }
                }

                // 4. Pinned span — a blue band per word rect.
                pin?.let { p ->
                    list.items.filter {
                        it.verse == p.verse && onScreen(it) &&
                            it.tokenIndex?.toInt()?.let { t -> t in p.lo..p.hi } == true
                    }.forEach { drawRect(palette.pinBand, Offset(it.x - 1.5f, it.y), Size(it.w + 3f, it.h)) }
                }

                // 5. The text itself.
                val canvas = drawContext.canvas.nativeCanvas
                for (it in list.items) {
                    if (it.y + it.h < top || it.y > top + viewH) continue
                    val dyTop = it.y + (it.h - textH) * 0.5f
                    val baseline = dyTop - fm.ascent

                    if (it.kind == "verseNumber") {
                        bold.color = palette.gold.toArgbInt()
                        canvas.drawText(it.text, it.x, baseline, bold)
                        continue
                    }
                    val flags = it.flags.toInt()
                    val added = flags and PureFlags.ADDED != 0
                    val divine = flags and PureFlags.DIVINE != 0
                    val title = flags and PureFlags.TITLE != 0
                    val color = when {
                        added -> palette.inkFaded
                        divine -> palette.divine
                        title -> palette.titleInk
                        else -> palette.ink
                    }
                    val paint = if (added) italic else regular
                    paint.color = color.toArgbInt()
                    canvas.drawText(it.text, it.x, baseline, paint)
                    // A faint gold underline marks a Strong's-tagged word.
                    if (it.strongs.isNotEmpty()) {
                        drawRect(palette.goldFaint, Offset(it.x, it.y + it.h - 3f), Size(it.w, 1f))
                    }
                }
            }
        }
    }
}

/** The verse under a tap: the hit word's verse, else the nearest verse-number by
 *  y (mirrors ReaderView.VerseAt). Used for the long-press context target. */
private fun verseAt(
    dl: DisplayList?,
    pos: Offset,
    originX: Float,
    scrollY: Float,
    book: String,
    chapter: Int,
    engine: StudyEngine,
    chap: Chapter?,
): String? {
    if (dl == null) return null
    if (chap != null) {
        val hj = runCatching {
            synchronized(engine) { chap.HitTestJson(pos.x - originX, pos.y - MARGIN + scrollY) }
        }.getOrNull()
        if (hj != null) {
            runCatching { parseWire<Hit>(hj) }.getOrNull()?.let { return it.verse }
        }
    }
    val y = pos.y - MARGIN + scrollY
    var best: DisplayItem? = null
    var bestD = Float.MAX_VALUE
    for (it in dl.items.filter { it.kind == "verseNumber" }) {
        val d = abs(it.y + it.h * 0.5f - y)
        if (d < bestD) { bestD = d; best = it }
    }
    return best?.refOf(book, chapter)
}

/** Compose [Color] → packed ARGB int for android.graphics.Paint. */
private fun Color.toArgbInt(): Int {
    val a = (alpha * 255f + 0.5f).toInt()
    val r = (red * 255f + 0.5f).toInt()
    val g = (green * 255f + 0.5f).toInt()
    val b = (blue * 255f + 0.5f).toInt()
    return (a shl 24) or (r shl 16) or (g shl 8) or b
}
