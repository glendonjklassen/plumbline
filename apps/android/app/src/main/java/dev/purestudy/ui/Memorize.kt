// The memorization UI (Tier 2 #15): the SM-2 review/drill flow, the canon
// coverage map, and the activity heatmap — the Android/Compose mirror of the GTK
// shell's show_memorize / draw_mem_coverage / draw_mem_activity (apps/desktop
// M:3117, M:3355, M:3435) and the WinUI Memorize.cs. All study logic lives across
// the ABI (StudyEngine.Memory*); this file is orchestration + paint only.
//
// The three views are full-screen destinations (not overlays): StudyScreen holds
// a nullable MemorizeView and swaps the whole screen for [MemorizeScreen] while a
// view is open, restoring the reader when it is dismissed (system back or the ‹
// button, wired through [MemFrame]'s BackHandler + onClose). The big visuals are
// pinch-zoom + pan (Glendon's decision for dense canvas maps) via the reusable
// [zoomable] modifier below.
//
// Author D (Compose UI).

package dev.purestudy.ui

import android.graphics.Paint
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.purestudy.CanonSegments
import dev.purestudy.DayActivity
import dev.purestudy.MemoryActivity
import dev.purestudy.MemoryCoverage
import dev.purestudy.MemoryDrill
import dev.purestudy.MemoryDue
import dev.purestudy.RecallScore
import dev.purestudy.StudyEngine
import dev.purestudy.TocBook
import dev.purestudy.VerseData
import dev.purestudy.parseWire
import java.time.Instant
import kotlin.math.roundToInt

// ── shared helpers ───────────────────────────────────────────────────────────

/** The RFC3339 UTC "now" the engine's SRS clock wants (minSdk 26 → java.time). */
fun nowUtc(): String = Instant.now().toString()

/** Start memorizing a verse — seeds its SRS card (due now) if absent. Null =
 *  success, else an error message. The long-press "Memorize this verse" action
 *  (see the wiring spec) routes here, matching GTK's memorize_verse. */
fun memorizeVerse(engine: StudyEngine, verseRef: String): String? =
    synchronized(engine) { engine.MemoryAdd(verseRef, nowUtc()) }

/** The three memorization destinations, mirroring GTK's "Memorize ▸" submenu
 *  (Review due / Coverage map / Activity). */
enum class MemorizeView { ReviewDue, Coverage, Activity }

/**
 * The single entry point StudyScreen mounts full-screen while a memorization view
 * is open. [books] is the TOC (66 books, canon order) — only [MemorizeView.Coverage]
 * needs it. [onClose] restores the reader (StudyScreen sets its memorize state to
 * null).
 */
@Composable
fun MemorizeScreen(
    engine: StudyEngine,
    view: MemorizeView,
    books: List<TocBook>,
    palette: ReaderPalette,
    onClose: () -> Unit,
) {
    when (view) {
        MemorizeView.ReviewDue -> MemorizeReview(engine, palette, onClose)
        MemorizeView.Coverage -> MemorizeCoverage(engine, books, palette, onClose)
        MemorizeView.Activity -> MemorizeActivity(engine, palette, onClose)
    }
}

/** A plain, theme-aware full-screen frame: a ‹ back / title bar over [content].
 *  System back and the ‹ button both invoke [onClose] — the Android equivalent of
 *  the WinUI Memorize windows (Esc / chrome closes). */
@Composable
private fun MemFrame(
    title: String,
    palette: ReaderPalette,
    onClose: () -> Unit,
    content: @Composable () -> Unit,
) {
    BackHandler(onBack = onClose)
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        Surface(color = palette.paneNavBg) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                TextButton(onClick = onClose) { Text("‹", fontSize = 20.sp, color = palette.ink) }
                Text(
                    title,
                    color = palette.ink,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        }
        HorizontalDivider(color = palette.rule)
        Box(Modifier.fillMaxWidth().weight(1f)) { content() }
    }
}

// ── (a) the SM-2 review / drill flow (GTK show_memorize) ──────────────────────

/** The prompt modes, mirroring GTK's Prompt enum:
 *  [FirstLetters] = the first-letter skeleton, [Blank] = progressively blanked at
 *  the slider `level`, [Full] = the full verse text (Reveal). */
private enum class Prompt { FirstLetters, Blank, Full }

/**
 * Step the verses due now, drilling each (first-letters · progressive blank-out ·
 * typed recall), then grade with SM-2 (Again/Hard/Good/Easy) and advance —
 * closing when the queue empties.
 */
@Composable
fun MemorizeReview(engine: StudyEngine, palette: ReaderPalette, onClose: () -> Unit) {
    // Snapshot the due queue once at open (like the desktop shells) — grading
    // reschedules, but this session works the queue as it stood on entry.
    val due = remember {
        runCatching {
            synchronized(engine) { engine.MemoryDueJson(nowUtc()) }
                ?.let { parseWire<MemoryDue>(it).refs }
        }.getOrNull() ?: emptyList()
    }

    MemFrame("Memorize", palette, onClose) {
        if (due.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(22.dp), contentAlignment = Alignment.Center) {
                Text(
                    "Nothing due for review.\n\n" +
                        "Long-press a verse → “Memorize this verse” to start a card.",
                    color = palette.ink,
                    fontSize = 16.sp,
                )
            }
        } else {
            ReviewBody(engine, due, palette, onFinish = onClose)
        }
    }
}

@Composable
private fun ReviewBody(
    engine: StudyEngine,
    due: List<String>,
    palette: ReaderPalette,
    onFinish: () -> Unit,
) {
    var idx by remember { mutableStateOf(0) }
    var mode by remember { mutableStateOf(Prompt.FirstLetters) }
    var level by remember { mutableStateOf(0) }
    var typed by remember { mutableStateOf("") }
    var result by remember { mutableStateOf("") }

    // idx never runs past the end: advance() calls onFinish before it would.
    val curRef = due[idx]

    val refDisplay = remember(curRef) {
        runCatching {
            synchronized(engine) { engine.VerseJson(curRef) }
                ?.let { parseWire<VerseData>(it).display }
        }.getOrNull() ?: curRef
    }

    // One drill feeds every mode: it carries FirstLetters/Text and the (level-
    // dependent) Blanked plus the constant MaxLevel. Re-fetched on ref/level.
    val drill: MemoryDrill? = remember(curRef, level) {
        runCatching {
            synchronized(engine) { engine.MemoryDrillJson(curRef, level) }
                ?.let { parseWire<MemoryDrill>(it) }
        }.getOrNull()
    }
    val maxLevel = (drill?.maxLevel ?: 0).takeIf { it > 0 } ?: 4
    val promptText = when (mode) {
        Prompt.Full -> drill?.text ?: ""
        Prompt.Blank -> drill?.blanked ?: ""
        Prompt.FirstLetters -> drill?.firstLetters ?: ""
    }

    // Advance resets to the first-letter prompt (GTK sets Prompt::FirstLetters).
    fun advance() {
        typed = ""
        result = ""
        mode = Prompt.FirstLetters
        level = 0
        if (idx + 1 >= due.size) onFinish() else idx += 1
    }

    fun grade(g: String) {
        runCatching { synchronized(engine) { engine.MemoryGrade(curRef, g, nowUtc()) } }
        advance()
    }

    fun check() {
        val sj = runCatching { synchronized(engine) { engine.MemoryScoreJson(curRef, typed) } }
            .getOrNull() ?: return
        val score = runCatching { parseWire<RecallScore>(sj) }.getOrNull() ?: return
        val pct = (score.accuracy * 100f).roundToInt()
        val missed = score.words.filter { !it.ok }.map { it.word }
        result = if (missed.isEmpty()) "✓ $pct% — perfect"
        else "$pct% — missed: ${missed.joinToString(" ")}"
    }

    Column(
        Modifier.fillMaxSize().padding(horizontal = 22.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Card ${idx + 1} of ${due.size} due", color = palette.faded, fontSize = 12.sp)
        Text(refDisplay, color = palette.ink, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)

        // The prompt — the only element that grows to fill the free space.
        Box(Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState())) {
            Text(promptText, color = palette.ink, fontSize = 18.sp)
        }

        // Prompt-mode controls: first-letters · a blank-out slider · reveal.
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TextButton(onClick = { mode = Prompt.FirstLetters }) {
                Text("First letters", color = palette.ink)
            }
            Slider(
                value = level.toFloat().coerceIn(0f, maxLevel.toFloat()),
                onValueChange = { mode = Prompt.Blank; level = it.roundToInt() },
                valueRange = 0f..maxLevel.toFloat(),
                steps = (maxLevel - 1).coerceAtLeast(0),
                modifier = Modifier.weight(1f),
                colors = SliderDefaults.colors(
                    thumbColor = palette.gold,
                    activeTrackColor = palette.gold,
                ),
            )
            TextButton(onClick = { mode = Prompt.Full }) {
                Text("Reveal", color = palette.gold)
            }
        }

        // Typed recall.
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            OutlinedTextField(
                value = typed,
                onValueChange = { typed = it },
                placeholder = { Text("Type the verse from memory, then Check") },
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = { check() }) { Text("Check", color = palette.ink) }
        }
        if (result.isNotEmpty()) Text(result, color = palette.ink, fontSize = 14.sp)

        // Grade buttons (SM-2's four). Again resets to relearning (destructive
        // tone); Easy carries the strongest accent — GTK's suggested-action.
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            GradeButton("Again", filled = false, accent = palette.disputed, paper = palette.paper) { grade("again") }
            GradeButton("Hard", filled = false, accent = palette.ink, paper = palette.paper) { grade("hard") }
            GradeButton("Good", filled = true, accent = palette.sectionGold, paper = palette.paper) { grade("good") }
            GradeButton("Easy", filled = true, accent = palette.gold, paper = palette.paper) { grade("easy") }
        }
    }
}

@Composable
private fun RowScope.GradeButton(
    label: String,
    filled: Boolean,
    accent: Color,
    paper: Color,
    onClick: () -> Unit,
) {
    if (filled) {
        Button(
            onClick = onClick,
            modifier = Modifier.weight(1f),
            colors = ButtonDefaults.buttonColors(containerColor = accent, contentColor = paper),
        ) { Text(label) }
    } else {
        OutlinedButton(
            onClick = onClick,
            modifier = Modifier.weight(1f),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = accent),
        ) { Text(label) }
    }
}

// ── (b) the coverage map (GTK draw_mem_coverage) ──────────────────────────────

/**
 * The canon strip shaded by how much of each book is being memorized and how well
 * (average mastery), OT/NT seam marked, section labels along the top — the
 * dispersion visual language reused for memory work. Pinch-zoom + pan for a close
 * look at the thin per-book columns. [books] is the TOC (66 books, canon order);
 * a verse maps to a book by its ref key.
 */
@Composable
fun MemorizeCoverage(
    engine: StudyEngine,
    books: List<TocBook>,
    palette: ReaderPalette,
    onClose: () -> Unit,
) {
    val coverage = remember {
        runCatching {
            synchronized(engine) { engine.MemoryCoverageJson(nowUtc()) }
                ?.let { parseWire<MemoryCoverage>(it) }
        }.getOrNull()
    }
    val segments = remember {
        runCatching {
            synchronized(engine) { engine.CanonSegmentsJson() }
                ?.let { parseWire<CanonSegments>(it) }
        }.getOrNull()
    }
    MemFrame("Memory coverage", palette, onClose) {
        CoverageCanvas(
            books, coverage, segments, palette,
            Modifier.fillMaxSize().zoomable(),
        )
    }
}

@Composable
private fun CoverageCanvas(
    books: List<TocBook>,
    coverage: MemoryCoverage?,
    segments: CanonSegments?,
    palette: ReaderPalette,
    modifier: Modifier,
) {
    // Per-book aggregate: card count + summed mastery score → an average shade.
    val byBook = remember(coverage) {
        val acc = HashMap<String, Pair<Int, Double>>()
        coverage?.verses?.forEach { v ->
            val book = bookOf(v.ref) ?: return@forEach
            val sc = when (v.mastery) {
                "new" -> 0.15
                "learning" -> 0.40
                "young" -> 0.70
                "mature" -> 1.0
                else -> 0.15
            }
            val cur = acc[book] ?: (0 to 0.0)
            acc[book] = (cur.first + 1) to (cur.second + sc)
        }
        acc
    }
    val labelPaint = remember { Paint(Paint.ANTI_ALIAS_FLAG) }

    Canvas(modifier) {
        val w = size.width
        val h = size.height
        drawRect(palette.paper, size = size)
        if (w < 10f || books.isEmpty()) return@Canvas

        val nb = books.size.toFloat()
        val top = 26f
        for (i in books.indices) {
            val x0 = i / nb * w
            val x1 = (i + 1) / nb * w
            val bb = byBook[books[i].id]
            val alpha = if (bb != null) {
                (0.2 + 0.75 * (bb.second / maxOf(1, bb.first))).toFloat()
            } else {
                0.05f
            }
            drawRect(
                color = palette.gold.copy(alpha = alpha.coerceIn(0f, 1f)),
                topLeft = Offset(x0, top),
                size = Size(maxOf(0.5f, x1 - x0 - 0.5f), h - top),
            )
        }

        // OT/NT seam.
        val divide = segments?.otNtDivide ?: 39
        val dx = divide / nb * w
        drawRect(
            color = palette.gold.copy(alpha = 0.9f),
            topLeft = Offset(dx - 0.75f, 0f),
            size = Size(1.5f, h),
        )

        // Section labels along the top.
        labelPaint.textSize = 11.dp.toPx()
        labelPaint.color = palette.faded.toArgb()
        segments?.segments?.forEach { seg ->
            val mid = (seg.first + seg.last + 1) / 2f / nb * w
            val tw = labelPaint.measureText(seg.label)
            drawTopText(seg.label, maxOf(1f, mid - tw / 2f), 6f, labelPaint)
        }
    }
}

/** The book id (OSIS) of a compact ref key ("Gen 1:7" → "Gen", "1Cor 13:4" →
 *  "1Cor"). Book ids never contain spaces, so the last space bounds the book —
 *  the same split as core's VRef::parse_ref_key. */
private fun bookOf(refKey: String): String? {
    val i = refKey.lastIndexOf(' ')
    return if (i > 0) refKey.substring(0, i) else null
}

// ── (c) the activity heatmap (GTK draw_mem_activity) ──────────────────────────

/**
 * Reviews per calendar day, oldest → newest, as gold columns with the first and
 * last day labelled — a glance at when the memory work happened. Pinch-zoom + pan
 * for a long history.
 */
@Composable
fun MemorizeActivity(engine: StudyEngine, palette: ReaderPalette, onClose: () -> Unit) {
    val days = remember {
        runCatching {
            synchronized(engine) { engine.MemoryActivityJson() }
                ?.let { parseWire<MemoryActivity>(it).days }
        }.getOrNull() ?: emptyList()
    }
    MemFrame("Memory activity", palette, onClose) {
        ActivityCanvas(days, palette, Modifier.fillMaxSize().zoomable())
    }
}

@Composable
private fun ActivityCanvas(days: List<DayActivity>, palette: ReaderPalette, modifier: Modifier) {
    val textPaint = remember { Paint(Paint.ANTI_ALIAS_FLAG) }
    Canvas(modifier) {
        val w = size.width
        val h = size.height
        drawRect(palette.paper, size = size)

        if (days.isEmpty()) {
            textPaint.textSize = 13.dp.toPx()
            textPaint.color = palette.faded.toArgb()
            drawTopText(
                "No reviews yet — grade some cards in Review due.",
                24f, h / 2f - 8f, textPaint,
            )
            return@Canvas
        }

        val max = maxOf(1, days.maxOf { it.reviews })
        val n = days.size.toFloat()
        val baseline = h - 28f
        val plotH = baseline - 24f
        val gap = (w - 48f) / n
        val barW = maxOf(minOf(gap, 28f), 2f) - 2f
        for (i in days.indices) {
            val x = 24f + i * gap
            val bh = days[i].reviews.toFloat() / max * plotH
            drawRect(
                color = palette.gold.copy(alpha = 0.85f),
                topLeft = Offset(x, baseline - bh),
                size = Size(maxOf(2f, barW), bh),
            )
        }

        // First + last day labels.
        textPaint.textSize = 10.dp.toPx()
        textPaint.color = palette.faded.toArgb()
        drawTopText(days.first().day, 24f, baseline + 6f, textPaint)
        if (days.size > 1) {
            val tw = textPaint.measureText(days.last().day)
            drawTopText(days.last().day, w - 24f - tw, baseline + 6f, textPaint)
        }
    }
}

/** Draw [text] with its visual top at [top] (Paint draws from the baseline, so we
 *  offset by the ascent) — matching the top-anchored move_to of the GTK/Pango and
 *  WinUI/DirectWrite label draws. */
private fun DrawScope.drawTopText(text: String, x: Float, top: Float, paint: Paint) {
    drawContext.canvas.nativeCanvas.drawText(text, x, top - paint.fontMetrics.ascent, paint)
}

// ── zoom + pan ────────────────────────────────────────────────────────────────

/**
 * Pinch-to-zoom + drag-to-pan for a big canvas visual (Glendon's decision for the
 * dense maps). Applies a [graphicsLayer] scale/translation driven by
 * [detectTransformGestures]; panning is disabled (offset reset) at 1× so the map
 * always springs back to fit.
 */
fun Modifier.zoomable(minScale: Float = 1f, maxScale: Float = 6f): Modifier = composed {
    var scale by remember { mutableStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    graphicsLayer {
        scaleX = scale
        scaleY = scale
        translationX = offset.x
        translationY = offset.y
    }.pointerInput(Unit) {
        detectTransformGestures { _, pan, zoom, _ ->
            scale = (scale * zoom).coerceIn(minScale, maxScale)
            offset = if (scale <= 1f) Offset.Zero else offset + pan
        }
    }
}
