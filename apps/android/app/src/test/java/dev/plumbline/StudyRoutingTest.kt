// The two decisions the study reads had to stop making on the main thread.
//
// Both used to be inline in a Compose event handler, interleaved with the blocking
// engine calls that fed them (G-03). Pulling the engine calls onto
// Dispatchers.Default forced the decisions out into functions, which is the only
// reason they can be tested at all here — this module cannot enter composition
// (compose-ui-test is androidTest-only, and there is no Robolectric on the
// unit-test classpath; adding either is a build-file change).

package dev.plumbline

import dev.plumbline.ui.ChapterRef
import dev.plumbline.ui.opensAPassage
import dev.plumbline.ui.weaveOpening
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class StudyRoutingTest {

    // ── which passages a weave tap opens ────────────────────────────────────────

    @Test
    fun a_weave_opens_its_first_resolved_link_not_merely_its_first() {
        val opening = weaveOpening(
            listOf(
                link("Nowhere 1:1", "Nowhere 2:2", resolved = false),
                link("Isa 53:5", "1Pet 2:24", resolved = true),
            ),
        )
        assertEquals("Isa 53:5", opening?.primary)
        assertEquals(ChapterRef("1Pet", 2), opening?.second)
    }

    /** Nothing resolved — a hand-authored weave whose refs the corpus does not
     *  know — still opens something, which is what the reader tapped for. */
    @Test
    fun a_weave_with_nothing_resolved_falls_back_to_its_first_link() {
        val opening = weaveOpening(listOf(link("Gen 1:1", "John 1:1", resolved = false)))
        assertEquals("Gen 1:1", opening?.primary)
        assertEquals(ChapterRef("John", 1), opening?.second)
    }

    @Test
    fun a_weave_with_no_links_opens_nothing() {
        assertNull(weaveOpening(emptyList()))
    }

    /** Both ends in one chapter: the fold's second pane would show the chapter the
     *  reader is already looking at, so it is left where it was. */
    @Test
    fun both_ends_of_a_link_in_one_chapter_leave_the_second_pane_alone() {
        val opening = weaveOpening(listOf(link("John 3:16", "John 3:17", resolved = true)))
        assertEquals("John 3:16", opening?.primary)
        assertNull("two panes on one chapter show the reader nothing new", opening?.second)
    }

    @Test
    fun the_same_book_at_a_different_chapter_does_open_the_second_pane() {
        val opening = weaveOpening(listOf(link("Ps 22:1", "Ps 69:21", resolved = true)))
        assertEquals(ChapterRef("Ps", 69), opening?.second)
    }

    /** A refKey carries a book with a space in it. `lastIndexOf(' ')` is what keeps
     *  the split at the chapter, not at the book's own space. */
    @Test
    fun a_two_word_book_name_still_splits_at_the_chapter() {
        val opening = weaveOpening(listOf(link("Song 1:1", "1 Kings 4:32", resolved = true)))
        assertEquals(ChapterRef("1 Kings", 4), opening?.second)
    }

    /** The `b` end being unusable does not cancel the tap: the reader still goes to
     *  the side the card is about. */
    @Test
    fun an_unparseable_far_end_still_opens_the_near_one() {
        val opening = weaveOpening(listOf(link("Gen 1:1", "wherever", resolved = true)))
        assertEquals("Gen 1:1", opening?.primary)
        assertNull(opening?.second)
    }

    /** The tightening this move made: a link whose OWN end is not a refKey used to
     *  leave the reader's pane where it was and move the second pane to the far
     *  end, which reads as the app ignoring the tap and then moving the wrong pane. */
    @Test
    fun a_link_whose_near_end_is_not_a_reference_opens_nothing() {
        assertNull(weaveOpening(listOf(link("Genesis", "John 1:1", resolved = true))))
        assertNull(weaveOpening(listOf(link("Gen chapter:1", "John 1:1", resolved = true))))
    }

    // ── what a search answer was ────────────────────────────────────────────────

    @Test
    fun a_goto_answer_that_says_where_opens_the_passage() {
        assertTrue(SearchResult(kind = "goto", book = "John", chapter = 3).opensAPassage())
    }

    @Test
    fun a_hits_answer_lists_results_however_many_it_has() {
        assertFalse(SearchResult(kind = "hits", hits = emptyList()).opensAPassage())
    }

    /** The reader's own pane is navigated on the strength of this, through two
     *  `!!`s. A `goto` that does not say where must fall through to the hit list. */
    @Test
    fun a_goto_answer_missing_its_target_does_not_open_a_passage() {
        assertFalse(SearchResult(kind = "goto", book = "John").opensAPassage())
        assertFalse(SearchResult(kind = "goto", chapter = 3).opensAPassage())
    }

    private fun link(a: String, b: String, resolved: Boolean) =
        WeaveLink1(a = a, aDisplay = a, b = b, bDisplay = b, resolved = resolved)
}
