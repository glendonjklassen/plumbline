// What a Settings slider costs while the finger is down.
//
// Text size, margin and line spacing are all LAYOUT INPUTS to the reading pane,
// so every value the dialog pushed up re-laid the chapter — a native display list
// per tick, ~120 of them for a two-second drag, all but the last orphaned the
// moment the next value arrived. SliderDraft keeps the thumb live and hands the
// value up once, when the drag ends.
//
// Two invariants, and they pull against each other: the thumb must move on every
// tick (a slider that lags the finger is broken), and the value must leave the
// dialog once.

package dev.plumbline

import dev.plumbline.ui.SliderDraft
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SliderDraftTest {

    @Test
    fun a_two_second_drag_moves_the_thumb_every_tick_and_commits_once() {
        val pushed = ArrayList<Float>()
        val draft = SliderDraft(18f)
        var last = 18f

        for (tick in 1..TICKS) {
            last = 18f + tick * 0.1f
            draft.drag(last)
            assertEquals("the thumb must follow the finger on tick $tick", last, draft.value, 0f)
            assertTrue(
                "tick $tick reached the reader's layout mid-drag: $pushed",
                pushed.isEmpty(),
            )
        }

        draft.commit { pushed.add(it) }
        assertEquals("exactly one value may leave a drag", listOf(last), pushed)
    }

    /** onValueChangeFinished fires, and then the dialog closes and flushes. The
     *  reader must not pay for the same value twice. */
    @Test
    fun committing_twice_pushes_once() {
        val pushed = ArrayList<Float>()
        val draft = SliderDraft(1.35f)
        draft.drag(1.6f)
        draft.commit { pushed.add(it) }
        draft.commit { pushed.add(it) }
        assertEquals(listOf(1.6f), pushed)
    }

    /** The other half of that pairing: a thumb the platform never told us was
     *  released (an accessibility set-progress, a dialog dismissed mid-drag) must
     *  still reach the reader's settings. */
    @Test
    fun a_drag_never_finished_still_commits() {
        val pushed = ArrayList<Float>()
        val draft = SliderDraft(28f)
        draft.drag(40f)
        draft.commit { pushed.add(it) }
        assertEquals(listOf(40f), pushed)
    }

    @Test
    fun an_untouched_slider_commits_nothing() {
        val pushed = ArrayList<Float>()
        SliderDraft(18f).commit { pushed.add(it) }
        assertTrue("a dialog opened and closed must not re-lay the chapter", pushed.isEmpty())
    }

    /** Dragged away and back: the value is what it was, so there is nothing to
     *  hand up and nothing to lay out. */
    @Test
    fun a_drag_that_ends_where_it_started_commits_nothing() {
        val pushed = ArrayList<Float>()
        val draft = SliderDraft(18f)
        draft.drag(30f)
        draft.drag(24f)
        draft.drag(18f)
        draft.commit { pushed.add(it) }
        assertTrue("nothing changed, so nothing may be pushed: $pushed", pushed.isEmpty())
        assertEquals(18f, draft.value, 0f)
    }

    /** A second drag after a committed one is its own commit. */
    @Test
    fun each_drag_commits_its_own_value() {
        val pushed = ArrayList<Float>()
        val draft = SliderDraft(18f)
        draft.drag(20f)
        draft.commit { pushed.add(it) }
        draft.drag(22f)
        draft.commit { pushed.add(it) }
        assertEquals(listOf(20f, 22f), pushed)
    }

    private companion object {
        /** 60 fps for two seconds — the drag the punch list measured. */
        private const val TICKS = 120
    }
}
