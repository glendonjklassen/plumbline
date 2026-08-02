// The memorization UI (Tier 2 #15): the SM-2 review/drill flow, the canon
// coverage map, and the activity heatmap — the Android/Compose mirror of the GTK
// shell's show_memorize / draw_mem_coverage / draw_mem_activity (apps/desktop
// M:3117, M:3355, M:3435) and the WinUI Memorize.cs. All study logic lives across
// the ABI (StudyEngine.Memory*); this file is orchestration + paint only.
//
// The hub [MemorizeList] carries the coverage strip INLINE above the verse list
// (product call, 2026-07-24 — coverage is a section, not a screen), with
// ReviewDue and Activity as the two full-screen destinations, dismissed back to
// the hub via [MemFrame]'s BackHandler + onClose. Activity is a half/half split:
// calendar heatmap over a most-recent-first history log.
//
// Author D (Compose UI).

package dev.plumbline.ui

import android.graphics.Paint
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.CanonSegments
import dev.plumbline.DayActivity
import dev.plumbline.MemoryActivity
import dev.plumbline.MemoryCoverage
import dev.plumbline.MemoryDrill
import dev.plumbline.MemoryDue
import dev.plumbline.RecallScore
import dev.plumbline.StudyEngine
import dev.plumbline.TocBook
import dev.plumbline.VerseData
import dev.plumbline.parseWire
import java.time.Instant
import java.time.LocalDate
import java.time.format.TextStyle
import java.time.temporal.ChronoUnit
import java.util.Locale
import kotlin.math.roundToInt

// ── shared helpers ───────────────────────────────────────────────────────────

/** The RFC3339 UTC "now" the engine's SRS clock wants (minSdk 26 → java.time). */
fun nowUtc(): String = Instant.now().toString()

/** Start memorizing a verse — seeds its SRS card (due now) if absent. Null =
 *  success, else an error message. The long-press "Memorize this verse" action
 *  (see the wiring spec) routes here, matching GTK's memorize_verse. */
fun memorizeVerse(engine: StudyEngine, verseRef: String): String? =
    synchronized(engine) { engine.MemoryAdd(verseRef, nowUtc()) }

/** The memorization destinations: the hub [List] (verse list + inline coverage
 *  strip — product call, 2026-07-24: coverage is a section of the hub, not a
 *  screen that replaces it), the [ReviewDue] drill, and [Activity]. */
enum class MemorizeView { ReviewDue, List, Activity }

/**
 * The single entry point StudyScreen mounts full-screen while a memorization view
 * is open. [books] is the TOC (66 books, canon order) — [MemorizeView.Coverage]
 * and [MemorizeView.List] use it. [onOpen] jumps the reader to a verse (the list
 * taps through); [onClose] restores the reader (StudyScreen clears its memorize
 * state).
 */
@Composable
fun MemorizeScreen(
    engine: StudyEngine,
    view: MemorizeView,
    books: List<TocBook>,
    palette: ReaderPalette,
    onSelectView: (MemorizeView) -> Unit = {},
    onDrill: (ref: String) -> Unit = {},
    onClose: () -> Unit,
) {
    when (view) {
        MemorizeView.ReviewDue -> MemorizeReview(engine, palette, onClose)
        MemorizeView.List -> MemorizeList(engine, books, palette, onSelectView, onDrill, onClose)
        MemorizeView.Activity -> MemorizeActivity(engine, palette, onClose)
    }
}

/** The memorization hub (the single "Memorize" menu entry): a list of every
 *  verse the reader is memorizing (tap to open it), with buttons up top to Review
 *  due / Coverage map / Activity. Built from `MemoryCoverageJson` (all cards,
 *  canon-sorted) — no new core endpoint. */
@Composable
fun MemorizeList(
    engine: StudyEngine,
    books: List<TocBook>,
    palette: ReaderPalette,
    onSelectView: (MemorizeView) -> Unit,
    onDrill: (ref: String) -> Unit,
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
    val nameOf = remember(books) { books.associate { it.id to it.name } }
    // The LIST is per card — a passage card is one row labelled "Ps 23:1–6".
    // The coverage strip below keeps using `verses`, which a passage card
    // contributes every verse to (2026-07-27).
    val cards = coverage?.cards ?: emptyList()
    val verses = coverage?.verses ?: emptyList()
    val dueCount = cards.count { it.due }

    MemFrame("Memorize", palette, onClose) {
        Column(Modifier.fillMaxSize()) {
            // Actions: Review due (with a count) and Activity.
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Button(
                    onClick = { onSelectView(MemorizeView.ReviewDue) },
                    enabled = dueCount > 0,
                    colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
                ) { Text(if (dueCount > 0) "Review $dueCount due" else "Nothing due") }
                OutlinedButton(
                    onClick = { onSelectView(MemorizeView.Activity) },
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = palette.ink),
                ) { Text("Activity") }
            }
            HorizontalDivider(color = palette.rule)

            // Coverage lives INLINE with the verse list it summarises — a strip,
            // not a screen (fullscreen felt like leaving the hub).
            if (verses.isNotEmpty()) {
                Text(
                    "Coverage — the canon shaded by mastery",
                    color = palette.faded, fontSize = 12.sp,
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                )
                CoverageCanvas(
                    books, coverage, segments, palette,
                    Modifier.fillMaxWidth().height(120.dp).padding(horizontal = 12.dp),
                )
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(top = 8.dp))
            }

            if (cards.isEmpty()) {
                Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                    Text(
                        "No verses yet.\n\nLong-press a verse → “Memorize this verse”, or " +
                            "“Memorize passage…” for a whole section.",
                        color = palette.ink, fontSize = 16.sp,
                    )
                }
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    item {
                        val nv = cards.sumOf { it.verses }
                        Text(
                            "${cards.size} card${if (cards.size == 1) "" else "s"} · " +
                                "$nv verse${if (nv == 1) "" else "s"} · tap to drill",
                            color = palette.faded, fontSize = 12.sp,
                            modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp),
                        )
                    }
                    items(cards) { v ->
                        // The label carries the range ("Ps 23:1–6"); swap only the
                        // OSIS book id for its display name.
                        val shown = v.label.ifBlank { v.ref }
                        val sp = shown.lastIndexOf(' ')
                        val bookId = if (sp > 0) shown.substring(0, sp) else shown
                        val chv = if (sp > 0) shown.substring(sp + 1) else ""
                        Row(
                            Modifier.fillMaxWidth()
                                .clickable { onDrill(v.ref) }
                                .padding(horizontal = 20.dp, vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column(Modifier.weight(1f)) {
                                Text("${nameOf[bookId] ?: bookId} $chv", color = palette.ink, fontSize = 16.sp)
                                Text(
                                    masteryLabel(v.mastery) + " · " + v.reps + " review" + (if (v.reps == 1) "" else "s"),
                                    color = palette.faded, fontSize = 12.sp,
                                )
                            }
                            if (v.due) {
                                Text("due", color = palette.gold, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                            }
                        }
                        HorizontalDivider(color = palette.rule)
                    }
                }
            }
        }
    }
}

/** A mastery token → a reader-facing label. */
private fun masteryLabel(m: String): String = when (m) {
    "new" -> "New"
    "learning" -> "Learning"
    "young" -> "Young"
    "mature" -> "Mature"
    else -> m.replaceFirstChar { it.uppercase() }
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
        ScreenBar(title, palette, onClose)
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
fun MemorizeReview(engine: StudyEngine, palette: ReaderPalette, onClose: () -> Unit, only: String? = null) {
    // [only] drills a single chosen verse (the hub taps one directly); otherwise
    // snapshot the due queue once at open (grading reschedules, but this session
    // works the queue as it stood on entry).
    val due = remember(only) {
        if (only != null) {
            listOf(only)
        } else {
            runCatching {
                synchronized(engine) { engine.MemoryDueJson(nowUtc()) }
                    ?.let { parseWire<MemoryDue>(it).refs }
            }.getOrNull() ?: emptyList()
        }
    }

    MemFrame(if (only != null) "Memorize verse" else "Memorize", palette, onClose) {
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
    // A grade the engine refused. Shown by the grade buttons and cleared when the
    // next card comes up.
    var gradeError by remember { mutableStateOf("") }

    // idx never runs past the end: advance() calls onFinish before it would.
    val curRef = due[idx]

    val verseDisplay = remember(curRef) {
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
    // A passage card is titled by its range ("Psalm 23:1–6"): take the drill's
    // label and swap its OSIS id for the display name the verse lookup gives.
    val refDisplay = remember(curRef, drill?.label, verseDisplay) {
        val label = drill?.label.orEmpty()
        val dash = label.indexOf('–')
        if (dash < 0 || !label.startsWith(curRef)) verseDisplay
        else verseDisplay + label.substring(dash)
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
        gradeError = ""
        mode = Prompt.FirstLetters
        level = 0
        if (idx + 1 >= due.size) onFinish() else idx += 1
    }

    /** Grade and move on — but only on a grade the engine recorded. A refused write
     *  used to advance anyway, so the reader believed they had rescheduled a card
     *  that in fact never moved, and the next card buried the reason. */
    fun grade(g: String) {
        val outcome = saveOutcome(runCatching { synchronized(engine) { engine.MemoryGrade(curRef, g, nowUtc()) } })
        when (outcome) {
            is SaveOutcome.Saved -> advance()
            is SaveOutcome.Failed -> gradeError = "Not saved — ${outcome.message}. This card is still due."
        }
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
        // The whole card scrolls, so typing (which raises the keyboard) never
        // collapses the prompt — the first-letter/blanked hint stays visible
        // above the recall field. imePadding lifts the field clear of the keyboard.
        Modifier.fillMaxSize().imePadding().verticalScroll(rememberScrollState())
            .padding(horizontal = 22.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("Card ${idx + 1} of ${due.size}", color = palette.faded, fontSize = 12.sp)
        Text(refDisplay, color = palette.ink, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)

        // The prompt/hint — natural height (no weight), so it can't be squeezed
        // away when the keyboard opens.
        Text(promptText, color = palette.ink, fontSize = 18.sp, modifier = Modifier.fillMaxWidth())

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

        if (gradeError.isNotEmpty()) {
            Text(gradeError, color = palette.disputed, fontSize = 14.sp)
        }

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

// ── (b) the coverage strip (GTK draw_mem_coverage; inline in the hub) ─────────

/**
 * The canon strip shaded by how much of each book is being memorized and how
 * well (average mastery), OT/NT seam marked, section labels along the top — the
 * dispersion visual language reused for memory work. Rendered inline in the
 * memorize hub above the verse list it summarises. [books] is the TOC (66
 * books, canon order); a verse maps to a book by its ref key.
 */
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

// ── (c) memory activity: calendar heatmap over a history log ──────────────────

/**
 * When the memory work happened, split half-and-half (product call, 2026-07-24):
 * the top half is a calendar heatmap (weeks as columns, GitHub-style, shaded by
 * reviews that day), the bottom half a most-recent-first history log.
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
        if (days.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                Text(
                    "No reviews yet — grade some cards in Review due.",
                    color = palette.ink, fontSize = 15.sp,
                )
            }
        } else {
            Column(Modifier.fillMaxSize()) {
                ActivityCalendar(
                    days, palette,
                    Modifier.fillMaxWidth().weight(1f).padding(horizontal = 14.dp, vertical = 10.dp),
                )
                HorizontalDivider(color = palette.rule)
                LazyColumn(Modifier.fillMaxWidth().weight(1f)) {
                    item {
                        Text(
                            "History",
                            color = palette.faded, fontSize = 12.sp,
                            modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                        )
                    }
                    items(days.asReversed()) { d ->
                        Row(
                            Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(prettyDay(d.day), color = palette.ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                            Text(
                                "${d.reviews} review${if (d.reviews == 1) "" else "s"}",
                                color = palette.gold, fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
                            )
                        }
                        HorizontalDivider(color = palette.rule)
                    }
                }
            }
        }
    }
}

/** "2026-07-21" → "Tue, Jul 21 2026" (falls back to the raw day string). */
private fun prettyDay(day: String): String = runCatching {
    val d = LocalDate.parse(day)
    val dow = d.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault())
    val mon = d.month.getDisplayName(TextStyle.SHORT, Locale.getDefault())
    "$dow, $mon ${d.dayOfMonth} ${d.year}"
}.getOrDefault(day)

/**
 * The calendar heatmap: columns are weeks (Monday-first rows), each day a cell
 * shaded by its review count, month names along the top. Sized to fit — the
 * history log below carries the exact numbers.
 */
@Composable
private fun ActivityCalendar(days: List<DayActivity>, palette: ReaderPalette, modifier: Modifier) {
    val byDay = remember(days) {
        days.mapNotNull { d ->
            runCatching { LocalDate.parse(d.day) }.getOrNull()?.let { it to d.reviews }
        }.toMap()
    }
    val textPaint = remember { Paint(Paint.ANTI_ALIAS_FLAG) }
    Canvas(modifier) {
        drawRect(palette.paper, size = size)
        if (byDay.isEmpty()) return@Canvas

        val first = byDay.keys.min()
        val last = byDay.keys.max()
        val start = first.minusDays(first.dayOfWeek.value - 1L)   // Monday of the first week
        val weeks = (ChronoUnit.DAYS.between(start, last).toInt() / 7) + 1
        val labelH = 14.dp.toPx()
        val cell = minOf((size.width - 4f) / weeks, (size.height - labelH) / 7f)
        val gap = maxOf(1.2f, cell * 0.12f)
        val maxR = maxOf(1, days.maxOf { it.reviews })

        textPaint.textSize = 9.dp.toPx()
        textPaint.color = palette.faded.toArgb()

        var prevMonth = -1
        for (wi in 0 until weeks) {
            val monday = start.plusWeeks(wi.toLong())
            if (monday.monthValue != prevMonth) {
                prevMonth = monday.monthValue
                drawTopText(
                    monday.month.getDisplayName(TextStyle.SHORT, Locale.getDefault()),
                    wi * cell, 0f, textPaint,
                )
            }
            for (di in 0..6) {
                val d = monday.plusDays(di.toLong())
                if (d.isAfter(last)) break
                val r = byDay[d] ?: 0
                val color = if (r > 0) {
                    palette.gold.copy(alpha = 0.20f + 0.75f * r / maxR)
                } else {
                    palette.gold.copy(alpha = 0.06f)
                }
                drawRect(
                    color,
                    topLeft = Offset(wi * cell, labelH + di * cell),
                    size = Size(cell - gap, cell - gap),
                )
            }
        }
    }
}

/** Draw [text] with its visual top at [top] (Paint draws from the baseline, so we
 *  offset by the ascent) — matching the top-anchored move_to of the GTK/Pango and
 *  WinUI/DirectWrite label draws. */
private fun DrawScope.drawTopText(text: String, x: Float, top: Float, paint: Paint) {
    drawContext.canvas.nativeCanvas.drawText(text, x, top - paint.fontMetrics.ascent, paint)
}

// (The zoom/pan modifier the fullscreen coverage/activity views used lives on in
// Maps.kt as ZoomState.zoomable — both views are now inline/fit-to-frame here.)
