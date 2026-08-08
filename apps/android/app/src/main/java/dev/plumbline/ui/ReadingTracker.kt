// The reading map's shell half: how long the reader actually spent in a chapter.
//
// The core (crates/core/src/reading.rs) owns what "read" MEANS — the word counts,
// the reading rate, the 90% bar, the glow curve — and the COUNTING as well:
// `reading::DwellTracker` holds the grace period, the idle cutoff, the
// tail-banking and the report cadence. This file owns only the one thing the
// core cannot know, having no clock and no window: that another second passed
// with a chapter genuinely in front of somebody.
//
// The arithmetic lives in the core, shared with the web twin
// (state/readingTracker.ts); no threshold lives in this file to go stale.
//
// Three refusals still shape it, and they are the whole design; they just live in
// the core:
//
//   * A GRACE period before anything accrues, so paging through a book to find
//     something never credits the chapters it flies past.
//   * An IDLE cutoff, so a phone left face-up on a table does not read Leviticus
//     overnight.
//   * PAUSE stops the clock, because a backgrounded app is not being read — and
//     banks what it had on the way out, since locking a phone is how a reading
//     session usually ends.
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

/** How often the tracker wakes and tells the core another second went by. The
 *  core clamps what one sample may credit, and decides on its own cadence when
 *  a report is worth writing down. */
private const val SAMPLE_MS = 1_000L

/** [SAMPLE_MS] in the seconds the ABI takes. */
private const val STEP_SECONDS = SAMPLE_MS / 1000f

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
    val reached = rememberUpdatedState(reachedVerse)
    val onDone = rememberUpdatedState(onCompleted)
    val scope = rememberCoroutineScope()

    // Set by any interaction, cleared by the sample that carries it over. The
    // core wants "did anything happen since the last sample", not a timestamp.
    var touched by remember { mutableStateOf(false) }
    LaunchedEffect(interactionEpoch) { touched = true }
    var paused by remember { mutableStateOf(false) }

    /** One sample. A null [target] means nothing is being read, which is how the
     *  core is told to bank the tail and serve the grace period again. */
    suspend fun sample(target: String?, step: Float) {
        val wasTouched = touched
        touched = false
        val out = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) {
                    engine.ReadingTickJson(target, chapter, reached.value, step, wasTouched, nowUtc())
                }
            }.getOrNull()?.let { runCatching { parseWire<ReadingRecorded>(it) }.getOrNull() }
        }
        if (out?.completed == true) onDone.value(out)
    }

    // Stop the clock while backgrounded, and bank what we have on the way out:
    // ON_PAUSE is the last moment anything is guaranteed to run. Backgrounding is
    // how a phone reading session usually ends, and without this up to a whole
    // report's worth of real reading is thrown away every time someone locks
    // their phone. (Web twin: the visibilitychange/pagehide flush.)
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner) {
        val obs = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_PAUSE -> {
                    paused = true
                    scope.launch { sample(null, 0f) }
                }
                // Coming back is not continuing, but the core knows that: the
                // null sample above cleared the counters, so the next sample with
                // a chapter in it serves the grace period again.
                Lifecycle.Event.ON_RESUME -> paused = false
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
                sample(book, STEP_SECONDS)
            }
        } finally {
            // Leaving the chapter, or the composition — bank the tail.
            withContext(kotlinx.coroutines.NonCancellable) { sample(null, 0f) }
        }
    }
}
