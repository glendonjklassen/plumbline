// Where an engine call runs, and where its answer is painted.
//
// Every StudyEngine call is a blocking native call behind the monitor the two
// reader panes and the study surface share, so one made on the main thread costs
// however long the longest call already running takes. Routing a panel link used
// to make up to TEN in a row on the main thread and searching two — including the
// producers that build a lazy index on first use by folding the whole corpus. (The
// one measured number the tree has for such a fold is 10,205 ms, clocked on the web
// engine worker building the concept map — a feature since removed, 2026-07-30, so
// do not go looking for the call. A wasm worker is not this shell either, but the
// concept fold behind APPEARS ALONGSIDE is the same shape of work.)
//
// `engineCall` is the single mechanism that fixes both, so this exercises it for
// real: a coroutine scope whose dispatcher is the runBlocking event loop stands in
// for the main thread, which is faithful enough for the three things that matter —
// the call leaves the caller's thread, the answer comes back to it, and the engine
// monitor does not travel with it.

package dev.plumbline

import dev.plumbline.ui.StudyTurns
import dev.plumbline.ui.engineCall
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class EngineCallTest {

    @Test
    fun the_call_leaves_the_calling_thread_and_the_paint_comes_back_to_it() = onTheMainThread {
        val engine = Any()
        val caller = Thread.currentThread()
        var ran: Thread? = null
        var painted: Thread? = null
        val done = CompletableDeferred<Unit>()

        engineCall(engine, StudyTurns(), { ran = Thread.currentThread(); "a study card" }) {
            painted = Thread.currentThread()
            done.complete(Unit)
        }
        done.await()

        assertNotSame("a blocking engine call may not run on the thread that asked for it", caller, ran)
        assertSame("the answer has to be painted on the thread that owns the UI", caller, painted)
    }

    /** The lock is the hazard the move introduces: held across the hop back, a
     *  Default worker would sit on the monitor waiting for a main-thread
     *  continuation while the main thread can be waiting for the same monitor. */
    @Test
    fun the_engine_monitor_is_held_for_the_call_and_released_before_the_paint() = onTheMainThread {
        val engine = Any()
        var heldForTheCall = false
        var heldForThePaint = true
        val done = CompletableDeferred<Unit>()

        engineCall(engine, StudyTurns(), { heldForTheCall = Thread.holdsLock(engine); 1 }) {
            heldForThePaint = Thread.holdsLock(engine)
            done.complete(Unit)
        }
        done.await()

        assertTrue("the call must take the monitor the reader panes also take", heldForTheCall)
        assertFalse("the monitor must not be held across the hop back to the main thread", heldForThePaint)
    }

    /** The tap-tap sequence, exactly as it happens: both ends are the main thread,
     *  so the second tap's turn opens while the first coroutine is still parked in
     *  its withContext. */
    @Test
    fun a_second_tap_while_the_first_is_still_running_paints_only_the_second() = onTheMainThread {
        val engine = Any()
        val turns = StudyTurns()
        val painted = ArrayList<String>()
        val done = CompletableDeferred<Unit>()

        engineCall(engine, turns, { "the first tap's card" }) { painted.add(it!!) }
        engineCall(engine, turns, { "the second tap's card" }) { painted.add(it!!); done.complete(Unit) }
        done.await()

        assertEquals("a superseded read may not repaint the pane it lost", listOf("the second tap's card"), painted)
    }

    /** Search's clear button, then a fresh query: the results in flight belong to a
     *  field that no longer holds them. (The precise guard on `abandon()` is
     *  StudyTurnsTest — the later search here is only something to wait on, since a
     *  turn once passed can never come back round.) */
    @Test
    fun a_read_the_reader_abandoned_does_not_paint_over_the_search_after_it() = onTheMainThread {
        val engine = Any()
        val turns = StudyTurns()
        val painted = ArrayList<String>()
        val done = CompletableDeferred<Unit>()

        engineCall(engine, turns, { "hits for a query the reader cleared" }) { painted.add(it!!) }
        turns.abandon()
        engineCall(engine, turns, { "the search after it" }) { painted.add(it!!); done.complete(Unit) }
        done.await()

        assertEquals(listOf("the search after it"), painted)
    }

    /** A write — the reader's own note — is not in the contest for the study
     *  surface, so its repaint must land even though a study read superseded it. */
    @Test
    fun work_with_no_turn_paints_even_after_a_newer_read_starts() = onTheMainThread {
        val engine = Any()
        val turns = StudyTurns()
        var noteEpoch = 0
        val done = CompletableDeferred<Unit>()

        engineCall(engine, null, { "note written" }) { noteEpoch++; done.complete(Unit) }
        turns.open()    // the reader taps a study link while the write is in flight
        done.await()

        assertEquals("the note marks must refresh whatever else the reader tapped", 1, noteEpoch)
    }

    /** A read that produced nothing still paints, because the caller's spinner is
     *  the thing that has to come down. */
    @Test
    fun a_call_that_returns_nothing_still_paints_so_the_spinner_can_drop() = onTheMainThread {
        val engine = Any()
        var loading = true
        var blocks: String? = "the card that was already there"
        val done = CompletableDeferred<Unit>()

        engineCall(engine, StudyTurns(), { null as String? }) { answer ->
            loading = false
            if (answer != null) blocks = answer
            done.complete(Unit)
        }
        done.await()

        assertFalse("a read that answered nothing must not leave the pane loading forever", loading)
        assertEquals("and it must not blank the card that was on screen", "the card that was already there", blocks)
    }

    /** A throwing engine call is the same case as an empty one: the shell has
     *  always swallowed these (runCatching at every call site), and it must not
     *  take the coroutine — or the spinner — down with it. */
    @Test
    fun a_call_that_throws_paints_nothing_and_does_not_escape() = onTheMainThread {
        val engine = Any()
        var loading = true
        var answered: String? = "untouched"
        val done = CompletableDeferred<Unit>()

        engineCall<String>(engine, StudyTurns(), { error("the engine handle was closed") }) { a ->
            loading = false
            if (a != null) answered = a
            done.complete(Unit)
        }
        done.await()

        assertFalse(loading)
        assertEquals("untouched", answered)
    }

    /**
     * A test body on a scope that stands in for the main thread: a coroutine scope
     * whose dispatcher is the runBlocking event loop, so a `launch` runs on THIS
     * thread and a `withContext(Dispatchers.Default)` genuinely leaves it.
     *
     * The ceiling is not decoration. Every test here waits on a paint, and a paint
     * that never comes is exactly the regression they are written for — left to
     * `await()` alone it hangs the whole Gradle run instead of failing, which is
     * what it did while these were being mutation-tested.
     */
    private fun onTheMainThread(body: suspend CoroutineScope.() -> Unit) = runBlocking {
        withTimeout(PAINT_CEILING_MS) { body() }
    }

    private companion object {
        /** Generous: the work under test is a lambda returning a constant, so any
         *  wait longer than a dispatch is a paint that is never coming. */
        private const val PAINT_CEILING_MS = 10_000L
    }
}
