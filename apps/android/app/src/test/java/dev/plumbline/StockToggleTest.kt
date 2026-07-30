// The stock-REMOVAL rule from MainActivity.kt, on a real temp directory.
//
// Turning the bundled study set off used to delete every stock FILENAME, so a
// stock thread or weave the reader had renamed, re-noted or added verses to was
// destroyed alongside the pristine ones — their own work, deleted by a settings
// toggle that reads as "hide the examples" (2026-07-29). `isPristineCopy` is the
// rule that fixed it: a stock file may go only when its bytes are exactly the
// bundled asset's.
//
// Half these tests exist for ONE failure mode — a reader-edited file that a
// sloppier rule calls pristine. A length-only check, a "compare the first block"
// check, or a comparison that stops when either stream ends each pass the obvious
// cases and each delete somebody's work. They are pinned individually.
//
// Pure java.io — no Activity, no AssetManager, no Android runtime.

package dev.plumbline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream

class StockToggleTest {

    @get:Rule
    val tmp = TemporaryFolder()

    /** A bundled stock thread, as it ships. */
    private val pristine = """{"name":"Romans Road","notes":"a","entries":[]}"""

    private fun asset(text: String): InputStream = ByteArrayInputStream(text.toByteArray())

    private fun file(dir: File, name: String, text: String): File =
        File(dir, name).also { it.writeText(text) }

    @Test
    fun `an untouched stock file is removable`() {
        val dir = tmp.newFolder("threads")
        val dest = file(dir, "romans-road.json", pristine)
        assertTrue("the bytes are exactly the asset's", isPristineCopy(dest, asset(pristine)))
    }

    @Test
    fun `a renamed stock file is kept`() {
        val dir = tmp.newFolder("threads")
        val dest = file(dir, "romans-road.json", """{"name":"My road through Romans","notes":"a","entries":[]}""")
        assertFalse("the reader renamed it — this is their file now", isPristineCopy(dest, asset(pristine)))
    }

    // ── the cases that would cost real data ────────────────────────────────
    // Each of these is an EDIT that a plausible shortcut calls pristine.

    @Test
    fun `an edit that keeps the byte count is kept`() {
        val dir = tmp.newFolder("threads")
        // One byte differs, at the very end, and the length is identical: a
        // length-only rule (or one comparing a prefix) deletes the reader's note.
        val edited = """{"name":"Romans Road","notes":"b","entries":[]}"""
        assertEquals("the fixture must be the same length as pristine", pristine.length, edited.length)
        val dest = file(dir, "romans-road.json", edited)
        assertFalse("same length, different bytes — this is an edit", isPristineCopy(dest, asset(pristine)))
    }

    @Test
    fun `a file the reader appended to is kept`() {
        val dir = tmp.newFolder("threads")
        // Starts with every pristine byte: a comparison that stops when the asset
        // stream ends sees a perfect match and deletes it.
        val dest = file(dir, "romans-road.json", pristine + """{"more":"mine"}""")
        assertFalse("a longer file that starts pristine is still an edit", isPristineCopy(dest, asset(pristine)))
    }

    @Test
    fun `a truncated stock file is kept`() {
        val dir = tmp.newFolder("threads")
        // The mirror image: every byte present matches, but bytes are missing. A
        // comparison that stops when the FILE ends calls this pristine.
        val dest = file(dir, "romans-road.json", pristine.substring(0, pristine.length - 6))
        assertFalse("a short file that starts pristine is not the asset", isPristineCopy(dest, asset(pristine)))
    }

    @Test
    fun `a file larger than one read buffer is compared to its last byte`() {
        val dir = tmp.newFolder("weaves")
        // The biggest stock weave is 20 KB, and the compare reads in blocks: a rule
        // that checked only the first block would delete this.
        val big = "x".repeat(20_000)
        val dest = file(dir, "solomons-temple.json", big + "MINE")
        assertFalse("the difference is 20 KB in and still counts", isPristineCopy(dest, asset(big + "shipped")))
        val same = file(dir, "the-crucifixion.json", big)
        assertTrue("20 KB of identical bytes is identical", isPristineCopy(same, asset(big)))
    }

    @Test
    fun `a difference that lands exactly on a block boundary is still a difference`() {
        val dir = tmp.newFolder("weaves")
        // THE "END TOGETHER" LEG, and the only place it can be got wrong. Every
        // case above is caught by comparing byte counts, because a whole small
        // file arrives in the first block. When one stream ends exactly ON a block
        // boundary, though, the next round reads 0 from it and n from the other —
        // and a comparison that treats "one of them ended" as "they matched"
        // deletes the reader's file.
        //
        // 64 KB is a multiple of every plausible block size, so this keeps biting
        // if the compare buffer is ever resized.
        val block = "x".repeat(65_536)
        val appended = file(dir, "solomons-temple.json", block + "MINE")
        assertFalse("the reader appended past the boundary — keep it", isPristineCopy(appended, asset(block)))
        val truncated = file(dir, "the-crucifixion.json", block)
        val longer = asset(block + "shipped")
        assertFalse("the file stops on the boundary, the asset does not", isPristineCopy(truncated, longer))
        val whole = file(dir, "the-empty-tomb.json", block)
        assertTrue("and an exact multiple that really matches still matches", isPristineCopy(whole, asset(block)))
    }

    // ── what the toggle does with those verdicts ───────────────────────────

    @Test
    fun `nothing to remove is not an error`() {
        val dir = tmp.newFolder("threads")
        val gone = File(dir, "romans-road.json")
        assertFalse("a stock file the reader deleted stays deleted", isPristineCopy(gone, asset(pristine)))
        val subdir = File(dir, "suggested").also { it.mkdirs() }
        assertFalse("a directory is never a removable stock file", isPristineCopy(subdir, asset(pristine)))
        assertTrue("and it is still there", subdir.isDirectory)
    }

    /** The OFF pass as clearStock runs it: for each bundled name, delete the
     *  destination only when it is provably the shipped bytes. Returns the names
     *  that survived. */
    private fun clearStock(dir: File, assets: Map<String, String>): List<String> {
        for ((name, bytes) in assets) {
            val dest = File(dir, name)
            if (!dest.isFile) continue
            if (isPristineCopy(dest, asset(bytes))) dest.delete()
        }
        return dir.list()!!.sorted()
    }

    @Test
    fun `turning the set off keeps the reader's edits and takes the examples`() {
        val dir = tmp.newFolder("threads")
        file(dir, "romans-road.json", """{"name":"Romans Road","notes":"mine","entries":[]}""")
        file(dir, "law-and-grace.json", pristine)
        file(dir, "my-own-thread.json", """{"name":"My own","notes":"","entries":[]}""")

        val left = clearStock(dir, mapOf("romans-road.json" to pristine, "law-and-grace.json" to pristine))

        assertEquals(
            "the edited stock thread and the reader's own must both survive; the pristine example must go",
            listOf("my-own-thread.json", "romans-road.json"),
            left,
        )
        assertTrue(
            "and the surviving stock file must still hold the reader's edit",
            File(dir, "romans-road.json").readText().contains("\"notes\":\"mine\""),
        )
    }

    @Test
    fun `turning it off twice removes nothing further`() {
        val dir = tmp.newFolder("threads")
        file(dir, "romans-road.json", """{"name":"Romans Road","notes":"mine","entries":[]}""")
        file(dir, "law-and-grace.json", pristine)
        val assets = mapOf("romans-road.json" to pristine, "law-and-grace.json" to pristine)

        clearStock(dir, assets)
        val left = clearStock(dir, assets)

        assertEquals("a second pass must be a no-op", listOf("romans-road.json"), left)
    }
}
