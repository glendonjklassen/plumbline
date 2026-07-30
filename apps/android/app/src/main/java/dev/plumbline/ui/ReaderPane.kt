// One reading column: a Compose Canvas that lets the Rust core do the layout.
// This is the Android mirror of apps/windows/PureStudyWin/ReaderView.cs — the
// core's line-breaker measures text through our Paint-backed callback and hands
// back a display list + per-word hit regions; this composable paints the items
// and forwards tap coordinates back for hit-testing. Same thin-shell contract
// the GTK (Pango) and WinUI (DirectWrite) shells follow. Constants mirror the
// feature manifest (MARGIN 28, MAX_COLUMN 720, line_height = textH·1.35).
//
// Dragging a chapter up and down is the thing a reader does most, so the scroll
// offset is kept OUT OF COMPOSITION: it is written from gestures and read from
// the draw phase, which means a frame repaints without recomposing. Everything a
// frame needs that doesn't move — the bands, the note dots, the inks as packed
// ARGB, the per-verse extents the scroll reports binary-search — is worked out
// once per layout. Keep it that way: one composition-phase read of `scroll` puts
// a full recomposition back on every frame.
//
// A laid-out chapter is a native handle, and the layout effect is cancelled every
// time a reader turns a page before the last one finished. Every allocation goes
// through publishOrClose, which either hands the handle to the pane or frees it —
// there is no third path out of that function.
//
// Author D (Compose UI).

package dev.plumbline.ui

import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.gestures.rememberScrollableState
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.Chapter
import dev.plumbline.DisplayItem
import dev.plumbline.DisplayList
import dev.plumbline.Hit
import dev.plumbline.PlumblineFlags
import dev.plumbline.StudyEngine
import dev.plumbline.UserNotes
import dev.plumbline.core.PlumblineLayoutConfig
import dev.plumbline.parseWire
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.Closeable
import kotlin.coroutines.CoroutineContext
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

// The manifest constants are LOGICAL units (the desktop shells run ~1 device px
// per logical px). On a phone (density ~2.6) treating them as raw device px
// capped the text column at ~277dp and left wide empty gutters — the
// "lots of whitespace" feedback (2026-07-24) — so both scale by density here.
private const val MARGIN_DP = 28f      // GTK MARGIN — all sides
private const val MAX_COLUMN_DP = 720f // GTK MAX_COLUMN

/** A pinned word span (single tap sets a one-word anchor; parity-lite with the
 *  desktop pin — full widen-on-second-tap is a TODO). */
private data class PinSpan(val verse: String, val lo: Int, val hi: Int)

private data class ReaderTypefaces(val regular: Typeface, val italic: Typeface, val bold: Typeface)

private fun loadTypefaces(context: Context): ReaderTypefaces {
    fun asset(path: String): Typeface? =
        runCatching { Typeface.createFromAsset(context.assets, path) }.getOrNull()
    // Bundled EB Garamond (a variable font, weight 400–700 — the same files
    // the web ships); fall back to the platform serif.
    val regular = asset("fonts/EBGaramond-Regular.ttf") ?: Typeface.SERIF
    val italic = asset("fonts/EBGaramond-Italic.ttf")
        ?: Typeface.create(Typeface.SERIF, Typeface.ITALIC)
    val bold = runCatching {
        Typeface.Builder(context.assets, "fonts/EBGaramond-Regular.ttf")
            .setFontVariationSettings("'wght' 700").build()
    }.getOrNull() ?: Typeface.create(regular, Typeface.BOLD)
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
    // Horizontal margin (dp) either side of the text column, and the line-height
    // multiple — both reader prefs (config), defaulting to the feature-manifest
    // MARGIN 28 / line_height 1.35.
    sideMargin: Float = 28f,
    lineSpacing: Float = 1.35f,
    versePerLine: Boolean = false,
    searchHits: Set<String> = emptySet(),
    onWordTap: (Hit) -> Unit = {},
    onVerseLongPress: (String) -> Unit = {},
    // A horizontal fling steps the chapter: +1 (swipe left → next), -1 (swipe
    // right → previous).
    onSwipeChapter: (Int) -> Unit = {},
    // Bump to force a re-fetch of the per-verse note marks after a note edit
    // that didn't change book/chapter.
    noteEpoch: Int = 0,
    /** The plain-English overlay (the AKJV delta). A layout INPUT: it changes
     *  the words on the page, so the chapter re-lays when it flips. */
    akjvOverlay: Boolean = false,
    // Scroll this verse number into view once the layout lands; bump the epoch
    // to re-apply for the same verse (the book navigator's verse tap).
    targetVerse: Int? = null,
    targetEpoch: Int = 0,
    // Bump to clear the tapped-word pin (the study sheet was dismissed — the
    // word should un-highlight with it).
    clearPinEpoch: Int = 0,
    // Reports the first visible verse as the reader scrolls — persisted so a
    // session reopens mid-chapter where it left off.
    onFirstVisibleVerse: (Int) -> Unit = {},
    /** Reports the DEEPEST verse the reader has scrolled to in this chapter — the
     *  reading map's high-water mark (core::reading). Monotonic within a chapter:
     *  it only ever rises, because scrolling back up does not un-read anything.
     *  Distinct from [onFirstVisibleVerse], which tracks the top edge and moves
     *  both ways. */
    onVerseReached: (Int) -> Unit = {},
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    val fontPx = with(density) { fontSizeSp.sp.toPx() }
    val marginPx = with(density) { MARGIN_DP.dp.toPx() }

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
    val lineH = textH * lineSpacing
    // A single space's advance, measured as the desktop does ("n n" − "nn").
    val space = max(1f, regular.measureText("n n") - regular.measureText("nn"))

    var dl by remember { mutableStateOf<DisplayList?>(null) }
    var chapterHandle by remember { mutableStateOf<Chapter?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var pin by remember { mutableStateOf<PinSpan?>(null) }
    var noteVerses by remember { mutableStateOf<Set<Int>>(emptySet()) }

    // The study sheet was dismissed — drop the tapped word's highlight too.
    LaunchedEffect(clearPinEpoch) {
        if (clearPinEpoch > 0) pin = null
    }

    // The scroll offset is a state OBJECT read through `.floatValue`, never a
    // delegated `var`: nothing in COMPOSITION may touch it. The pane's whole
    // content is one Canvas, so a scroll frame needs only the draw phase — but
    // this was read and (in the clamp) written during composition, which
    // recomposed the pane on every pixel of every drag. Every read below is in
    // the draw lambda, a gesture callback, or a snapshotFlow: all after
    // composition. The clamp moved to the three places that write it.
    val scroll = remember { mutableFloatStateOf(0f) }
    // Set from the layout phase (onSizeChanged) and read in composition for the
    // scroll extent — it changes on a rotation or a fold, not on a frame.
    var viewportH by remember { mutableFloatStateOf(0f) }
    var swipeDx by remember { mutableFloatStateOf(0f) }   // accumulated horizontal drag

    // Free the native display list when this pane leaves the tree.
    DisposableEffect(Unit) { onDispose { chapterHandle?.close() } }

    BoxWithConstraints(modifier.fillMaxSize()) {
        val widthPx = with(density) { maxWidth.toPx() }
        val sidePx = with(density) { sideMargin.dp.toPx() }
        val column = min(widthPx - 2 * sidePx, with(density) { MAX_COLUMN_DP.dp.toPx() })
        val originX = (widthPx - column) / 2f

        // (Re)lay out the chapter whenever an input that affects it changes
        // (margin/spacing change the column width + rhythm, so re-lay out too).
        LaunchedEffect(book, chapter, widthPx, fontPx, versePerLine, sideMargin, lineSpacing, akjvOverlay) {
            if (widthPx < 60f) return@LaunchedEffect
            val cfg = PlumblineLayoutConfig.ByValue().apply {
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
            // The layout allocates a NATIVE display list. publishOrClose owns it
            // from the allocation until either `publish` takes it or it is freed —
            // this effect is cancelled on every fast chapter turn, and the old
            // shape (lay out, then assign) dropped the handle on the floor every
            // time that happened.
            publishOrClose(
                // Serialise engine access: two panes may lay out concurrently.
                // (This is the `synchronized(engine)` monitor, held inside.)
                engineLock = engine,
                acquire = {
                    // Set inside the lock and immediately before the layout:
                    // the overlay is engine state, so a pane laying out
                    // concurrently must not see a half-applied toggle.
                    engine.SetAkjvOverlay(akjvOverlay)
                    engine.LayoutChapter(book, chapter, cfg) { t ->
                        measurePaint.measureText(t)
                    }
                },
                derive = { chap -> parseWire<DisplayList>(chap.Json()) },
                publish = { chap, parsed ->
                    chapterHandle?.close()
                    chapterHandle = chap; dl = parsed; problem = null; scroll.floatValue = 0f; pin = null
                },
                onProblem = {
                    chapterHandle?.close()
                    chapterHandle = null; dl = null; problem = it.message ?: "layout failed"
                },
            )
        }

        // Which verses carry a personal note — the reader's own words get a
        // visible mark (a gutter dot by the verse number). Re-fetched with the
        // note epoch (a note edit bumps it).
        LaunchedEffect(book, chapter, noteEpoch) {
            val prefix = "$book $chapter:"
            noteVerses = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.UserNotesJson() } }.getOrNull()
                    ?.let { runCatching { parseWire<UserNotes>(it).notes }.getOrNull() }
                    ?.filter { it.verse.startsWith(prefix) }
                    ?.mapNotNull { it.verse.substringAfterLast(':').toIntOrNull() }
                    ?.toSet() ?: emptySet()
            }
        }

        // Where every verse sits, worked out ONCE per layout. The scroll path asks
        // two questions of it on every frame — which verse is at the top edge, and
        // how deep has the reader got — and both are a binary search over this
        // table instead of a scan over every word in the chapter (verseExtents /
        // verseAtTop / deepestVerseEntered below).
        val extents = remember(dl) { verseExtents(dl?.items ?: emptyList()) }

        // Report the first visible verse (top edge) whenever scroll settles on a
        // new one — the config persists it for cross-session scroll restore. ONE
        // collector per layout: this was keyed on the offset itself, so a drag
        // cancelled and relaunched a coroutine every frame.
        var lastReported by remember { mutableIntStateOf(-1) }
        val reportFirst = rememberUpdatedState(onFirstVisibleVerse)
        LaunchedEffect(extents) {
            snapshotFlow { verseAtTop(extents, scroll.floatValue) }.collect { first ->
                if (first > 0 && first != lastReported) {
                    lastReported = first
                    reportFirst.value(first)
                }
            }
        }

        // The reading map's high-water mark: the deepest verse the reader has
        // scrolled to. A verse counts once the line it STARTS on has cleared the
        // fold — which is what the old max-over-every-word-above-the-fold worked
        // out, spelled plainly. Reset per chapter, and only ever rising within
        // one: the core takes the max anyway, but reporting a fall would put
        // pointless writes on the scroll path. The fold is generous by a margin
        // (the offset is measured from the document top, margin included), so the
        // document bottom coming into view reaches the last verse and a short
        // final verse can't strand a chapter at 99%.
        var deepest by remember(book, chapter) { mutableIntStateOf(0) }
        val reportReached = rememberUpdatedState(onVerseReached)
        LaunchedEffect(extents) {
            snapshotFlow {
                if (viewportH <= 0f) 0 else deepestVerseEntered(extents, scroll.floatValue + viewportH)
            }.collect { reached ->
                if (reached > deepest) {
                    deepest = reached
                    reportReached.value(reached)
                }
            }
        }

        val docHeight = (dl?.height ?: 0f) + 2 * marginPx
        val maxScroll = max(0f, docHeight - viewportH)

        // Scroll the navigator's target verse into view once the layout lands.
        // Epoch-guarded so a re-layout (font/margin change) doesn't re-jump.
        var appliedTarget by remember { mutableStateOf(-1) }
        LaunchedEffect(dl, targetEpoch) {
            val tv = targetVerse ?: return@LaunchedEffect
            val list = dl ?: return@LaunchedEffect
            if (appliedTarget == targetEpoch) return@LaunchedEffect
            val item = list.items.firstOrNull { it.kind == "verseNumber" && it.verseNumber?.toInt() == tv }
            if (item != null) {
                scroll.floatValue = max(0f, item.y).coerceAtMost(maxScroll)
                appliedTarget = targetEpoch
            }
        }

        // Live-updated snapshot for the tap gesture (so the gesture detector is
        // keyed on the chapter handle only, not restarted on every scroll frame).
        val originXNow = rememberUpdatedState(originX)

        val scrollState = rememberScrollableState { delta ->
            val y = scroll.floatValue
            val newY = (y - delta).coerceIn(0f, maxScroll)
            scroll.floatValue = newY
            y - newY
        }

        // Horizontal fling → chapter step. Distinct from the vertical scroll
        // above; Compose routes each drag to the detector matching its axis.
        val swipeState = rememberDraggableState { delta -> swipeDx += delta }

        // Everything the paint pass can know before the reader moves: the inks as
        // packed ARGB, and the bands/dots as positioned rectangles. All of it was
        // a filter + groupBy + a colour conversion per word PER FRAME, rebuilding
        // values that only change with the layout, the palette or a tap. A scroll
        // frame now culls and paints, and allocates nothing.
        val inks = remember(palette) { ReaderInks(palette) }
        val problemPaint = remember(regular, inks) { Paint(regular).apply { color = inks.added } }
        val bandLines = remember(dl, searchHits, book, chapter, column) {
            val out = ArrayList<LayoutRect>()
            if (searchHits.isNotEmpty()) {
                // One wash per LINE, not per hit word. Items arrive top-to-bottom,
                // so a line's hits are contiguous and the previous y is enough to
                // dedupe — painting a translucent band twice would darken it.
                var lastY = Float.NaN
                for (item in dl?.items ?: emptyList()) {
                    if (item.refOf(book, chapter) !in searchHits) continue
                    if (item.y == lastY) continue
                    lastY = item.y
                    out.add(LayoutRect(-6f, item.y, column + 12f, item.h))
                }
            }
            out
        }
        val pinRects = remember(dl, pin) {
            val out = ArrayList<LayoutRect>()
            val p = pin
            if (p != null) {
                for (item in dl?.items ?: emptyList()) {
                    if (item.verse != p.verse) continue
                    val t = item.tokenIndex ?: continue
                    if (t < p.lo || t > p.hi) continue
                    out.add(LayoutRect(item.x - 1.5f, item.y, item.w + 3f, item.h))
                }
            }
            out
        }
        val noteDots = remember(dl, noteVerses) {
            val out = ArrayList<LayoutDot>()
            if (noteVerses.isNotEmpty()) {
                for (item in dl?.items ?: emptyList()) {
                    if (item.kind != "verseNumber") continue
                    val v = item.verseNumber ?: continue
                    if (v !in noteVerses) continue
                    out.add(LayoutDot(item.x - 18f, item.y + item.h * 0.5f, item.y, item.h))
                }
            }
            out
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
                .onSizeChanged {
                    val h = it.height.toFloat()
                    viewportH = h
                    // A shorter viewport (a rotation, a fold) can leave the offset
                    // past the document's end. Re-clamp HERE — from the layout
                    // phase — rather than in composition, which is what used to
                    // write scroll state on every frame.
                    scroll.floatValue = scroll.floatValue.coerceIn(0f, max(0f, docHeight - h))
                }
                .scrollable(scrollState, Orientation.Vertical)
                .draggable(
                    state = swipeState,
                    orientation = Orientation.Horizontal,
                    onDragStopped = {
                        val threshold = with(density) { 64.dp.toPx() }
                        if (swipeDx <= -threshold) onSwipeChapter(1)       // swipe left → next
                        else if (swipeDx >= threshold) onSwipeChapter(-1)  // swipe right → prev
                        swipeDx = 0f
                    },
                )
                .pointerInput(chapterHandle) {
                    detectTapGestures(
                        onTap = { pos ->
                            val chap = chapterHandle ?: return@detectTapGestures
                            val x = pos.x - originXNow.value
                            val y = pos.y - marginPx + scroll.floatValue
                            val hj = runCatching {
                                synchronized(engine) { chap.HitTestJson(x, y) }
                            }.getOrNull() ?: return@detectTapGestures
                            val hit = runCatching { parseWire<Hit>(hj) }.getOrNull()
                                ?: return@detectTapGestures
                            pin = PinSpan(hit.verse, hit.tokenIndex.toInt(), hit.tokenIndex.toInt())
                            onWordTap(hit)
                        },
                        onLongPress = { pos ->
                            val verse = verseAt(dl, pos, originXNow.value, scroll.floatValue, marginPx, book, chapter, engine, chapterHandle)
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
                    problem ?: "loading…", marginPx, marginPx - fm.ascent, problemPaint,
                )
                return@Canvas
            }

            // The one place the scroll offset is read on a frame — a draw-phase
            // read, so a drag invalidates the draw and nothing else. Clamped on
            // the way in as well as at every write: the extent depends on the
            // viewport, and a target-verse jump can land before the pane has
            // been measured.
            val scrollY = scroll.floatValue.coerceIn(0f, maxScroll)
            val top = scrollY - marginPx
            val viewH = size.height

            translate(left = originX, top = marginPx - scrollY) {
                // Same viewport cull the text uses (§5): a wash/band for an item
                // scrolled off-screen must not paint — otherwise it lands over the
                // chrome above the pane on scroll. clipToBounds is the safety net.
                fun onScreen(y: Float, h: Float) = y + h >= top && y <= top + viewH

                // 1. Search hits — a soft band per line.
                for (r in bandLines) {
                    if (!onScreen(r.y, r.h)) continue
                    drawRect(palette.band, Offset(r.x, r.y), Size(r.w, r.h))
                }

                // 4. Pinned span — a blue band per word rect.
                for (r in pinRects) {
                    if (!onScreen(r.y, r.h)) continue
                    drawRect(palette.pinBand, Offset(r.x, r.y), Size(r.w, r.h))
                }

                // 4b. Note marks: a small gutter dot beside the verse number of
                // any verse carrying the reader's own note (their words are in
                // the study pane; this says "you wrote here" at a glance).
                for (d in noteDots) {
                    if (!onScreen(d.y, d.h)) continue
                    drawCircle(color = palette.gutterDot, radius = 5f, center = Offset(d.cx, d.cy))
                }

                // 5. The text itself.
                val canvas = drawContext.canvas.nativeCanvas
                for (it in list.items) {
                    if (it.y + it.h < top || it.y > top + viewH) continue
                    val dyTop = it.y + (it.h - textH) * 0.5f
                    val baseline = dyTop - fm.ascent

                    if (it.kind == "verseNumber") {
                        bold.color = inks.gold
                        canvas.drawText(it.text, it.x, baseline, bold)
                        continue
                    }
                    val flags = it.flags
                    val added = flags and PlumblineFlags.ADDED != 0
                    val divine = flags and PlumblineFlags.DIVINE != 0
                    val title = flags and PlumblineFlags.TITLE != 0
                    val paint = if (added) italic else regular
                    paint.color = when {
                        added -> inks.added
                        divine -> inks.divine
                        title -> inks.title
                        else -> inks.ink
                    }
                    canvas.drawText(it.text, it.x, baseline, paint)
                    // NO mark for a Strong's-tagged word. There used to be a
                    // faint gold rule under every one, and since most words
                    // carry a Strong's number it amounted to underlining the
                    // Bible: noise that told the reader nothing they act on.
                    // Whether a word answers when tapped is learned once, not
                    // repeated by the page (2026-07-28). Web twin: paint.ts.
                    //
                    // The AKJV overlay's mark: DOTTED, at the natural underline
                    // depth — it sat lower while it had to clear the Strong's
                    // rule, and moved up when that went. Not bold or grey —
                    // italic already means "supplied by the translator", and at
                    // 6.9% of words a heavy mark reads as a ransom note.
                    if (flags and PlumblineFlags.RERENDERED != 0) {
                        var dx = it.x
                        val dy = it.y + it.h - 3f
                        while (dx < it.x + it.w) {
                            drawRect(palette.gold, Offset(dx, dy), Size(1.5f, 1f))
                            dx += 4f
                        }
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
    marginPx: Float,
    book: String,
    chapter: Int,
    engine: StudyEngine,
    chap: Chapter?,
): String? {
    if (dl == null) return null
    if (chap != null) {
        val hj = runCatching {
            synchronized(engine) { chap.HitTestJson(pos.x - originX, pos.y - marginPx + scrollY) }
        }.getOrNull()
        if (hj != null) {
            runCatching { parseWire<Hit>(hj) }.getOrNull()?.let { return it.verse }
        }
    }
    val y = pos.y - marginPx + scrollY
    var best: DisplayItem? = null
    var bestD = Float.MAX_VALUE
    for (it in dl.items.filter { it.kind == "verseNumber" }) {
        val d = abs(it.y + it.h * 0.5f - y)
        if (d < bestD) { bestD = d; best = it }
    }
    return best?.refOf(book, chapter)
}

// ── the native handle: published or closed, never dropped ───────────────────

/**
 * Produce a value that comes with a NATIVE handle, and guarantee the handle is
 * either published or freed.
 *
 * A laid-out chapter is native memory that only `plumbline_layout_free` releases
 * (via [Closeable.close]), so a handle that reaches nobody is leaked for the life
 * of the process. Three things can happen between the allocation and the
 * assignment that keeps it, and the old straight-line shape — lay out, then
 * assign — handled only the first:
 *
 *  1. it lands: [publish] takes ownership, and the pane frees it when it is
 *     replaced or the pane leaves the tree;
 *  2. [derive] throws with the handle already allocated (the JSON parse) — the
 *     handle was inside a failed `Result` nothing held;
 *  3. the coroutine is CANCELLED while [acquire] runs. This is the common one: a
 *     reader turning pages fast re-keys the layout effect, which cancels the one
 *     in flight, and leaving the reader cancels it too. The native call is not
 *     interruptible, so the layout finishes and cancellation is observed on the
 *     way out of [withContext] — the handle exists and the assignment that would
 *     have owned it never runs. One leaked chapter per cancelled turn.
 *
 * So [acquire] is separated from everything that can throw, and the `finally`
 * closes anything not handed over — including on cancellation, where the
 * exception is rethrown (never reported as a layout problem: the pane is either
 * going away or already laying out the next chapter, and swallowing it would
 * break structured concurrency).
 *
 * @param engineLock held across [acquire] and [derive]: engine state is set
 *   immediately before the layout reads it, so the two cannot be interleaved.
 */
internal suspend fun <H : Closeable, V> publishOrClose(
    engineLock: Any,
    acquire: () -> H,
    derive: (H) -> V,
    publish: (H, V) -> Unit,
    onProblem: (Throwable) -> Unit,
    context: CoroutineContext = Dispatchers.Default,
) {
    var owned: H? = null
    try {
        val (handle, value) = withContext(context) {
            synchronized(engineLock) {
                val h = acquire()
                owned = h          // owned from HERE — derive may throw
                h to derive(h)
            }
        }
        publish(handle, value)
        owned = null               // ownership transferred
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (t: Throwable) {
        onProblem(t)
    } finally {
        owned?.close()
    }
}

// ── the scroll path, precomputed ────────────────────────────────────────────
// Everything below is worked out once per layout (or per palette) and then only
// read: the reader drags a chapter more than they do anything else, and a frame
// that allocates or rescans the display list is a frame that stutters.

/** A rectangle placed once per layout: the wash under a search-hit line and the
 *  band under a pinned word are both this. [y] and [h] are the source item's
 *  own, so the draw pass culls with exactly the test the text uses. */
private class LayoutRect(val x: Float, val y: Float, val w: Float, val h: Float)

/** A note dot: its centre, plus the verse number's extent it is culled by. */
private class LayoutDot(val cx: Float, val cy: Float, val y: Float, val h: Float)

/** The reader's inks as packed ARGB — what android.graphics.Paint takes. Built
 *  once per palette: converting a Compose [Color] per word per frame was ~900
 *  conversions a frame for five values that change only with the theme. */
private class ReaderInks(palette: ReaderPalette) {
    val ink = palette.ink.toArgbInt()
    val added = palette.inkFaded.toArgbInt()
    val divine = palette.divine.toArgbInt()
    val title = palette.titleInk.toArgbInt()
    val gold = palette.gold.toArgbInt()
}

/** One verse's vertical landmarks in a laid-out chapter, in the display list's
 *  own coordinate space.
 *
 *  @property numberBottom the bottom edge of the line the verse NUMBER sits on —
 *    the verse is the top-edge verse until this scrolls past the top.
 *  @property entryBottom the bottom edge of the line the verse's FIRST word sits
 *    on — the verse has come into view once this is above the fold.
 */
internal data class VerseExtent(val verse: Int, val numberBottom: Float, val entryBottom: Float)

/**
 * The per-verse extents of one laid-out chapter, in verse order.
 *
 * Built once per layout for the two questions the scroll path asks on every
 * frame. Both bounds rise with the verse number, because the core emits the
 * display list strictly top-to-bottom, a verse's number ahead of its words, and
 * every item one line tall — which is what makes the binary searches below
 * legal. A linear scan of this table must always give the same answer;
 * VerseExtentsTest holds them to it.
 */
internal fun verseExtents(items: List<DisplayItem>): List<VerseExtent> {
    val out = ArrayList<VerseExtent>()
    var awaitingFirstWord = false
    for (item in items) {
        if (item.kind == "verseNumber") {
            val v = item.verseNumber ?: continue
            val bottom = item.y + item.h
            // entryBottom starts on the number's line and moves down to the first
            // word's, which is a different line only when the word wrapped. A
            // verse that renders no words at all (the overlay can blank a token)
            // keeps the number's line, so every verse has an extent and the table
            // stays sorted.
            out.add(VerseExtent(v, bottom, bottom))
            awaitingFirstWord = true
        } else if (awaitingFirstWord && item.kind == "word" && out.isNotEmpty()) {
            out[out.size - 1] = out.last().copy(entryBottom = item.y + item.h)
            awaitingFirstWord = false
        }
    }
    return out
}

/** The verse at the pane's top edge: the first one whose number has not yet
 *  scrolled past [scrollY]. 0 when the offset is below every verse. */
internal fun verseAtTop(extents: List<VerseExtent>, scrollY: Float): Int {
    // Lower bound on `numberBottom > scrollY` — the FIRST index that satisfies
    // it, which is what `firstOrNull { it.y + it.h > scrollY }` used to find.
    var lo = 0
    var hi = extents.size
    while (lo < hi) {
        val mid = (lo + hi) ushr 1
        if (extents[mid].numberBottom > scrollY) hi = mid else lo = mid + 1
    }
    return if (lo < extents.size) extents[lo].verse else 0
}

/** The deepest verse that has come into view above [fold] — the reading map's
 *  high-water candidate. 0 when none has. */
internal fun deepestVerseEntered(extents: List<VerseExtent>, fold: Float): Int {
    // Upper bound on `entryBottom <= fold`: the LAST index that satisfies it,
    // and since verses only ever run down the page that is the same verse the
    // old filter-every-word-then-max arrived at.
    var lo = 0
    var hi = extents.size
    while (lo < hi) {
        val mid = (lo + hi) ushr 1
        if (extents[mid].entryBottom <= fold) lo = mid + 1 else hi = mid
    }
    return if (lo > 0) extents[lo - 1].verse else 0
}

/** Compose [Color] → packed ARGB int for android.graphics.Paint. */
private fun Color.toArgbInt(): Int {
    val a = (alpha * 255f + 0.5f).toInt()
    val r = (red * 255f + 0.5f).toInt()
    val g = (green * 255f + 0.5f).toInt()
    val b = (blue * 255f + 0.5f).toInt()
    return (a shl 24) or (r shl 16) or (g shl 8) or b
}
