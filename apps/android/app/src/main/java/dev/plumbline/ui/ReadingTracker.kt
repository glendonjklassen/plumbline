// The reading map's shell half: how long the reader actually spent in a chapter.
//
// The core (crates/core/src/reading.rs) owns what "read" MEANS — the word counts,
// the reading rate, the 90% bar, the glow curve. This file owns only the one
// thing the core cannot know, having no clock and no window: how many seconds a
// chapter was genuinely in front of somebody. It hands those seconds over on a
// slow tick and forgets them.
//
// Three refusals, and they are the whole design:
//
//   * A GRACE period before anything accrues, so paging through a book to find
//     something never credits the chapters it flies past.
//   * An IDLE cutoff, so a phone left face-up on a table does not read Leviticus
//     overnight.
//   * PAUSE stops the clock, because a backgrounded app is not being read — and
//     banks what it had on the way out, since locking a phone is how a reading
//     session usually ends.
//
// All three thresholds come from the core's spec over the ABI rather than being
// written down here, so the phone and the browser cannot drift on them.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.plumbline.ReadingRecorded
import dev.plumbline.StudyEngine
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** How often the tracker wakes to add up time. Short enough that the grace and
 *  idle thresholds land accurately; the REPORT to the engine is far rarer. */
private const val SAMPLE_MS = 1_000L

/**
 * Track reading time in [book] [chapter] and report it to the engine.
 *
 * [reachedVerse] is the deepest verse the reader has scrolled to (from
 * `ReaderPane.onVerseReached`) — the core pairs it with the dwell to work out
 * coverage, and needs both: time without scrolling credits only what was on
 * screen, and scrolling without time credits nothing.
 *
 * [interactionEpoch] should change on any scroll, tap or key — it is what tells
 * idle from present. [onCompleted] fires when a pass carries the chapter over
 * the bar, so the shell can say so once rather than on every tick.
 */
@Composable
fun ReadingTracker(
    engine: StudyEngine,
    book: String,
    chapter: Int,
    reachedVerse: Int,
    interactionEpoch: Int,
    enabled: Boolean = true,
    onCompleted: (ReadingRecorded) -> Unit = {},
) {
    // The core's tuning. Fetched once; the defaults in ReadingSpec stand in for
    // the moment before it lands, so the very first seconds of a session are
    // still measured against something sane.
    var grace by remember { mutableStateOf(3f) }
    var idle by remember { mutableStateOf(120f) }
    var tick by remember { mutableStateOf(30f) }
    LaunchedEffect(Unit) {
        val spec = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.ReadingBooksJson(nowUtc()) } }.getOrNull()
                ?.let { runCatching { parseWire<dev.plumbline.ReadingBooks>(it).spec }.getOrNull() }
        }
        if (spec != null) {
            grace = spec.graceSeconds; idle = spec.idleSeconds; tick = spec.tickSeconds
        }
    }

    val reached = rememberUpdatedState(reachedVerse)
    val onDone = rememberUpdatedState(onCompleted)
    val scope = rememberCoroutineScope()

    // Seconds banked but not yet handed over, and the seconds-since-interaction
    // that decides whether the reader is still here. Both live across
    // recomposition but reset per chapter — a new chapter is a new pass.
    var pending by remember(book, chapter) { mutableStateOf(0f) }
    var onScreen by remember(book, chapter) { mutableStateOf(0f) }
    var sinceInput by remember(book, chapter) { mutableStateOf(0f) }
    var paused by remember { mutableStateOf(false) }

    // Any interaction wakes accrual back up.
    LaunchedEffect(interactionEpoch) { sinceInput = 0f }

    /** Hand the banked seconds to the engine. Called on the tick, and on the way
     *  out of a chapter or the app — the tail of a session is real reading and
     *  should not be thrown away because it fell between ticks. */
    suspend fun flush() {
        val secs = pending
        val verse = reached.value
        if (secs <= 0f) return
        pending = 0f
        val out = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) {
                    engine.ReadingRecordJson(book, chapter, verse, secs, nowUtc())
                }
            }.getOrNull()?.let { runCatching { parseWire<ReadingRecorded>(it) }.getOrNull() }
        }
        if (out?.completed == true) onDone.value(out)
    }

    // Stop the clock while backgrounded, and bank what we have on the way out:
    // ON_PAUSE is the last moment anything is guaranteed to run.
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner) {
        val obs = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_PAUSE -> {
                    paused = true
                    // Bank the tail. Backgrounding is how a phone reading session
                    // usually ends, and ON_PAUSE is the last moment anything is
                    // guaranteed to run — without this, up to a whole tick of real
                    // reading is thrown away every time someone locks their phone.
                    // (Web twin: the visibilitychange/pagehide flush.)
                    scope.launch { flush() }
                }
                Lifecycle.Event.ON_RESUME -> {
                    paused = false
                    // Coming back is not continuing: re-serve the grace period so
                    // a glance at a notification and back doesn't bank time, and
                    // treat it as interaction so we aren't instantly idle.
                    onScreen = 0f
                    sinceInput = 0f
                }
                else -> Unit
            }
        }
        owner.lifecycle.addObserver(obs)
        onDispose { owner.lifecycle.removeObserver(obs) }
    }

    LaunchedEffect(book, chapter, enabled) {
        if (!enabled) return@LaunchedEffect
        try {
            while (true) {
                delay(SAMPLE_MS)
                if (paused) continue
                val step = SAMPLE_MS / 1000f
                onScreen += step
                sinceInput += step
                // Grace first, then presence. Neither is a punishment: both exist
                // so that time nobody spent reading never becomes progress.
                if (onScreen < grace) continue
                if (sinceInput > idle) continue
                pending += step
                if (pending >= tick) flush()
            }
        } finally {
            // Leaving the chapter (or the composition) — bank the tail.
            withContext(kotlinx.coroutines.NonCancellable) { flush() }
        }
    }
}
