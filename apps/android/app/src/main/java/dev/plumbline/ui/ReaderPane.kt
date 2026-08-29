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
// The text itself is worked out once per layout too: it is RECORDED into an
// android.graphics.Picture and replayed with a single drawPicture. A screenful of
// this page is 400–900 words, each of them its own display item, and each one was
// a Canvas.drawText from Kotlin — a JNI crossing, a String marshalled across it
// and a shaping lookup — on every frame of every drag. What may be baked into
// that recording and what may not is the whole risk of it: ChapterPaintKey
// enumerates every input, and the washes, the note dots and the pinned span stay
// LIVE beneath the replay so their z-order holds and a tap costs no re-record.
//
// The chapters just behind the reader stay laid out in a small LRU
// (CHAPTER_CACHE), keyed by everything the line-breaker saw, so a back-swipe
// repaints instead of re-laying out. That cache OWNS every native handle in it.
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
import android.graphics.Picture
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
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

// The manifest constants are LOGICAL units (the desktop shells run ~1 device px
// per logical px). On a phone (density ~2.6) treating them as raw device px
// capped the text column at ~277dp and left wide empty gutters, so both scale
// by density here.
private const val MARGIN_DP = 28f      // GTK MARGIN — all sides
private const val MAX_COLUMN_DP = 720f // GTK MAX_COLUMN

/** How many laid-out chapters to keep. Two is what a back-swipe needs (the page
 *  on screen and the one behind it); three covers swiping back and forth over a
 *  boundary without ever paying for the layout twice. It stops there because an
 *  entry is not free: ~327 bytes per display item on the JVM (measured) plus
 *  ~185 native, which is ~350 KB for an average 691-item chapter and ~1.3 MB for
 *  Psalm 119's 2,643 — a phone would not thank us for ten of those.
 *
 *  It may never drop below 2. The chapter on screen is the most recently used
 *  entry and eviction takes the least recently used, so 2 is the smallest bound
 *  at which the pane cannot free the page it is painting (see [ChapterCache]). */
internal const val CHAPTER_CACHE = 3

/** A pinned word span (single tap sets a one-word anchor; parity-lite with the
 *  desktop pin — full widen-on-second-tap is a TODO). */
private data class PinSpan(val verse: String, val lo: Int, val hi: Int)

internal data class ReaderTypefaces(val regular: Typeface, val italic: Typeface, val bold: Typeface)

/** The three canvas faces, parsed at most once per PROCESS.
 *
 *  This was a `remember` inside the pane, so it was once per pane INSTANCE:
 *  three panes meant three parses of the same 1.6 MB of variable TTF, and
 *  every pane add, fold change and Activity recreate paid again — on the main
 *  thread, in front of first paint. The platform's own Typeface cache is nine
 *  entries shared process-wide with everything else that names a font, so it
 *  was never a promise worth leaning on.
 *
 *  Holding these forever is the point and costs nothing extra: they are the
 *  faces the reader is always looking at, and a [Typeface] holds no Context. */
private val TYPEFACES = Keyed<ReaderTypefaces>()

internal fun readerTypefaces(context: Context, token: String? = null): ReaderTypefaces {
    val spec = fontFor(token)
    return TYPEFACES.get(spec.token) { loadTypefaces(context, spec) }
}

private fun loadTypefaces(context: Context, spec: FontSpec): ReaderTypefaces {
    // Weight pinned EXPLICITLY, not taken from the file's default instance:
    // Fira Code's `wght` axis defaults to 300, so its regular would paint as
    // Light. Same reason as buildSerifFamily in Typography.kt.
    fun at(path: String, weight: Int): Typeface? = runCatching {
        Typeface.Builder(context.assets, path).setFontVariationSettings("'wght' $weight").build()
    }.getOrNull() ?: runCatching { Typeface.createFromAsset(context.assets, path) }.getOrNull()

    val regular = at(spec.regular, 400) ?: Typeface.SERIF
    // A family with no italic file paints added words UPRIGHT — the palette's
    // `added` tone is what marks them (see FontSpec). Synthesising a slant here
    // is exactly what we are refusing to do.
    val italic = spec.italic?.let { at(it, 400) } ?: regular
    // A static family (Atkinson) carries its bold as its own file; a variable
    // one is the same file driven to 700 on the wght axis.
    val bold = spec.bold?.let { at(it, 700) }
        ?: at(spec.regular, 700)
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
    // Horizontal margin (dp) either side of the text column, and the line-height
    // multiple — both reader prefs (config), defaulting to the feature-manifest
    // MARGIN 28 / line_height 1.35.
    sideMargin: Float = 28f,
    lineSpacing: Float = 1.35f,
    versePerLine: Boolean = false,
    // Page-turn mode (config): the side gutters become tap zones that page the
    // text — right ahead, left back — so a page-turner remote can drive the
    // page. Guarantees at least a 44dp gutter whatever the margin slider says.
    pageTurn: Boolean = false,
    // The two reader-typography switches (config, both ON by default).
    // `verseNumbers` is a LAYOUT input — it moves every word on every line, so
    // it belongs in ChapterKey; `addedItalics` is paint-only and deliberately
    // does NOT, or a repaint-only change would throw away a good layout.
    verseNumbers: Boolean = true,
    addedItalics: Boolean = true,
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
    val textFont = LocalTextFont.current
    // The reader's size under the face's optical scale (FontSpec.scale): faces
    // differ in x-height, and switching faces must not change the apparent
    // size. Applied to the px the paints and the layout BOTH use — never to
    // the stored setting, which keeps the number the reader chose.
    val fontPx = with(density) { fontSizeSp.sp.toPx() } * fontFor(textFont).scale
    val marginPx = with(density) { MARGIN_DP.dp.toPx() }

    val typefaces = remember(context, textFont) { readerTypefaces(context, textFont) }
    val paints = remember(fontPx) {
        fun p(tf: Typeface) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = tf; textSize = fontPx
            // LEFT, and pinned rather than assumed. `x` from the display list is
            // always a left edge — the engine mirrors the whole list for a
            // right-to-left text (crates/layout), so direction is settled before
            // any of this — and Align.RIGHT or CENTER would make drawText read
            // the same number as a different edge and paint every word offset
            // from its own hit box. The default is already LEFT; saying so keeps
            // a future themed Paint from quietly changing where words land.
            textAlign = Paint.Align.LEFT
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

    // The chapter on screen, and the small LRU that owns it along with the ones
    // just behind it. `current` is always the cache's most recently used entry —
    // that is the discipline the cache's safety rests on, so publish through
    // `show` and nowhere else.
    val chapters = remember { ChapterCache<ChapterKey, LaidOutChapter>(CHAPTER_CACHE) }
    var current by remember { mutableStateOf<LaidOutChapter?>(null) }
    val dl = current?.list
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

    // Free every native display list when this pane leaves the tree — the cache
    // holds the one on screen too, so this is the only place they are all freed.
    DisposableEffect(Unit) { onDispose { chapters.close() } }

    BoxWithConstraints(modifier.fillMaxSize()) {
        val widthPx = with(density) { maxWidth.toPx() }
        val sidePx = with(density) { (if (pageTurn) max(sideMargin, 44f) else sideMargin).dp.toPx() }
        val column = min(widthPx - 2 * sidePx, with(density) { MAX_COLUMN_DP.dp.toPx() })
        val originX = (widthPx - column) / 2f

        // Everything the core's line-breaker will see. ONE list, used twice — it
        // is what re-triggers the layout and what keys the cache — so the two can
        // never drift apart and hand back a chapter laid out for other inputs.
        val chapterKey = ChapterKey(
            book = book,
            chapter = chapter,
            column = column,
            fontPx = fontPx,
            lineHeight = lineH,
            spaceWidth = space,
            versePerLine = versePerLine,
            verseNumbers = verseNumbers,
            akjvOverlay = akjvOverlay,
        )

        // Publish a laid-out chapter, and make it the cache's most recent entry
        // in the same breath. Resets exactly as a fresh layout always has: to the
        // top, with no pin. The cache is there to save the layout, not to
        // remember where the reader was.
        fun show(laid: LaidOutChapter) {
            current = laid
            problem = null
            scroll.floatValue = 0f
            pin = null
        }

        // (Re)lay out the chapter whenever an input that affects it changes
        // (margin/spacing change the column width + rhythm, so re-lay out too).
        LaunchedEffect(chapterKey) {
            if (widthPx < 60f) return@LaunchedEffect
            // A back-swipe: the chapter is still laid out, so there is no native
            // line-break and no JSON parse to do — just show it again.
            val cached = chapters.get(chapterKey)
            if (cached != null) {
                show(cached)
                return@LaunchedEffect
            }
            val cfg = PlumblineLayoutConfig.ByValue().apply {
                width = column
                lineHeight = lineH
                spaceWidth = space
                verseNumGap = space * 1.4f
                paraIndent = lineH * 0.9f
                paraSpacing = lineH * 0.45f
                verseBreak = if (versePerLine) 1 else 0
                this.verseNumbers = if (verseNumbers) 1 else 0
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
                    // The cache takes ownership here — of this handle, and of the
                    // chapter this one replaces, which stays laid out for the
                    // swipe back. Nothing else in the pane closes a handle.
                    val laid = LaidOutChapter(chap, parsed)
                    chapters.put(chapterKey, laid)
                    show(laid)
                },
                onProblem = {
                    // The chapters already laid out are still good — this is one
                    // chapter that would not lay out, not a poisoned cache.
                    current = null; problem = it.message ?: "layout failed"
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

        // NOT the usual bottom-stop (content bottom meets screen bottom): the
        // reader may keep pushing until the chapter's LAST LINE reaches the TOP
        // of the pane — and no further. For reading on your back, where
        // something blocks the bottom of the screen and turning early means
        // moving your head (maintainer UAT ask, 2026-08-11). The LINE, not the
        // content bottom: stopping at the line's top keeps it on screen, where
        // the first cap let the text slide off entirely and left a blank pane
        // (UAT, 2026-08-12). Real scroll range, not an elastic overshoot — no
        // snap-back.
        val lastLineTop = remember(dl) {
            dl?.items?.asSequence()?.filter { it.kind == "word" }?.maxOfOrNull { it.y } ?: 0f
        }
        val maxScroll = if (dl != null) lastLineTop + marginPx else 0f

        // Scroll the navigator's target verse into view once the layout lands.
        // Epoch-guarded so a re-layout (font/margin change) doesn't re-jump.
        // Int state, like every other primitive on this path: a generic
        // mutableStateOf boxes the Int on each write, and the lint check that
        // would catch that — AutoboxingStateCreation — is one of the four
        // disabled in app/build.gradle.kts because AGP 8.7's lint crashes on
        // them, so nothing but this comment enforces it.
        var appliedTarget by remember { mutableIntStateOf(-1) }
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
        val columnNow = rememberUpdatedState(column)

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

        // And the text, recorded once and replayed per frame. Every input to that
        // recording is named in the key — read ChapterPaintKey before adding
        // anything to recordChapter, because what is missing from the key is what
        // stays on screen after it changes.
        val paintKey = ChapterPaintKey(
            layout = Same(dl),
            fonts = Same(typefaces),
            fontPx = fontPx,
            textH = textH,
            ascent = fm.ascent,
            inks = inks,
            addedItalics = addedItalics,
        )
        val recorder = remember { Recorded<Picture>() }
        val chapterPicture = dl?.let { list ->
            recorder.of(paintKey) {
                // Italics off hands the recorder the UPRIGHT paint for supplied
                // words. They stay marked by the `added` ink, which is the same
                // fallback a face with no italic already gets.
                recordChapter(list, regular, if (addedItalics) italic else regular, bold, inks, textH, fm.ascent)
            }
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
                    // A rotation or fold can leave the offset past the scroll
                    // range. Re-clamp HERE — from the layout phase — not in
                    // composition, which would write scroll state on every
                    // frame. Same bound as maxScroll above (last line to the
                    // pane's top), which no longer depends on the viewport.
                    scroll.floatValue = scroll.floatValue.coerceIn(0f, maxScroll)
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
                .pointerInput(current) {
                    detectTapGestures(
                        onTap = { pos ->
                            // Page-turn mode: a tap in either gutter pages the
                            // text instead of opening word study. 85% of a
                            // screen, the same portion the web's PageDown takes.
                            if (pageTurn && (pos.x < originXNow.value || pos.x > originXNow.value + columnNow.value)) {
                                val dir = if (pos.x > originXNow.value + columnNow.value) 1f else -1f
                                scroll.floatValue = (scroll.floatValue + dir * 0.85f * viewportH).coerceIn(0f, maxScroll)
                                return@detectTapGestures
                            }
                            val chap = current?.handle ?: return@detectTapGestures
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
                            val verse = verseAt(dl, pos, originXNow.value, scroll.floatValue, marginPx, book, chapter, engine, current?.handle)
                            if (verse != null) onVerseLongPress(verse)
                        },
                    )
                },
        ) {
            drawRect(palette.paper, size = size)

            // The recording exists exactly when the chapter does (it is derived
            // from the same `dl` a line above), so the two are one question.
            val picture = chapterPicture
            if (picture == null) {
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
                // The three live layers, in the order they have always painted and
                // all of them UNDER the recorded text (§5). They stay live because
                // they are tens of rectangles that change without a re-layout — a
                // tap moves the pin, a search moves the bands — and re-recording
                // 400–900 words for one of those would give the frame back the
                // cost this whole arrangement removes.
                //
                // Each still culls to the viewport: a wash for an item scrolled
                // off-screen must not paint, or it lands over the chrome above the
                // pane on scroll. clipToBounds is the safety net.
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

                // 5. The text itself — one call, whatever the chapter's length.
                // The per-word loop, and the viewport cull that went with it, are
                // inside the recording now (recordChapter); Skia rejects the lines
                // off-screen during playback, in native code, per frame.
                drawContext.canvas.nativeCanvas.drawPicture(picture)
            }
        }
    }
}

// ── the chapter, recorded once ──────────────────────────────────────────────

/**
 * Record a laid-out chapter's text into a [Picture]: the verse numbers, the
 * words, and the overlay's dotted mark, in the display list's own coordinate
 * space (the pane translates for the margin and the scroll when it replays).
 *
 * Painted straight, a screenful is 400–900 `Canvas.drawText` calls per frame —
 * a JNI crossing, a Java String marshalled across it and a shaping lookup for
 * every word, sixty times a second while a thumb is moving. Recorded, the
 * shaping happens once and a frame is a single `drawPicture`.
 *
 * Which is why what goes in here is load-bearing: a frame replays this without
 * consulting anything, so anything drawn here that changes without a re-record
 * is a chapter that stays on screen after the thing that changed it. Every
 * input this function reads is named in [ChapterPaintKey]; add nothing here
 * without adding it there.
 */
private fun recordChapter(
    list: DisplayList,
    regular: Paint,
    italic: Paint,
    bold: Paint,
    inks: ReaderInks,
    textH: Float,
    ascent: Float,
): Picture {
    val picture = Picture()
    // The recording's size is a CULL HINT, not a clip — no bounding-box hierarchy
    // is built for it, so playback still draws what falls outside — but a hint
    // that lies is worth nothing: this is the whole document, plus slack for the
    // last dot of an overlay mark at the right edge.
    val canvas = picture.beginRecording(
        ceil(list.width).toInt() + 4,
        ceil(list.height).toInt() + 4,
    )
    // The overlay's dotted rule gets its own paint: the text paints carry
    // typefaces and this is a filled 1.5×1 box. Anti-aliased, because the Compose
    // drawRect it replaces was.
    val mark = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = inks.gold }
    for (it in list.items) {
        val dyTop = it.y + (it.h - textH) * 0.5f
        val baseline = dyTop - ascent

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
        // Safe to mutate a shared Paint between draws: a recording COPIES the
        // paint into each op, so the colour set here is the colour that replays.
        paint.color = when {
            added -> inks.added
            divine -> inks.divine
            title -> inks.title
            else -> inks.ink
        }
        canvas.drawText(it.text, it.x, baseline, paint)
        // NO mark for a Strong's-tagged word. A faint gold rule under every one —
        // and most words carry a Strong's number — amounts to underlining the
        // Bible: noise that tells the reader nothing they act on. Whether a word
        // answers when tapped is learned once, not repeated by the page. Web
        // twin: paint.ts.
        //
        // The AKJV overlay's mark: DOTTED, at the natural underline depth. Not
        // bold or grey — italic already means "supplied by the translator", and
        // at 6.9% of words a heavy mark reads as a ransom note.
        if (flags and PlumblineFlags.RERENDERED != 0) {
            var dx = it.x
            val dy = it.y + it.h - 3f
            while (dx < it.x + it.w) {
                canvas.drawRect(dx, dy, dx + 1.5f, dy + 1f, mark)
                dx += 4f
            }
        }
    }
    picture.endRecording()
    return picture
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

// ── what a recording depends on, and when it is stale ───────────────────────

/**
 * Reference identity as a value: `==` on the wrapper asks "the very same
 * object?".
 *
 * That is both the question a paint key wants — a re-layout produces a NEW
 * display list, and the same object is by definition the same words in the same
 * boxes — and the only cheap one. `DisplayList` is a data class, so its own `==`
 * walks all ~700 items of an average chapter, and a key is compared on every
 * recomposition.
 */
internal class Same(val of: Any?) {
    override fun equals(other: Any?) = other is Same && of === other.of
    override fun hashCode() = System.identityHashCode(of)
    override fun toString() = "Same@${Integer.toHexString(System.identityHashCode(of))}"
}

/**
 * Everything the recorded chapter [Picture] is made of. A frame replays that
 * recording without looking at any of this, so an input missing HERE is text
 * that stays on screen after the thing that changed it — a stale chapter, which
 * is a far worse bug than the per-frame cost the recording removes.
 *
 * The inputs, and why each is its own field:
 *
 *  - [layout] — the display list, by reference. It carries every layout input
 *    with it: book, chapter, column width, line spacing, verse-per-line and the
 *    AKJV overlay all reach the page only by producing a new one.
 *  - [fonts] — the typefaces. Loaded once per pane today and so constant, keyed
 *    anyway: the day a face becomes a reader pref is not the day to remember
 *    this list existed.
 *  - [fontPx] — the paints' text size. NOT covered by [layout]: the paints
 *    change in the composition that reads the new pref, the display list only
 *    when the background layout lands, and between those two the page must be
 *    redrawn at the new size.
 *  - [textH] / [ascent] — the font metrics the baseline arithmetic uses.
 *  - [inks] — the palette's five text colours, packed. THE input with no layout
 *    behind it: a theme change re-lays out nothing, so this field alone is what
 *    repaints the chapter in the new ink.
 *
 * What is deliberately NOT here, because it is not recorded: the scroll offset
 * (a draw-phase read, applied as a translate over the replay), the search bands,
 * the note dots and the pinned span. Those three paint live beneath the picture
 * — they are tens of rectangles, they keep their z-order that way, and a tap
 * that moves the pin must not re-record 900 words. Move any of them into
 * [recordChapter] and its input belongs in this key.
 */
internal data class ChapterPaintKey(
    val layout: Same,
    val fonts: Same,
    val fontPx: Float,
    val textH: Float,
    val ascent: Float,
    val inks: ReaderInks,
    /** Whether supplied words were recorded in the italic face. In the key
     *  because the recording BAKES the choice: without it, turning the setting
     *  off would leave the italics on screen until something else happened to
     *  invalidate the picture. */
    val addedItalics: Boolean = true,
)

/**
 * A recording and the key it was made for: [of] hands back the recording,
 * re-making it exactly when the key changes.
 *
 * The pane could lean on Compose's `remember(key)` for this. It doesn't, because
 * then "an input changed, so the chapter was re-recorded" would be a fact about
 * Compose rather than something this repo can put a JVM test on — and the cost
 * of getting it wrong is a page that lies to the reader. ChapterPaintKeyTest
 * drives this class directly.
 */
internal class Recorded<T : Any> {
    private var key: ChapterPaintKey? = null
    private var value: T? = null

    /** How many recordings have actually been made — what the tests count. */
    var records: Int = 0
        private set

    fun of(key: ChapterPaintKey, record: (ChapterPaintKey) -> T): T {
        val held = value
        if (held != null && this.key == key) return held
        val made = record(key)
        this.key = key
        value = made
        records++
        return made
    }
}

// ── the chapters just behind the reader ─────────────────────────────────────

/**
 * What identifies a laid-out chapter: the passage, plus every input the core's
 * line-breaker saw.
 *
 * [column] already folds the pane's width and the reader's side margin — and
 * only those two, because the margin moves the text sideways at DRAW time, so a
 * margin change that leaves the column clamped at MAX_COLUMN correctly reuses
 * the layout. [lineHeight] folds the text size and the line-spacing pref,
 * [spaceWidth] the text size and the typeface, and the rest of
 * `PlumblineLayoutConfig` is derived from those two. [fontPx] is redundant with
 * them in all practical cases and kept regardless: a cached layout painted at
 * another size is mangled text, and the field costs nothing.
 */
internal data class ChapterKey(
    val book: String,
    val chapter: Int,
    val column: Float,
    val fontPx: Float,
    val lineHeight: Float,
    val spaceWidth: Float,
    val versePerLine: Boolean,
    val verseNumbers: Boolean,
    val akjvOverlay: Boolean,
)

/** A chapter the core has laid out: the native display list (which hit-tests
 *  taps, and which only [close] frees) and the parsed copy the pane paints. */
internal class LaidOutChapter(val handle: Chapter, val list: DisplayList) : Closeable {
    override fun close() = handle.close()
}

/**
 * The last few laid-out chapters, least recently used first.
 *
 * A back-swipe goes to a chapter the reader was on seconds ago, and re-reaching
 * it means a native line-break of every word plus a JSON parse of the result.
 * Keeping it instead costs ~350 KB for an average chapter (691 display items at
 * a measured ~327 bytes on the JVM plus ~185 native) and ~1.3 MB for Psalm 119's
 * 2,643 items, which is why this is bounded at [CHAPTER_CACHE] and not at
 * "chapters visited".
 *
 * The cache OWNS every entry: an evicted one is closed here, and nothing else in
 * the pane closes a handle. That is safe only because of one discipline, which
 * the pane keeps and ChapterPaintCacheTest pins — the chapter on screen is
 * always the entry most recently [get] or [put], and eviction only ever takes
 * the least recent. With [capacity] of two or more the entry being painted and
 * hit-tested therefore cannot be the victim; at one it would be freed under the
 * reader's thumb, which is why the constructor refuses.
 *
 * A change that re-shapes every chapter — the text size, the column — strands the
 * entries laid out the old way: they can never be asked for again, and they leave
 * as the next few layouts push them out. Bounded by [capacity] and so not worth a
 * mechanism; deliberately not a sweep, because the only entry it would be safe to
 * close early is the one the pane is painting.
 */
internal class ChapterCache<K : Any, V : Closeable>(private val capacity: Int) : Closeable {

    init {
        require(capacity >= 2) {
            "a chapter cache of $capacity would evict the chapter on screen"
        }
    }

    /** Least recently used first — a LinkedHashMap keeps insertion order, and
     *  re-inserting on every touch is the whole LRU at these sizes. */
    private val entries = LinkedHashMap<K, V>()

    /** The entry for [key], now the most recently used. */
    fun get(key: K): V? {
        val held = entries.remove(key) ?: return null
        entries[key] = held
        return held
    }

    /** Take ownership of [value] as the most recently used entry, closing the
     *  least recently used one if that puts the cache over [capacity]. */
    fun put(key: K, value: V) {
        // Replacing a key: whatever was there can no longer be reached, so it is
        // freed here. The pane never arrives this way — it looks a key up before
        // laying it out — but a leak that depends on a call order is not a leak
        // worth keeping.
        entries.remove(key)?.let { if (it !== value) it.close() }
        entries[key] = value
        while (entries.size > capacity) {
            val oldest = entries.keys.first()
            entries.remove(oldest)?.close()
        }
    }

    /** Keys, least recently used first — the tests' window on the ordering. */
    fun lruOrder(): List<K> = entries.keys.toList()

    val size: Int get() = entries.size

    /** Free every chapter: the pane has left the tree. */
    override fun close() {
        for (held in entries.values) held.close()
        entries.clear()
    }
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
 *  conversions a frame for five values that change only with the theme.
 *
 *  A data class on purpose. These five numbers are the only trace the theme
 *  leaves in [ChapterPaintKey], and comparing them by VALUE is what makes a
 *  theme change re-record the chapter — identity would not, because the palette
 *  object is rebuilt whether or not its colours moved. */
internal data class ReaderInks(
    val ink: Int,
    val added: Int,
    val divine: Int,
    val title: Int,
    val gold: Int,
) {
    constructor(palette: ReaderPalette) : this(
        ink = palette.ink.toArgbInt(),
        added = palette.inkFaded.toArgbInt(),
        divine = palette.divine.toArgbInt(),
        title = palette.titleInk.toArgbInt(),
        gold = palette.gold.toArgbInt(),
    )
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
    // it, which is what `firstOrNull { it.y + it.h > scrollY }` finds.
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
    // and since verses only ever run down the page that is the same verse a
    // filter-every-word-then-max would arrive at.
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
