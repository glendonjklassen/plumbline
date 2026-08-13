// When the reader's page has to be painted again.
//
// ReaderPane no longer draws a chapter word by word on every frame: it records
// the whole thing into an android.graphics.Picture once and replays it. That is
// worth 400–900 JNI drawText calls a frame, and it buys a new way to be wrong —
// a frame replays the recording without consulting anything, so an input the
// recording depends on that is NOT in ChapterPaintKey leaves the reader looking
// at the old page. A theme change with the ink missing from the key is the exact
// shape of it: the words stay in the colours of the theme before last.
//
// So this file is a list of every input the recording reads, and it asserts two
// things about each: it changes the key, and a changed key records again. Drop
// any one field from ChapterPaintKey and the case that names it fails — plus the
// shape test at the end, which pins the field list itself so an input can be
// neither added silently nor removed silently.
//
// Pure logic, plain JUnit: no Canvas, no Picture, no Android runtime. The
// recording itself is framework code the maintainer sees on the device; what is
// testable here is exactly what decides WHEN it happens.

package dev.plumbline

import dev.plumbline.ui.ChapterPaintKey
import dev.plumbline.ui.ReaderInks
import dev.plumbline.ui.Recorded
import dev.plumbline.ui.Same
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.lang.reflect.Modifier

class ChapterPaintKeyTest {

    // ── the inputs, one baseline and one change each ────────────────────────

    /** Two chapters' worth of display list with identical CONTENT. `listB` is
     *  what a re-layout hands back: equal to look at, a different object. */
    private val listA = chapter()
    private val listB = chapter()

    /** Whatever object the pane's typefaces happen to be — the key only asks
     *  whether it is still the same one. */
    private val fonts = Any()

    private val inks = ReaderInks(
        ink = 0xFF1A1A18.toInt(),
        added = 0xFF6E6A62.toInt(),
        divine = 0xFF7A2E2E.toInt(),
        title = 0xFF4A4A44.toInt(),
        gold = 0xFF8A6A1F.toInt(),
    )

    private fun key(
        layout: Any? = listA,
        fonts: Any? = this.fonts,
        fontPx: Float = 47.4f,
        textH: Float = 55.2f,
        ascent: Float = -44.1f,
        inks: ReaderInks = this.inks,
        addedItalics: Boolean = true,
    ) = ChapterPaintKey(Same(layout), Same(fonts), fontPx, textH, ascent, inks, addedItalics)

    /**
     * Every input [dev.plumbline.ui.recordChapter] reads, as the thing the reader
     * did to change it. Each must produce a key that differs from [key]'s
     * baseline — and each is here because losing it is a specific stale page.
     */
    private fun everyInput(): List<Pair<String, ChapterPaintKey>> = listOf(
        "the chapter was laid out again — new words, new boxes" to key(layout = listB),
        "the reading face changed" to key(fonts = Any()),
        "the reader moved the text-size slider" to key(fontPx = 52f),
        "the face's height moved with the size" to key(textH = 61f),
        "the face's ascent moved with the size" to key(ascent = -48f),
        "the theme changed the body ink" to key(inks = inks.copy(ink = 0xFFE8E4DA.toInt())),
        "the theme changed the added-word ink (KJV italics)" to key(inks = inks.copy(added = 0xFF9A968E.toInt())),
        "the theme changed the divine-name ink" to key(inks = inks.copy(divine = 0xFFCC6666.toInt())),
        "the theme changed the psalm-title ink" to key(inks = inks.copy(title = 0xFF8A8A80.toInt())),
        "the theme changed the gold — verse numbers AND the overlay's dotted mark"
            to key(inks = inks.copy(gold = 0xFFD4AF37.toInt())),
        // The recording BAKES which paint supplied words were drawn with, so the
        // switch has to move the key or the italics stay on screen after it.
        "the reader turned the supplied-word italics off" to key(addedItalics = false),
    )

    // ── the two things that must be true of every one of them ───────────────

    @Test
    fun `every input to the recording changes the key`() {
        val base = key()
        assertEquals("a key built twice from the same inputs must be the same key", base, key())
        for ((what, changed) in everyInput()) {
            assertNotEquals(
                "$what, and the paint key did not move — the chapter would stay on screen " +
                    "exactly as it was recorded",
                base, changed,
            )
        }
    }

    @Test
    fun `an input changes, so the chapter is recorded again — and only then`() {
        val recorder = Recorded<String>()
        var made = 0
        val record: (ChapterPaintKey) -> String = { made++; "picture $made" }

        val first = recorder.of(key(), record)
        // A drag is sixty frames a second of exactly the same inputs. Every one
        // of them must replay the recording it already has; that IS the fix.
        repeat(60) { assertSame(first, recorder.of(key(), record)) }
        assertEquals("a frame that changed nothing re-recorded the chapter", 1, recorder.records)

        for ((what, changed) in everyInput()) {
            val before = recorder.records
            recorder.of(changed, record)
            assertEquals(
                "$what, and the chapter was not recorded again — the reader is looking at a stale page",
                before + 1, recorder.records,
            )
            // …and back, which is a change too (returning to the old theme, or
            // to the chapter behind a back-swipe, must repaint just the same).
            val back = recorder.records
            recorder.of(key(), record)
            assertEquals("returning to the previous inputs must record again", back + 1, recorder.records)
        }
    }

    /** The display list rides in the key by REFERENCE, deliberately: it is the
     *  only comparison cheap enough to make on every recomposition. This is the
     *  proof that the reference is what the key is reading — the two lists below
     *  are equal as data and still different keys. */
    @Test
    fun `the display list is compared by identity, not by walking every item`() {
        assertEquals("the two sample lists must be equal as DATA for this test to mean anything", listA, listB)
        assertNotEquals(key(layout = listA), key(layout = listB))
        assertEquals(key(layout = listA), key(layout = listA))
        assertNotEquals("a chapter and no chapter are not the same page", key(layout = listA), key(layout = null))
    }

    /**
     * The field list itself, so an input can be neither added nor dropped in
     * silence: a new field with no case in [everyInput] fails here, and a field
     * removed to "simplify" the key fails both here and above.
     *
     * The absences are as deliberate as the presences. The scroll offset, the
     * pinned span, the search bands and the note dots are NOT recorded — the
     * offset is a draw-phase translate over the replay and the other three paint
     * live beneath it — so putting any of them here would mean re-recording the
     * whole chapter on a tap.
     */
    @Test
    fun `the key's fields are exactly the recorded inputs`() {
        val fields = ChapterPaintKey::class.java.declaredFields
            .filterNot { Modifier.isStatic(it.modifiers) }   // the Compose plugin's $stable
            .map { it.name }
            .toSet()
        assertEquals(
            "ChapterPaintKey's fields and the inputs this file tests have drifted apart",
            setOf("layout", "fonts", "fontPx", "textH", "ascent", "inks", "addedItalics"),
            fields,
        )
    }

    /** [Same] is doing one job; it should do it for nulls and for itself too. */
    @Test
    fun `identity wrapper`() {
        val thing = Any()
        assertEquals(Same(thing), Same(thing))
        assertEquals(Same(thing).hashCode(), Same(thing).hashCode())
        assertEquals(Same(null), Same(null))
        assertNotEquals(Same(thing), Same(null))
        assertNotEquals(Same(Any()), Same(Any()))
        assertTrue("a wrapper is not the thing it wraps", Same(thing) != thing)
    }

    // ── a sample chapter ────────────────────────────────────────────────────

    /** Three words on a line, shaped the way the core emits them. */
    private fun chapter() = DisplayList(
        width = 640f,
        height = 48f,
        items = listOf(
            DisplayItem(x = 4f, y = 0f, w = 10f, h = 24f, text = "1", kind = "verseNumber", verseNumber = 1),
            DisplayItem(
                x = 20f, y = 0f, w = 40f, h = 24f, text = "In", kind = "word",
                verse = "Gen 1:1", tokenIndex = 0,
            ),
            DisplayItem(
                x = 64f, y = 0f, w = 60f, h = 24f, text = "the", kind = "word",
                verse = "Gen 1:1", tokenIndex = 1,
            ),
        ),
    )
}
