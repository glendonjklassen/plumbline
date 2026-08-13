// The chapters just behind the reader: what is kept, what is freed, and when.
//
// A back-swipe used to re-lay out a chapter the reader was on two seconds ago —
// a native line-break of every word plus a JSON parse of the result. ReaderPane
// keeps the last few instead (ChapterCache), and that is a trade with a sharp
// edge on both sides:
//
//  - a laid-out chapter is NATIVE memory nothing but close() frees, so an entry
//    dropped without closing is leaked for the life of the process; and
//  - the chapter on screen is IN the cache, so an entry closed at the wrong
//    moment is a freed display list under a thumb that is still scrolling and
//    hit-testing it.
//
// What keeps both safe is one property, and it is the property this file exists
// to hold: the entry the pane is painting is always the most recently used, and
// eviction only ever takes the least recently used. Break the ordering — evict
// the newest, or stop refreshing an entry when it is read — and the randomised
// walk below frees the page the reader is looking at.
//
// The other half is the cache KEY. Hand back a chapter laid out for a different
// column width or text size and the reader gets a page of text in the wrong
// places; so every input the core's line-breaker sees has a case here, and the
// last test reads ReaderPane's own layout config back out of the source to catch
// an input added to the layout but not to the key.
//
// Pure logic, plain JUnit, a Closeable stand-in for the native handle.

package dev.plumbline

import dev.plumbline.ui.CHAPTER_CACHE
import dev.plumbline.ui.ChapterCache
import dev.plumbline.ui.ChapterKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.Closeable
import java.io.File
import java.lang.reflect.Modifier
import kotlin.random.Random

/** A laid-out chapter's stand-in: it only has to notice being freed. */
private class FakeChapter(val name: String) : Closeable {
    var closes = 0
        private set

    override fun close() {
        closes++
    }
}

class ChapterPaintCacheTest {

    // ── the bound ───────────────────────────────────────────────────────────

    @Test
    fun `the pane keeps two to four chapters, and never one`() {
        assertTrue(
            "CHAPTER_CACHE is $CHAPTER_CACHE. Below 2 the pane frees the page it is painting; " +
                "well above 4 a phone is holding megabytes of display lists for chapters nobody " +
                "is going back to (~350 KB for an average chapter, ~1.3 MB for Psalm 119)",
            CHAPTER_CACHE in 2..4,
        )
    }

    @Test
    fun `a capacity that could free the page on screen is refused`() {
        for (bad in listOf(1, 0, -1)) {
            assertThrows(
                "a cache of $bad accepted — the chapter on screen is the most recent entry, " +
                    "so at a capacity of one the next chapter's arrival frees it",
                IllegalArgumentException::class.java,
            ) { ChapterCache<Int, FakeChapter>(bad) }
        }
        ChapterCache<Int, FakeChapter>(2).close() // the smallest safe one
    }

    // ── the ordering ────────────────────────────────────────────────────────

    @Test
    fun `the least recently used chapter is the one evicted, and it is closed`() {
        val cache = ChapterCache<Int, FakeChapter>(3)
        val ch = (1..4).map { FakeChapter("ch $it") }
        for (i in 0..2) cache.put(i + 1, ch[i])
        assertEquals(listOf(1, 2, 3), cache.lruOrder())

        cache.put(4, ch[3])
        assertEquals("the oldest chapter should have gone", listOf(2, 3, 4), cache.lruOrder())
        assertEquals("an evicted chapter that is not closed is leaked native memory", 1, ch[0].closes)
        assertEquals(0, ch[1].closes)
        assertNull("an evicted chapter must not still answer", cache.get(1))
        assertEquals(3, cache.size)
    }

    @Test
    fun `reading a chapter makes it the most recent, so the one before it goes first`() {
        val cache = ChapterCache<Int, FakeChapter>(3)
        val ch = (1..4).map { FakeChapter("ch $it") }
        for (i in 0..2) cache.put(i + 1, ch[i])

        // The reader swipes back to chapter 1: it is the page on screen now.
        assertSame("a back-swipe must find the chapter still laid out", ch[0], cache.get(1))
        assertEquals(listOf(2, 3, 1), cache.lruOrder())

        cache.put(4, ch[3])
        assertEquals("chapter 2 was the least recently used, not chapter 1", listOf(3, 1, 4), cache.lruOrder())
        assertEquals("the chapter the reader had just gone back to was freed", 0, ch[0].closes)
        assertEquals(1, ch[1].closes)
    }

    @Test
    fun `a back-swipe hands back the very same layout, not a copy`() {
        val cache = ChapterCache<Int, FakeChapter>(3)
        val one = FakeChapter("Gen 1")
        cache.put(1, one)
        cache.put(2, FakeChapter("Gen 2"))
        assertSame("the whole point: nothing to lay out and nothing to parse", one, cache.get(1))
        assertEquals(0, one.closes)
    }

    @Test
    fun `a chapter put twice under one key does not leak the first`() {
        val cache = ChapterCache<Int, FakeChapter>(2)
        val first = FakeChapter("first")
        val second = FakeChapter("second")
        cache.put(1, first)
        cache.put(1, second)
        assertEquals("the displaced layout is unreachable, so it must be freed", 1, first.closes)
        assertEquals(0, second.closes)
        assertSame(second, cache.get(1))
        assertEquals(1, cache.size)

        // Putting the SAME object back is not a reason to free it.
        cache.put(1, second)
        assertEquals(0, second.closes)
    }

    @Test
    fun `leaving the pane frees every chapter, once`() {
        val cache = ChapterCache<Int, FakeChapter>(3)
        val ch = (1..3).map { FakeChapter("ch $it") }
        for (i in 0..2) cache.put(i + 1, ch[i])
        cache.close()
        for (c in ch) assertEquals("${c.name} outlived the pane", 1, c.closes)
        assertEquals(0, cache.size)
        cache.close()
        for (c in ch) assertEquals("${c.name} was freed twice", 1, c.closes)
    }

    /**
     * The invariant, over a reader who wanders: the entry the pane is painting is
     * never the one eviction takes.
     *
     * The window that matters is the instant after a new chapter is put — the
     * page on screen is second-most-recent for exactly that long, and it is only
     * a capacity of two or more plus least-recently-used eviction that keeps it
     * out of the victim's seat.
     */
    @Test
    fun `the chapter on screen is never freed under the pane`() {
        for (capacity in 2..4) {
            for (seed in 1..100) {
                val rnd = Random(seed * 10 + capacity)
                val cache = ChapterCache<Int, FakeChapter>(capacity)
                var showing: FakeChapter? = null
                var at = rnd.nextInt(1, 6)
                var laidOut = 0
                repeat(80) {
                    // Mostly next/previous, the way a reader moves; occasionally a
                    // jump (a search result, the navigator).
                    at = when (rnd.nextInt(10)) {
                        0 -> rnd.nextInt(1, 40)
                        in 1..5 -> at + 1
                        else -> max1(at - 1)
                    }
                    val next = cache.get(at) ?: FakeChapter("ch $at").also {
                        laidOut++
                        cache.put(at, it)
                    }
                    // The put above is the eviction: the page that was on screen
                    // when it happened must have survived it.
                    showing?.let {
                        assertEquals(
                            "capacity $capacity, seed $seed: the chapter the pane was painting " +
                                "(${it.name}) was freed to make room for ch $at",
                            0, it.closes,
                        )
                    }
                    showing = next
                    assertEquals("the chapter just published was already freed", 0, next.closes)
                    assertTrue("the cache grew past its bound", cache.size <= capacity)
                }
                assertTrue(
                    "80 steps of back-and-forth needed $laidOut layouts — the cache saved nothing",
                    laidOut < 80,
                )
                cache.close()
            }
        }
    }

    private fun max1(v: Int) = if (v < 1) 1 else v

    // ── the key ─────────────────────────────────────────────────────────────

    private fun key(
        book: String = "Gen",
        chapter: Int = 1,
        column: Float = 640f,
        fontPx: Float = 47.4f,
        lineHeight: Float = 64.1f,
        spaceWidth: Float = 11.9f,
        versePerLine: Boolean = false,
        verseNumbers: Boolean = true,
        akjvOverlay: Boolean = false,
    ) = ChapterKey(book, chapter, column, fontPx, lineHeight, spaceWidth, versePerLine, verseNumbers, akjvOverlay)

    /** Every input the core's line-breaker sees, as what the reader did. */
    private fun everyInput(): List<Pair<String, ChapterKey>> = listOf(
        "the reader turned to another book" to key(book = "Exod"),
        "the reader turned the page" to key(chapter = 2),
        "the pane got wider, or the side margin narrower" to key(column = 700f),
        "the reader moved the text-size slider" to key(fontPx = 52f),
        "the reader moved the line-spacing slider" to key(lineHeight = 70f),
        "the reading face changed the width of a space" to key(spaceWidth = 12.4f),
        "the reader asked for a verse per line" to key(versePerLine = true),
        // Numbers off moves every word on every line — a LAYOUT input, unlike
        // the italics switch, which repaints and is deliberately not in the key.
        "the reader turned the verse numbers off" to key(verseNumbers = false),
        "the reader turned the plain-English overlay on" to key(akjvOverlay = true),
    )

    @Test
    fun `every layout input changes the cache key`() {
        val base = key()
        assertEquals("a key built twice from the same inputs must be the same key", base, key())
        for ((what, changed) in everyInput()) {
            assertNotEquals(
                "$what, and the cache key did not move — the pane would be handed a chapter " +
                    "laid out for the inputs before it",
                base, changed,
            )
        }
    }

    @Test
    fun `the key's fields are exactly the layout inputs`() {
        val fields = ChapterKey::class.java.declaredFields
            .filterNot { Modifier.isStatic(it.modifiers) }   // the Compose plugin's $stable
            .map { it.name }
            .toSet()
        assertEquals(
            "ChapterKey's fields and the inputs this file tests have drifted apart",
            setOf(
                "book", "chapter", "column", "fontPx",
                "lineHeight", "spaceWidth", "versePerLine", "verseNumbers", "akjvOverlay",
            ),
            fields,
        )
    }

    /**
     * And the key must still cover the config the pane actually fills in.
     *
     * This one reads the source, because the drift it guards against cannot be
     * seen from any value: a new `PlumblineLayoutConfig` field set from some new
     * reader pref would change how the core breaks lines while leaving
     * [ChapterKey] — and so the cache — unable to tell the two layouts apart. The
     * reader would swipe back and get the page laid out the old way. Every
     * right-hand side in that config block must therefore be one of the pane's
     * three derived values, each of which IS a key field.
     */
    @Test
    fun `every layout config field is set from something the key holds`() {
        val src = readerPaneSource()
        val marker = "PlumblineLayoutConfig.ByValue().apply {"
        val open = src.indexOf(marker)
        assertTrue(
            "ReaderPane no longer builds its layout config the way this guard reads it — " +
                "re-point the guard rather than deleting it",
            open >= 0,
        )
        var depth = 0
        var i = open + marker.length - 1
        while (i < src.length) {
            if (src[i] == '{') depth++
            if (src[i] == '}' && --depth == 0) break
            i++
        }
        val block = src.substring(open + marker.length, i)

        for (line in block.lines()) {
            val code = line.substringBefore("//").trim()
            if (!code.contains('=')) continue
            val rhs = code.substringAfter('=')
            val names = IDENT.findAll(rhs).map { it.value }.filter { it !in KEYWORDS }.toList()
            for (name in names) {
                assertTrue(
                    "the layout config is set from `$name`, which is not one of the values " +
                        "ChapterKey carries ($DERIVED) — a chapter cached before `$name` changed " +
                        "would be handed back for a layout it was never made with:\n  $code",
                    name in DERIVED,
                )
            }
        }
    }

    private companion object {
        /** The pane's own names for the three derived layout inputs, which are
         *  ChapterKey's `column`, `lineHeight` and `spaceWidth`, plus the flag
         *  that is passed straight through (`versePerLine`). */
        private val DERIVED = setOf("column", "lineH", "space", "versePerLine", "verseNumbers")

        private val KEYWORDS = setOf("if", "else", "true", "false")

        /** An identifier, and not the `f` of `1.4f`. */
        private val IDENT = Regex("""(?<![A-Za-z0-9_.])[A-Za-z_][A-Za-z0-9_]*""")

        /** Walks up from the test's working directory (Gradle: the `app` project
         *  dir), and fails loudly rather than skipping — a guard that quietly
         *  finds nothing guards nothing. Same shape as LayoutTriggerTest's. */
        private fun readerPaneSource(): String {
            val tail = "src/main/java/dev/plumbline/ui/ReaderPane.kt"
            var dir: File? = File("").absoluteFile
            while (dir != null) {
                for (prefix in listOf("", "app/", "apps/android/app/")) {
                    val f = File(dir, prefix + tail)
                    if (f.isFile) return f.readText()
                }
                dir = dir.parentFile
            }
            throw AssertionError("cannot find $tail from ${File("").absolutePath}")
        }
    }
}
