// Which of two taps gets to paint.
//
// Moving the study reads off the main thread (G-03) makes them concurrent, and
// concurrent reads are not ordered: they run on Dispatchers.Default and contend
// for the `synchronized(engine)` monitor, so the tap that started first can win
// the monitor second. Without a turn, two taps in a row leave whichever read
// FINISHED last on screen — the reader taps "Occurrences", changes their mind,
// taps a thread, and gets the concordance a second later.
//
// StudyTurns is the whole rule, and it is deliberately trivial: a counter read
// and written only from the main thread (taps, and continuations that resume on
// the same dispatcher), so it needs no synchronization of its own.

package dev.plumbline

import dev.plumbline.ui.StudyTurns
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StudyTurnsTest {

    @Test
    fun a_read_nothing_interrupted_paints() {
        val turns = StudyTurns()
        val turn = turns.open()
        assertTrue("the only read in flight is the one the reader is waiting for", turns.isCurrent(turn))
    }

    @Test
    fun a_second_tap_takes_the_surface_from_the_first() {
        val turns = StudyTurns()
        val first = turns.open()
        val second = turns.open()
        assertFalse(
            "the first tap's card must not paint over the second's — the reader asked again",
            turns.isCurrent(first),
        )
        assertTrue(turns.isCurrent(second))
    }

    /** The order the reads FINISH in is the thing that is not guaranteed, so the
     *  guard has to hold whichever way round they land. */
    @Test
    fun reads_that_land_out_of_order_still_leave_the_newest_on_screen() {
        val turns = StudyTurns()
        val first = turns.open()
        val second = turns.open()
        val painted = ArrayList<String>()

        // The second read wins the engine monitor and lands first…
        if (turns.isCurrent(second)) painted.add("second")
        // …and the first arrives after it.
        if (turns.isCurrent(first)) painted.add("first")

        assertEquals(listOf("second"), painted)
    }

    /** Ten taps down a concordance list: nine reads are paid for (they are already
     *  in the engine) but only the last one is allowed to repaint the pane. */
    @Test
    fun ten_taps_in_a_row_paint_once() {
        val turns = StudyTurns()
        val opened = (1..10).map { turns.open() }
        assertEquals(
            "only the newest of ten taps may paint",
            listOf(opened.last()),
            opened.filter { turns.isCurrent(it) },
        )
    }

    /** Search's clear button: the field is empty, so a search still in the engine
     *  must not repaint the results the reader just dismissed. */
    @Test
    fun an_abandoned_turn_never_paints() {
        val turns = StudyTurns()
        val turn = turns.open()
        turns.abandon()
        assertFalse("a cleared field must not be repainted by the search it cleared", turns.isCurrent(turn))
    }

    /** Abandoning is not a permanent off switch — the next search paints. */
    @Test
    fun a_search_after_an_abandoned_one_still_paints() {
        val turns = StudyTurns()
        turns.open()
        turns.abandon()
        val next = turns.open()
        assertTrue(turns.isCurrent(next))
    }

    /** A paint is not a one-shot claim: the current turn stays current, because a
     *  caller may ask twice (the spinner, then the card). */
    @Test
    fun asking_twice_about_the_current_turn_answers_the_same() {
        val turns = StudyTurns()
        val turn = turns.open()
        assertTrue(turns.isCurrent(turn))
        assertTrue(turns.isCurrent(turn))
    }
}
