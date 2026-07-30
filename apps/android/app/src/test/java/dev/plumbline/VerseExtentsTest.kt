// The scroll path's arithmetic: which verse is at the top edge, and how deep has
// the reader got.
//
// ReaderPane used to answer both by scanning the whole display list on every
// scroll frame, inside a LaunchedEffect keyed on the offset itself (so a drag
// also cancelled and relaunched a coroutine per frame). It now precomputes one
// per-verse extent table per layout and binary-searches it. A binary search that
// disagrees with the obvious implementation is the classic bug, so this pins
// three things: the table's shape, the searches against a linear scan of the same
// table, and — the one that matters most — the searches against the very item
// scans they replaced, over synthetic chapters laid out the way the core lays one
// out (crates/layout: strictly top-to-bottom, a verse's number ahead of its
// words, every box exactly one line tall).
//
// Pure arithmetic, plain JUnit, no Android runtime.

package dev.plumbline

import dev.plumbline.ui.VerseExtent
import dev.plumbline.ui.deepestVerseEntered
import dev.plumbline.ui.verseAtTop
import dev.plumbline.ui.verseExtents
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.random.Random

/** Every box the core emits is one line tall; an exact power-of-two-friendly
 *  value so every y in these tests is exact in Float. */
private const val LINE = 24f

/** A synthetic laid-out chapter. Lines are indices; a verse's number goes on
 *  [numberLine] and its words fill [wordLines] lines from [firstWordLine] (two
 *  to a line), which is how a wrapped first word and a verse spanning lines both
 *  get exercised. Consecutive verses may share a line — that tie is normal in
 *  flowing text and is where a boundary search goes wrong. */
private class FakeLayout {
    val items = ArrayList<DisplayItem>()

    fun verse(v: Int, numberLine: Int, firstWordLine: Int = numberLine, wordLines: Int = 1) {
        items.add(
            DisplayItem(
                x = 4f, y = numberLine * LINE, w = 10f, h = LINE,
                text = "$v", kind = "verseNumber", verseNumber = v,
            ),
        )
        var ti = 0
        for (l in 0 until wordLines) {
            for (col in 0 until 2) {
                items.add(
                    DisplayItem(
                        x = 20f + col * 60f, y = (firstWordLine + l) * LINE, w = 50f, h = LINE,
                        text = "word", kind = "word", verse = "Gen 1:$v", tokenIndex = ti++,
                    ),
                )
            }
        }
    }

    val bottom: Float get() = (items.maxOf { it.y }) + LINE
}

// ── the two scans the binary searches replaced, kept verbatim in shape ───────

/** The old first-visible scan: the first verse-number box whose bottom edge has
 *  not yet passed the top of the pane. */
private fun scanVerseAtTop(items: List<DisplayItem>, scrollY: Float): Int =
    items.firstOrNull { it.kind == "verseNumber" && it.y + it.h > scrollY }?.verseNumber ?: 0

/** The old high-water scan: the deepest verse with a word above the fold. */
private fun scanDeepest(items: List<DisplayItem>, fold: Float): Int =
    items.filter { it.kind == "word" && it.y + it.h <= fold }
        .mapNotNull { it.verse?.substringAfterLast(':')?.toIntOrNull() }
        .maxOrNull() ?: 0

// ── the obvious implementations, over the table itself ──────────────────────

private fun linearVerseAtTop(extents: List<VerseExtent>, scrollY: Float): Int =
    extents.firstOrNull { it.numberBottom > scrollY }?.verse ?: 0

private fun linearDeepest(extents: List<VerseExtent>, fold: Float): Int =
    extents.filter { it.entryBottom <= fold }.maxOfOrNull { it.verse } ?: 0

class VerseExtentsTest {

    /** Five lines: verse 1 alone, verse 2 whose first word wrapped to the next
     *  line, verse 3 running over two lines, and verse 4 starting on verse 3's
     *  last line (the tie). */
    private fun sample() = FakeLayout().apply {
        verse(1, numberLine = 0)
        verse(2, numberLine = 1, firstWordLine = 2)
        verse(3, numberLine = 3, wordLines = 2)
        verse(4, numberLine = 4)
    }

    @Test
    fun `the table is one row per verse, in verse order, with both bounds rising`() {
        val extents = verseExtents(sample().items)
        assertEquals(listOf(1, 2, 3, 4), extents.map { it.verse })
        assertEquals(
            "the number's line is the top-edge bound",
            listOf(24f, 48f, 96f, 120f),
            extents.map { it.numberBottom },
        )
        assertEquals(
            "a verse whose first word wrapped enters the page a line later than its number",
            listOf(24f, 72f, 96f, 120f),
            extents.map { it.entryBottom },
        )
        // The invariant the binary searches stand on.
        for (i in 1 until extents.size) {
            assertTrue(
                "numberBottom must not fall between verses ${extents[i - 1]} → ${extents[i]}",
                extents[i - 1].numberBottom <= extents[i].numberBottom,
            )
            assertTrue(
                "entryBottom must not fall between verses ${extents[i - 1]} → ${extents[i]}",
                extents[i - 1].entryBottom <= extents[i].entryBottom,
            )
        }
    }

    /** A verse that renders no words at all — every token blanked, which only the
     *  overlay can do — still gets an extent, on its number's own line, so the
     *  table stays sorted and searchable. */
    @Test
    fun `a verse with no words still has an extent`() {
        val items = FakeLayout().apply {
            verse(1, numberLine = 0)
            verse(2, numberLine = 1, wordLines = 0)
            verse(3, numberLine = 2)
        }.items
        val extents = verseExtents(items)
        assertEquals(listOf(1, 2, 3), extents.map { it.verse })
        assertEquals(48f, extents[1].numberBottom, 0f)
        assertEquals(48f, extents[1].entryBottom, 0f)
    }

    @Test
    fun `the top verse at a line boundary is the verse below the line`() {
        val extents = verseExtents(sample().items)
        // Verse 1's number ends at y=24. At exactly 24 it has gone: the pane's top
        // edge is on the next line, and the old scan's test was strict too.
        assertEquals(1, verseAtTop(extents, 23.99f))
        assertEquals(2, verseAtTop(extents, 24f))
        assertEquals(2, verseAtTop(extents, 47.99f))
        assertEquals(3, verseAtTop(extents, 48f))
    }

    @Test
    fun `the top verse above the first line and below the last`() {
        val extents = verseExtents(sample().items)
        assertEquals("nothing has scrolled yet: the first verse", 1, verseAtTop(extents, 0f))
        assertEquals("over-scrolled upward is still the first verse", 1, verseAtTop(extents, -100f))
        assertEquals("the last verse's own line", 4, verseAtTop(extents, 96f))
        assertEquals("past every verse: nothing to report", 0, verseAtTop(extents, 120f))
        assertEquals("far past every verse: nothing to report", 0, verseAtTop(extents, 9_999f))
        assertEquals("an empty chapter reports nothing", 0, verseAtTop(emptyList(), 0f))
    }

    @Test
    fun `the deepest verse entered at a boundary, and at both ends`() {
        val extents = verseExtents(sample().items)
        assertEquals("no line is fully above the fold yet", 0, deepestVerseEntered(extents, 0f))
        assertEquals(0, deepestVerseEntered(extents, 23.99f))
        assertEquals("the first line clears the fold exactly", 1, deepestVerseEntered(extents, 24f))
        assertEquals(1, deepestVerseEntered(extents, 71.99f))
        assertEquals("verse 2's wrapped first line clears the fold", 2, deepestVerseEntered(extents, 72f))
        assertEquals(3, deepestVerseEntered(extents, 96f))
        assertEquals(3, deepestVerseEntered(extents, 119.99f))
        assertEquals("the document bottom in view reaches the last verse", 4, deepestVerseEntered(extents, 120f))
        assertEquals(4, deepestVerseEntered(extents, 9_999f))
        assertEquals("an empty chapter reports nothing", 0, deepestVerseEntered(emptyList(), 9_999f))
    }

    /** The strongest test available: both searches must give what the obvious
     *  implementation gives, over randomised tables, at every boundary and
     *  between them. */
    @Test
    fun `both searches agree with a linear scan over a randomised table`() {
        for (seed in 1..200) {
            val rnd = Random(seed)
            val extents = randomTable(rnd)
            val probes = ArrayList<Float>()
            for (e in extents) {
                probes += e.numberBottom - LINE / 2f
                probes += e.numberBottom
                probes += e.numberBottom + 0.5f
                probes += e.entryBottom
                probes += e.entryBottom - 0.5f
            }
            probes += -LINE
            probes += 0f
            probes += 10_000f
            repeat(20) { probes += rnd.nextFloat() * (extents.size + 2) * LINE }

            for (p in probes) {
                assertEquals(
                    "verseAtTop disagrees with the linear scan at $p (seed $seed, ${extents.size} verses)",
                    linearVerseAtTop(extents, p),
                    verseAtTop(extents, p),
                )
                assertEquals(
                    "deepestVerseEntered disagrees with the linear scan at $p (seed $seed, ${extents.size} verses)",
                    linearDeepest(extents, p),
                    deepestVerseEntered(extents, p),
                )
            }
        }
    }

    /** And the table + searches together must answer exactly what the per-frame
     *  display-list scans answered, on chapters laid out like real ones. */
    @Test
    fun `the table and the searches agree with the item scans they replaced`() {
        for (seed in 1..100) {
            val rnd = Random(seed)
            val layout = randomChapter(rnd)
            val extents = verseExtents(layout.items)
            var y = -LINE
            while (y <= layout.bottom + 2 * LINE) {
                assertEquals(
                    "verseAtTop disagrees with the old item scan at $y (seed $seed)",
                    scanVerseAtTop(layout.items, y),
                    verseAtTop(extents, y),
                )
                assertEquals(
                    "deepestVerseEntered disagrees with the old item scan at fold $y (seed $seed)",
                    scanDeepest(layout.items, y),
                    deepestVerseEntered(extents, y),
                )
                y += LINE / 4f
            }
        }
    }

    // ── generators ─────────────────────────────────────────────────────────

    /** A table shaped like a real layout's: verses in order, both bounds rising,
     *  ties where two verses share a line, the odd wrapped first word. */
    private fun randomTable(rnd: Random): List<VerseExtent> {
        val out = ArrayList<VerseExtent>()
        var line = 0
        for (v in 1..rnd.nextInt(1, 40)) {
            line += rnd.nextInt(0, 3)
            val numberLine = line
            if (rnd.nextInt(8) == 0) line += 1 // the first word wrapped
            out.add(VerseExtent(v, (numberLine + 1) * LINE, (line + 1) * LINE))
        }
        return out
    }

    /** A chapter emitted the way the core emits one. */
    private fun randomChapter(rnd: Random): FakeLayout {
        val layout = FakeLayout()
        var line = 0
        for (v in 1..rnd.nextInt(1, 30)) {
            line += rnd.nextInt(0, 3)
            val numberLine = line
            if (rnd.nextInt(8) == 0) line += 1 // the first word wrapped past its number
            val wordLines = rnd.nextInt(1, 4)
            layout.verse(v, numberLine, firstWordLine = line, wordLines = wordLines)
            line += wordLines - 1
        }
        return layout
    }
}
