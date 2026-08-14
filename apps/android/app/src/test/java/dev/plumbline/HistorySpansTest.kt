package dev.plumbline

import dev.plumbline.ui.historySpans
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The reading-history grouping, which exists in BOTH shells (the web twin is
 * `apps/web/src/shell/historySpans.ts`) because the shells own this list —
 * `config.history` is prepended locally on every navigation and only reaches
 * the engine on a debounced save, so a core-derived version would be stale the
 * moment the reader turned a page.
 *
 * Two implementations means the rules have to be written down somewhere both
 * can be checked against. That is this file; the web's half is driven through
 * the history sheet in `e2e/history.spec.ts`.
 */
class HistorySpansTest {
    private fun spans(vararg entries: Pair<String, Int>) =
        historySpans(entries.toList()).map { it.label(it.book) to it.open }

    @Test
    fun `a run of contiguous chapters collapses to one line`() {
        // Read Gen 1, then 2, then 3 — which lands in a most-recent-first list
        // as 3, 2, 1.
        assertEquals(
            listOf("Gen 1–3" to 3),
            spans("Gen" to 3, "Gen" to 2, "Gen" to 1),
        )
    }

    @Test
    fun `the tap opens where the reader actually was, not the lowest number`() {
        // The run's most recent entry is its FIRST, and that is what opens.
        val one = historySpans(listOf("Gen" to 3, "Gen" to 2, "Gen" to 1)).single()
        assertEquals(3, one.open)
        // Reading BACKWARDS collapses too, and then the most recent is the low end.
        val back = historySpans(listOf("Gen" to 1, "Gen" to 2, "Gen" to 3)).single()
        assertEquals("Gen 1–3", back.label("Gen"))
        assertEquals(1, back.open)
    }

    @Test
    fun `a single chapter has no dash`() {
        assertEquals(listOf("John 3" to 3), spans("John" to 3))
    }

    @Test
    fun `a gap in the chapters is not a run`() {
        assertEquals(
            listOf("Gen 5" to 5, "Gen 1–2" to 2),
            spans("Gen" to 5, "Gen" to 2, "Gen" to 1),
        )
    }

    @Test
    fun `another book breaks the run, even when the chapters would have joined`() {
        // THE RULE THAT MATTERS: adjacency is in the LIST, not merely similarity.
        // Merging Gen 2 into Gen 3 here would claim the reader went 2→3 without
        // leaving, when in fact they went to John in between — it would rewrite
        // the order they did things in, which is the whole content of a history.
        assertEquals(
            listOf("Gen 3" to 3, "John 1" to 1, "Gen 2" to 2),
            spans("Gen" to 3, "John" to 1, "Gen" to 2),
        )
    }

    @Test
    fun `an empty history is an empty list, not a crash`() {
        assertEquals(emptyList<Any>(), historySpans(emptyList()))
    }

    @Test
    fun `several runs in a row each stand on their own`() {
        assertEquals(
            listOf("Ps 119" to 119, "Matt 5–7" to 7, "Rom 8" to 8),
            spans("Ps" to 119, "Matt" to 7, "Matt" to 6, "Matt" to 5, "Rom" to 8),
        )
    }
}
