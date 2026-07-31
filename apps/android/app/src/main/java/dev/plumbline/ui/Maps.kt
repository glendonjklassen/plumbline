// The three "map" visualisations, as full-screen Compose Canvas destinations
// that pinch-zoom and pan. These are the Android mirror of the GTK popups
// (apps/desktop/src/main.rs: draw_concept_radial + draw_dispersion +
// show_concept_map, draw_constellation, draw_chord_map) and the WinUI popups
// (apps/windows/PureStudyWin/Popups.cs: ConceptMap, Constellation, ChordMap).
//
// Faithful, not simplified: every geometry constant + banding formula mirrors
// the WinUI shell (whose bridge-row banding math is the exact reference), so a
// node/band lands in the same relative spot in every shell. All the study logic
// lives across the ABI — the shell only paints the returned view-models
// (ConceptMapJson / ConstellationJson / ChordMapJson / CanonSegmentsJson) and
// forwards taps back. Text is measured + drawn through a Paint-backed
// android.graphics.Canvas, the same thin-shell convention as ReaderPane.kt.
//
// Interaction that is a mouse affordance on the desktop (hover tooltips) is
// dropped on touch; taps navigate directly. The whole canvas zooms/pans as one
// (a product decision) via Modifier.zoomable — a detectTransformGestures +
// graphicsLayer transform reused by all three.
//
// Author D (Compose UI).

package dev.plumbline.ui

import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.sp
import dev.plumbline.CanonSegments
import dev.plumbline.ChordMapData
import dev.plumbline.ConceptMapData
import dev.plumbline.ConstellationData
import dev.plumbline.StudyEngine
import dev.plumbline.TocBook
import dev.plumbline.parseWire
import kotlinx.coroutines.delay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin

// ── reusable pinch-zoom + pan ────────────────────────────────────────────────

/**
 * Hoisted transform state for [zoomable]. Content point `p` renders at screen
 * position `offset + scale * p` (the layer's transform origin is the top-left,
 * so scaling is anchored there and the map below stays exact). [toContent]
 * inverts that so a tap in screen space can be hit-tested against the unscaled
 * drawing.
 */
class ZoomState(minScale: Float = 1f, maxScale: Float = 8f) {
    private val minScale = minScale
    private val maxScale = maxScale

    var scale by mutableStateOf(1f)
        private set
    var offset by mutableStateOf(Offset.Zero)
        private set
    private var viewportW = 0f
    private var viewportH = 0f

    /** Record the canvas size so the pan can be bounded to the content. */
    fun setViewport(w: Float, h: Float) {
        viewportW = w
        viewportH = h
        offset = clamp(offset, scale)
    }

    /** Keep the scaled content covering the viewport — no empty gutters, and it
     *  can't be flung off-screen. At scale 1 this pins the offset to 0 (fitted). */
    private fun clamp(o: Offset, s: Float): Offset {
        val minX = minOf(0f, viewportW * (1f - s))
        val minY = minOf(0f, viewportH * (1f - s))
        return Offset(o.x.coerceIn(minX, 0f), o.y.coerceIn(minY, 0f))
    }

    /** Apply one transform-gesture step (centroid/pan in screen space). Zoom is
     *  anchored on the centroid so the point under the fingers stays put; the
     *  result is clamped so the map stays within its frame. */
    fun onGesture(centroid: Offset, pan: Offset, zoom: Float) {
        val newScale = (scale * zoom).coerceIn(minScale, maxScale)
        val content = (centroid - offset) / scale       // point under the centroid
        offset = clamp(centroid - content * newScale + pan, newScale)
        scale = newScale
    }

    /** Map a screen-space point back into the canvas's own (unscaled) space. */
    fun toContent(screen: Offset): Offset = (screen - offset) / scale

    fun reset() {
        scale = 1f
        offset = Offset.Zero
    }
}

@Composable
fun rememberZoomState(minScale: Float = 1f, maxScale: Float = 8f): ZoomState =
    remember { ZoomState(minScale, maxScale) }

/**
 * Make a composable pinch-zoomable + pannable. The gesture detector sits
 * *outside* the [graphicsLayer] so its centroid/pan arrive in screen space
 * (matching [ZoomState]'s model); any tap detector the caller adds must sit
 * before this modifier for the same reason, then use [ZoomState.toContent].
 */
fun Modifier.zoomable(state: ZoomState): Modifier =
    this
        .pointerInput(Unit) {
            detectTransformGestures { centroid, pan, zoom, _ ->
                state.onGesture(centroid, pan, zoom)
            }
        }
        .graphicsLayer {
            scaleX = state.scale
            scaleY = state.scale
            translationX = state.offset.x
            translationY = state.offset.y
            transformOrigin = TransformOrigin(0f, 0f)
            // Clip the panned/zoomed drawing to the canvas bounds. Without this a
            // dragged map overflows its frame and — being later in draw order —
            // paints over the overlay's back bar (and any chrome above it).
            clip = true
        }

// ── shared paint helpers (ReaderPane's convention: measure + draw via Paint) ──

/** The reader's own regular face, not a fourth parse of the same file: the map
 *  popups draw the same Garamond the chapter does, and [readerTypefaces] has
 *  already paid for it once per process (its fallback to the platform serif
 *  covers the missing-asset case this used to handle itself). */
private fun mapTypeface(context: Context): Typeface = readerTypefaces(context).regular

@Composable
internal fun rememberMapPaint(): Paint {
    val context = LocalContext.current
    return remember {
        Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = mapTypeface(context) }
    }
}

/** Compose [Color] → packed ARGB int for android.graphics.Paint. */
private fun Color.toArgbInt(): Int {
    val a = (alpha * 255f + 0.5f).toInt()
    val r = (red * 255f + 0.5f).toInt()
    val g = (green * 255f + 0.5f).toInt()
    val b = (blue * 255f + 0.5f).toInt()
    return (a shl 24) or (r shl 16) or (g shl 8) or b
}

/** Draw a (possibly multi-line "gloss\nlemma") label. [top] is the y of the top
 *  of the first line; alignment fixes the horizontal anchor at [x]. */
private fun DrawScope.drawLabel(
    paint: Paint,
    lines: List<String>,
    x: Float,
    top: Float,
    align: Paint.Align,
    colorInt: Int,
) {
    paint.textAlign = align
    paint.color = colorInt
    val fm = paint.fontMetrics
    val lineH = fm.descent - fm.ascent
    val nc = drawContext.canvas.nativeCanvas
    for ((i, line) in lines.withIndex()) {
        nc.drawText(line, x, top - fm.ascent + i * lineH, paint)
    }
}

/** The total height a [lines] label occupies at the paint's current text size. */
private fun Paint.labelHeight(lines: Int): Float {
    val fm = fontMetrics
    return lines * (fm.descent - fm.ascent)
}

/** Render the wire label "gloss\nlemma" as GTK's "gloss (lemma)" (falling back
 *  to whichever line exists) — the bridge-caption naming, matching WinUI. */
internal fun partnerName(label: String): String {
    val parts = label.split('\n')
    return if (parts.size >= 2 && parts[1].isNotEmpty()) "${parts[0]} (${parts[1]})" else parts[0]
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Concept map — radial neighbourhood over a banded dispersion strip
// ═══════════════════════════════════════════════════════════════════════════

/**
 * What a map shows while it is being built. The FIRST analytical map of a
 * session pays for a corpus-wide sweep; the rest are instant. Several seconds
 * under a bare label reads as a hang (feedback 2026-07-27), so once the wait is
 * real, say it is one-time. Web twin: MapFrame.svelte's `slow`.
 */
@Composable
private fun MapBuilding(label: String, palette: ReaderPalette) {
    var slow by remember { mutableStateOf(false) }
    LaunchedEffect(label) {
        delay(600)
        slow = true
    }
    Column(
        Modifier.padding(horizontal = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(label, color = palette.faded)
        if (slow) {
            Text(
                "The first map of a session takes a few seconds: the whole text is being swept " +
                    "for this. The maps you open after it appear at once.",
                color = palette.faded,
                fontSize = 12.5.sp,
                lineHeight = 18.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 10.dp),
            )
        }
    }
}

/**
 * The concept map for [code]: a radial neighbourhood (centre label + spokes,
 * gold = semantic / green = community) over a canon dispersion strip. When the
 * code has a cross-testament bridge, an indigo partner row bands the strip
 * beneath a 1px gutter, with a caption naming the partners. Full-screen,
 * pinch-zoom + pan; double-tap resets. Mirrors WinUI Popups.ConceptMap.
 *
 * Opened by the word-study `conceptmap:CODE` link (wire verb `conceptMap`).
 */
@Composable
fun ConceptMap(
    engine: StudyEngine,
    code: String,
    palette: ReaderPalette,
    modifier: Modifier = Modifier,
) {
    val paint = rememberMapPaint()
    val zoom = rememberZoomState()

    var data by remember(code) { mutableStateOf<ConceptMapData?>(null) }
    var loading by remember(code) { mutableStateOf(true) }

    // First call builds the analytics engine (~seconds); do it off the main thread.
    LaunchedEffect(code) {
        loading = true
        val parsed = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) { engine.ConceptMapJson(code) }
                    ?.let { parseWire<ConceptMapData>(it) }
            }.getOrNull()
        }
        data = parsed
        loading = false
    }

    val caption = remember(data) {
        data?.bridge?.partners?.takeIf { it.isNotEmpty() }
            ?.let { "↔ across testaments: " + it.joinToString(", ") { p -> partnerName(p.label) } }
    }

    Box(
        modifier.fillMaxSize().background(palette.panelBg),
        contentAlignment = Alignment.Center,
    ) {
        val map = data
        when {
            loading -> MapBuilding("Building concept map…", palette)
            map == null -> Text("No concept data for $code.", color = palette.faded)
            else -> Canvas(
                Modifier
                    .fillMaxSize()
                    .onSizeChanged { zoom.setViewport(it.width.toFloat(), it.height.toFloat()) }
                    .pointerInput(Unit) { detectTapGestures(onDoubleTap = { zoom.reset() }) }
                    .zoomable(zoom),
            ) {
                val stripH = 56.dp.toPx()
                drawConceptRadial(map, paint, palette, size.height - stripH)
                drawDispersionStrip(map, paint, palette, size.height - stripH, stripH, caption)
            }
        }
    }
}

/** The radial neighbourhood (spokes + centre node) into the top [mapH] px of the
 *  current draw scope. Geometry mirrors WinUI Popups.ConceptMap; shared by the
 *  fullscreen map and the study panel's embedded card. */
internal fun DrawScope.drawConceptRadial(
    map: ConceptMapData,
    paint: Paint,
    palette: ReaderPalette,
    mapH: Float,
) {
    val w = size.width
    val cx = w / 2f
    val cy = mapH / 2f
    // Floor high enough that even the embedded card's ring clears the centre
    // label (drawn BELOW the node since 2026-07-26 — above, it superimposed
    // the centre word over the 12-o'clock spoke).
    val rOuter = max(min(w, mapH) / 2f - 95.dp.toPx(), 64.dp.toPx())
    // Relatedness → distance: the strongest semantic neighbour sits closest.
    // Community spokes (no weight) draw at the outer ring. The inner floor
    // keeps even the closest spoke clear of the centre label block.
    val rInner = max(rOuter * 0.55f, 56.dp.toPx())
    val weights = map.spokes.mapNotNull { it.weight }
    val wMin = weights.minOrNull() ?: 0.0
    val wMax = weights.maxOrNull() ?: 0.0
    fun spokeRadius(weight: Double?): Float {
        if (weight == null || wMax <= wMin) return rOuter
        val t = ((weight - wMin) / (wMax - wMin)).toFloat()
        return rOuter - (rOuter - rInner) * t
    }

    val goldStroke = palette.gold.copy(alpha = 0.5f)
    val greenStroke = Color(red = 107, green = 140, blue = 102, alpha = 128)
    val goldNode = palette.gold.copy(alpha = 0.9f)

    // ── spokes ──
    paint.textSize = 12.sp.toPx()
    val spokes = map.spokes
    val n = max(1, spokes.size)
    for (i in spokes.indices) {
        val angle = 2.0 * PI * i / n - PI / 2.0
        val ca = cos(angle).toFloat()
        val sa = sin(angle).toFloat()
        val radius = spokeRadius(spokes[i].weight)
        val nx = cx + radius * ca
        val ny = cy + radius * sa
        drawLine(
            if (spokes[i].semantic) goldStroke else greenStroke,
            Offset(cx, cy), Offset(nx, ny), strokeWidth = 1.4.dp.toPx(),
        )
        drawCircle(goldNode, radius = 3.dp.toPx(), center = Offset(nx, ny))

        val lines = spokes[i].label.split('\n')
        val th = paint.labelHeight(lines.size)
        val align: Paint.Align
        val lx: Float
        var top: Float
        when {
            ca > 0.35f -> { align = Paint.Align.LEFT; lx = nx + 9.dp.toPx(); top = ny - th / 2f }
            ca < -0.35f -> { align = Paint.Align.RIGHT; lx = nx - 9.dp.toPx(); top = ny - th / 2f }
            sa < 0f -> { align = Paint.Align.CENTER; lx = nx; top = ny - 10.dp.toPx() - th }
            else -> { align = Paint.Align.CENTER; lx = nx; top = ny + 9.dp.toPx() }
        }
        top = top.coerceIn(2f, max(2f, mapH - th - 2f))
        drawLabel(paint, lines, lx, top, align, palette.ink.toArgbInt())
    }

    // ── centre node ──
    // Label BELOW the node (web-shell parity): drawn above, it sat exactly on
    // the 12-o'clock spoke whenever the radius floor bound (the embedded card,
    // small phones) — the centre word superimposed over a related concept.
    drawCircle(palette.gold, radius = 5.dp.toPx(), center = Offset(cx, cy))
    paint.textSize = 15.sp.toPx()
    val centreLines = map.centerLabel.split('\n')
    drawLabel(
        paint, centreLines, cx, cy + 12.dp.toPx(),
        Paint.Align.CENTER, palette.ink.toArgbInt(),
    )
}

/** The canon dispersion strip at [y0]..[y0]+[stripH] (banding mirrors WinUI
 *  exactly): gold = where the code occurs, indigo bridge row = where its
 *  cross-testament partners occur, OT/NT seam, optional [caption] just above.
 *  Shared by the fullscreen map and the study panel's embedded heatmap. */
internal fun DrawScope.drawDispersionStrip(
    map: ConceptMapData,
    paint: Paint,
    palette: ReaderPalette,
    y0: Float,
    stripH: Float,
    caption: String?,
) {
    val w = size.width
    val bridge = map.bridge
    val hasBridge = bridge != null && bridge.byBook.any { it > 0 }
    if (map.byBook.none { it > 0 } && !hasBridge) return

    val bc = max(1, map.bookCount).toFloat()
    drawRect(Color.Black.copy(alpha = 10f / 255f), Offset(0f, y0), Size(w, stripH))

    val gap = 1f
    val primH = if (hasBridge) max((stripH - gap) * 0.55f, 1f) else stripH
    val brdgY = if (hasBridge) primH + gap else stripH
    val brdgH = if (hasBridge) stripH - primH - gap else 0f

    // Primary dispersion (gold): where CODE itself occurs. Alpha ∝ this row's
    // own max (GTK 0.62,0.49,0.22 == palette.gold).
    val bmax = max(1, map.byBook.maxOrNull() ?: 1).toFloat()
    for (bi in map.byBook.indices) {
        val cnt = map.byBook[bi]
        if (cnt == 0) continue
        val alpha = 0.15f + 0.75f * cnt / bmax
        val x0 = bi / bc * w
        val x1 = (bi + 1) / bc * w
        drawRect(palette.gold.copy(alpha = alpha), Offset(x0, y0), Size(x1 - x0, primH))
    }

    // Bridge dispersion (indigo): where the cross-testament partners occur.
    // Alpha ∝ the bridge row's OWN max; rgb 77,89,158.
    if (hasBridge) {
        val bb = bridge!!.byBook
        val pmax = max(1, bb.maxOrNull() ?: 1).toFloat()
        for (bi in bb.indices) {
            val cnt = bb[bi]
            if (cnt == 0) continue
            val alpha = 0.18f + 0.72f * cnt / pmax
            val x0 = bi / bc * w
            val x1 = (bi + 1) / bc * w
            drawRect(
                Color(red = 77, green = 89, blue = 158, alpha = (alpha * 255f).toInt()),
                Offset(x0, y0 + brdgY), Size(x1 - x0, brdgH),
            )
        }
    }

    // OT/NT seam (full height of the strip).
    val seam = map.otNtDivide / bc * w
    drawLine(
        Color(red = 102, green = 77, blue = 51, alpha = 128),
        Offset(seam, y0), Offset(seam, y0 + stripH), strokeWidth = 1.dp.toPx(),
    )

    // Caption naming the bridge partners, dim, just above the strip.
    caption?.let { cap ->
        paint.textSize = 11.sp.toPx()
        drawLabel(
            paint, listOf(cap), 8.dp.toPx(),
            y0 - paint.labelHeight(1) - 3.dp.toPx(),
            Paint.Align.LEFT,
            Color(red = 89, green = 77, blue = 56, alpha = 190).toArgbInt(),
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Constellation — the weave library as labelled lanes over the canon
// ═══════════════════════════════════════════════════════════════════════════

private val LaneColors = arrayOf(
    Triple(210, 180, 110), Triple(127, 180, 230), Triple(143, 184, 138),
    Triple(217, 140, 140), Triple(184, 156, 214), Triple(150, 194, 190),
    Triple(214, 170, 128),
)

/**
 * The constellation: one page of the weave library as labelled lanes over the
 * canon backbone (nodes sized by witness degree, links as gentle curves). A
 * prev/next bar pages the free lanes past the pinned ones. Tap a node to jump
 * the reader there, an edge to open its weave, the pin gutter to pin/unpin.
 * Full-screen, pinch-zoom + pan; double-tap resets. Mirrors WinUI
 * Popups.Constellation. (Hover tooltips are a mouse affordance; omitted on
 * touch.)
 */
@Composable
fun Constellation(
    engine: StudyEngine,
    palette: ReaderPalette,
    modifier: Modifier = Modifier,
    onNavigate: (book: String, chapter: Int, verse: String?) -> Unit = { _, _, _ -> },
    onOpenWeave: (index: Int) -> Unit = {},
) {
    val paint = rememberMapPaint()
    val zoom = rememberZoomState()

    var page by remember { mutableStateOf(0) }
    val pins = remember { mutableStateListOf<Int>() }
    var model by remember { mutableStateOf<ConstellationData?>(null) }
    var segments by remember { mutableStateOf<CanonSegments?>(null) }
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }

    LaunchedEffect(Unit) {
        segments = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.CanonSegmentsJson() }?.let { parseWire<CanonSegments>(it) } }
                .getOrNull()
        }
    }

    LaunchedEffect(page, pins.toList()) {
        val parsed = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) { engine.ConstellationJson(page, pins) }
                    ?.let { parseWire<ConstellationData>(it) }
            }.getOrNull()
        }
        model = parsed
        if (parsed != null && parsed.page != page) page = parsed.page  // the core clamps
    }

    // Paint-only geometry (mirrors WinUI): pin gutter, plot left margin, top pad.
    // The WinUI constants are DIPs; scale by density so a lane lands identically.
    val d = LocalDensity.current.density
    val gutterPx = 150f * d
    val plotLeftPx = 162f * d
    val topPadPx = 18f * d
    val bottomPx = 10f * d

    fun laneH(h: Float): Float = (h - topPadPx - bottomPx) / max(1, model?.laneCapacity ?: 1)
    fun nodeXY(xFrac: Float, laneFrac: Float, lane: Int, w: Float, h: Float): Offset =
        Offset(plotLeftPx + xFrac * (w - plotLeftPx), topPadPx + (lane + laneFrac) * laneH(h))

    Column(modifier.fillMaxSize().background(palette.panelBg)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TextButton(onClick = { if (page > 0) page -= 1 }) { Text("‹ prev", color = palette.ink) }
            TextButton(onClick = { page += 1 }) { Text("next ›", color = palette.ink) }
            Text(model?.caption ?: "", color = palette.faded, fontSize = 12.sp)
        }

        Box(Modifier.weight(1f).fillMaxWidth()) {
            Canvas(
                Modifier
                    .fillMaxSize()
                    .onSizeChanged { canvasSize = it; zoom.setViewport(it.width.toFloat(), it.height.toFloat()) }
                    .pointerInput(model) {
                        detectTapGestures(
                            onDoubleTap = { zoom.reset() },
                            onTap = { screen ->
                                val m = model ?: return@detectTapGestures
                                val w = canvasSize.width.toFloat()
                                val h = canvasSize.height.toFloat()
                                val p = zoom.toContent(screen)
                                // Priority: node > edge > pin gutter (matches WinUI).
                                var bestLane = -1
                                var bestNode = -1
                                var bestD = Float.MAX_VALUE
                                // Node/edge half-widths + slop are DIPs (rendered
                                // dp-scaled), and touch wants a comfier target than
                                // the mouse — so scale the WinUI slop by density.
                                for ((lane, l) in m.lanes.withIndex()) {
                                    for ((ni, node) in l.nodes.withIndex()) {
                                        val c = nodeXY(node.x, node.laneFrac, lane, w, h)
                                        val half = (1.4f + 2.4f * node.size) * d
                                        val dd = dist(p, c)
                                        if (dd <= half + 10f * d && dd < bestD) {
                                            bestD = dd; bestLane = lane; bestNode = ni
                                        }
                                    }
                                }
                                if (bestLane >= 0) {
                                    val node = m.lanes[bestLane].nodes[bestNode]
                                    onNavigate(node.book, node.chapter, node.refKey)
                                    return@detectTapGestures
                                }
                                for ((lane, l) in m.lanes.withIndex()) {
                                    for (e in l.edges) {
                                        val a = nodeXY(e.aX, e.aLaneFrac, lane, w, h)
                                        val b = nodeXY(e.bX, e.bLaneFrac, lane, w, h)
                                        if (curveDist(p, a, b) <= 8f * d) {
                                            onOpenWeave(l.weaveIndex)
                                            return@detectTapGestures
                                        }
                                    }
                                }
                                if (p.x < gutterPx) {
                                    val lane = ((p.y - topPadPx) / laneH(h)).toInt()
                                    if (lane in m.lanes.indices) {
                                        val idx = m.lanes[lane].weaveIndex
                                        if (!pins.remove(idx)) pins.add(idx)
                                    }
                                }
                            },
                        )
                    }
                    .zoomable(zoom),
            ) {
                val m = model ?: return@Canvas
                val w = size.width
                val h = size.height
                val lh = laneH(h)
                val cap = m.laneCapacity
                val segs = segments
                val bookCount = ((segs?.segments?.maxOfOrNull { it.last } ?: 65) + 1).toFloat()

                // Alternating lane bands over the full capacity.
                for (i in 0 until cap) {
                    if (i % 2 == 0) {
                        drawRect(
                            Color.Black.copy(alpha = 8f / 255f),
                            Offset(0f, topPadPx + i * lh), Size(w, lh),
                        )
                    }
                }

                // Canon section dividers + ruler labels, then the OT/NT seam.
                paint.textSize = 10.sp.toPx()
                segs?.segments?.forEach { seg ->
                    val x = plotLeftPx + seg.first / bookCount * (w - plotLeftPx)
                    drawLine(Color.Black.copy(alpha = 26f / 255f), Offset(x, topPadPx), Offset(x, h), 1.dp.toPx())
                    drawLabel(
                        paint, listOf(seg.label), x + 2.dp.toPx(), 2.dp.toPx(),
                        Paint.Align.LEFT, Color(red = 89, green = 77, blue = 56, alpha = 180).toArgbInt(),
                    )
                }
                val ot = segs?.otNtDivide ?: 39
                val seam = plotLeftPx + ot / bookCount * (w - plotLeftPx)
                drawLine(palette.gold.copy(alpha = 153f / 255f), Offset(seam, topPadPx), Offset(seam, h), 1.dp.toPx())

                // Per lane: pin marker + name, edges (under), nodes (over).
                paint.textSize = 10.5f.sp.toPx()
                for ((lane, l) in m.lanes.withIndex()) {
                    val (br, bg, bb) = LaneColors[lane % LaneColors.size]
                    val edgeCol = Color(
                        red = (br * 0.72f).toInt(), green = (bg * 0.72f).toInt(),
                        blue = (bb * 0.72f).toInt(), alpha = 128,
                    )
                    val nodeCol = Color(
                        red = (br * 0.72f).toInt(), green = (bg * 0.72f).toInt(),
                        blue = (bb * 0.72f).toInt(), alpha = 230,
                    )
                    val cyMid = topPadPx + lane * lh + lh / 2f
                    if (l.pinned) {
                        drawRect(palette.gold, Offset(6.dp.toPx(), cyMid - 4.dp.toPx()), Size(8.dp.toPx(), 8.dp.toPx()))
                    } else {
                        drawRect(
                            Color(red = 100, green = 100, blue = 100, alpha = 153),
                            Offset(6.5.dp.toPx(), cyMid - 3.5.dp.toPx()),
                            Size(7.dp.toPx(), 7.dp.toPx()),
                            style = Stroke(width = 1.dp.toPx()),
                        )
                    }
                    val name = if (l.name.length > 22) l.name.substring(0, 22) else l.name
                    drawLabel(
                        paint, listOf(name), 18.dp.toPx(), cyMid - 7.dp.toPx(), Paint.Align.LEFT,
                        (if (l.pinned) Color(red = 140, green = 107, blue = 38) else Color(red = 89, green = 84, blue = 77)).toArgbInt(),
                    )

                    for (e in l.edges) {
                        val a = nodeXY(e.aX, e.aLaneFrac, lane, w, h)
                        val b = nodeXY(e.bX, e.bLaneFrac, lane, w, h)
                        val dx = b.x - a.x
                        val path = Path().apply {
                            moveTo(a.x, a.y)
                            cubicTo(a.x + dx * 0.4f, a.y, b.x - dx * 0.4f, b.y, b.x, b.y)
                        }
                        drawPath(path, edgeCol, style = Stroke(width = 1.dp.toPx()))
                    }
                    for (node in l.nodes) {
                        val c = nodeXY(node.x, node.laneFrac, lane, w, h)
                        val half = (1.4f + 2.4f * node.size).dp.toPx()
                        drawRect(nodeCol, Offset(c.x - half, c.y - half), Size(half * 2, half * 2))
                    }
                }
            }
        }
    }
}

/** Distance from [p] to the drawn connector cubic between [a] and [b] (18
 *  samples, like the desktop's curve_samples), for edge hit-testing. */
private fun curveDist(p: Offset, a: Offset, b: Offset): Float {
    val dx = b.x - a.x
    val c1 = Offset(a.x + dx * 0.4f, a.y)
    val c2 = Offset(b.x - dx * 0.4f, b.y)
    var best = Float.MAX_VALUE
    var prev = a
    for (i in 1..18) {
        val t = i / 18f
        val u = 1f - t
        val q = Offset(
            u * u * u * a.x + 3 * u * u * t * c1.x + 3 * u * t * t * c2.x + t * t * t * b.x,
            u * u * u * a.y + 3 * u * u * t * c1.y + 3 * u * t * t * c2.y + t * t * t * b.y,
        )
        best = min(best, segDist(p, prev, q))
        prev = q
    }
    return best
}

private fun segDist(p: Offset, a: Offset, b: Offset): Float {
    val vx = b.x - a.x
    val vy = b.y - a.y
    val len2 = vx * vx + vy * vy
    val t = if (len2 <= 0f) 0f else (((p.x - a.x) * vx + (p.y - a.y) * vy) / len2).coerceIn(0f, 1f)
    return dist(p, Offset(a.x + t * vx, a.y + t * vy))
}

/** Euclidean distance between two points ([Offset.getDistance] avoids the
 *  Double-only kotlin.math.hypot). */
private fun dist(a: Offset, b: Offset): Float = (a - b).getDistance()

// ═══════════════════════════════════════════════════════════════════════════
// 3. Chord map — canon-ordered book-pair weave ribbons
// ═══════════════════════════════════════════════════════════════════════════

/**
 * The book-to-book weave chord map: how strongly each pair of books is woven,
 * as arc ribbons over the canon axis (gold = OT↔OT, blue = NT↔NT, purple =
 * cross-testament). Tap a book column to open it in the reader. Full-screen,
 * pinch-zoom + pan; double-tap resets. Mirrors WinUI Popups.ChordMap. ([books]
 * maps a tapped column to a book id; hover naming is a mouse affordance, omitted
 * on touch.)
 */
@Composable
fun ChordMap(
    engine: StudyEngine,
    books: List<TocBook>,
    palette: ReaderPalette,
    modifier: Modifier = Modifier,
    onPickBook: (book: String) -> Unit = {},
) {
    val paint = rememberMapPaint()
    val zoom = rememberZoomState()

    var data by remember { mutableStateOf<ChordMapData?>(null) }
    var segments by remember { mutableStateOf<CanonSegments?>(null) }
    var loading by remember { mutableStateOf(true) }
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }

    LaunchedEffect(Unit) {
        val result = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) {
                    val cm = engine.ChordMapJson()?.let { parseWire<ChordMapData>(it) }
                    val cs = engine.CanonSegmentsJson()?.let { parseWire<CanonSegments>(it) }
                    cm to cs
                }
            }.getOrNull()
        }
        data = result?.first
        segments = result?.second
        loading = false
    }

    Box(
        modifier.fillMaxSize().background(palette.panelBg),
        contentAlignment = Alignment.Center,
    ) {
        val map = data
        when {
            loading -> MapBuilding("Building weave map…", palette)
            map == null || map.pairs.isEmpty() -> Text("No weaves to map yet.", color = palette.faded)
            else -> Canvas(
                Modifier
                    .fillMaxSize()
                    .onSizeChanged { canvasSize = it; zoom.setViewport(it.width.toFloat(), it.height.toFloat()) }
                    .pointerInput(map) {
                        detectTapGestures(
                            onDoubleTap = { zoom.reset() },
                            onTap = { screen ->
                                val bc = max(1, map.bookCount)
                                val cx = zoom.toContent(screen).x
                                val idx = (cx / max(1f, canvasSize.width.toFloat()) * bc).toInt().coerceIn(0, bc - 1)
                                if (idx in books.indices) onPickBook(books[idx].id)
                            },
                        )
                    }
                    .zoomable(zoom),
            ) {
                val w = size.width
                val h = size.height
                val bc = max(1, map.bookCount).toFloat()
                val y0 = h - 26.dp.toPx()
                val maxC = max(1, map.max).toFloat()
                fun bookX(i: Int): Float = (i + 0.5f) / bc * w

                // Section bands + labels.
                paint.textSize = 10.sp.toPx()
                segments?.segments?.forEachIndexed { k, seg ->
                    val x0 = seg.first / bc * w
                    val x1 = (seg.last + 1) / bc * w
                    if (k % 2 == 1) drawRect(Color.Black.copy(alpha = 10f / 255f), Offset(x0, 0f), Size(x1 - x0, y0))
                    drawLabel(
                        paint, listOf(seg.label), x0 + 3.dp.toPx(), y0 + 6.dp.toPx(),
                        Paint.Align.LEFT, Color(red = 89, green = 77, blue = 56, alpha = 230).toArgbInt(),
                    )
                }

                // Book ticks, baseline, OT/NT seam.
                for (b in 0..map.bookCount) {
                    val tx = b / bc * w
                    drawLine(Color(red = 89, green = 77, blue = 56, alpha = 60), Offset(tx, y0), Offset(tx, y0 - 4.dp.toPx()), 1.dp.toPx())
                }
                drawLine(palette.gold.copy(alpha = 128f / 255f), Offset(0f, y0), Offset(w, y0), 1.5.dp.toPx())
                val seam = map.otNtDivide / bc * w
                drawLine(Color(red = 102, green = 77, blue = 51, alpha = 128), Offset(seam, 0f), Offset(seam, y0), 1.dp.toPx())

                // Ribbons, lightest→heaviest so thin ones aren't buried (WinUI order).
                val divide = map.otNtDivide
                for (pair in map.pairs.sortedBy { it.count }) {
                    val frac = pair.count / maxC
                    val aOt = pair.a < divide
                    val bOt = pair.b < divide
                    val cross = aOt != bOt
                    val color = when {
                        aOt && bOt -> Color(red = 0.72f, green = 0.57f, blue = 0.24f)     // OT gold
                        !aOt && !bOt -> Color(red = 0.30f, green = 0.53f, blue = 0.78f)    // NT blue
                        else -> Color(red = 0.58f, green = 0.38f, blue = 0.70f)            // cross purple
                    }
                    val alpha = min(0.25f + 0.45f * frac + if (cross) 0.06f else 0f, 0.75f)
                    val strokeCol = color.copy(alpha = alpha)
                    val width = (1.5f + 8f * frac).dp.toPx()
                    val x1 = bookX(pair.a)
                    val x2 = bookX(pair.b)
                    if (pair.a == pair.b) {
                        drawCircle(
                            strokeCol, radius = 8.dp.toPx(), center = Offset(x1, y0 - 8.dp.toPx()),
                            style = Stroke(width = max(1.5.dp.toPx(), width * 0.6f)),
                        )
                    } else {
                        val span = abs(x2 - x1) / max(1f, w)
                        val apex = 24.dp.toPx() + (y0 - 74.dp.toPx()) * span.toDouble().pow(0.75).toFloat()
                        val path = Path().apply {
                            moveTo(x1, y0)
                            cubicTo(x1, y0 - apex, x2, y0 - apex, x2, y0)
                        }
                        drawPath(path, strokeCol, style = Stroke(width = width))
                    }
                }

                // Legend.
                paint.textSize = 11.sp.toPx()
                fun legendDot(x: Float, c: Color, label: String) {
                    drawCircle(c, radius = 4.dp.toPx(), center = Offset(x, 14.dp.toPx()))
                    drawLabel(paint, listOf(label), x + 8.dp.toPx(), 7.dp.toPx(), Paint.Align.LEFT, Color(red = 89, green = 77, blue = 56, alpha = 220).toArgbInt())
                }
                legendDot(12.dp.toPx(), Color(red = 184, green = 145, blue = 61, alpha = 220), "OT ↔ OT")
                legendDot(92.dp.toPx(), Color(red = 77, green = 135, blue = 199, alpha = 220), "NT ↔ NT")
                legendDot(172.dp.toPx(), Color(red = 148, green = 97, blue = 179, alpha = 220), "OT ↔ NT")
                drawLabel(
                    paint, listOf("heavier = more links · tap a book to open it"),
                    262.dp.toPx(), 7.dp.toPx(), Paint.Align.LEFT,
                    Color(red = 89, green = 77, blue = 56, alpha = 160).toArgbInt(),
                )
            }
        }
    }
}
