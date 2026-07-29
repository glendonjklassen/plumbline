// The stock-seeding rules from MainActivity.kt, on a real temp directory.
//
// Two guarantees the reader's data depends on, both regressions as of 2026-07-29:
// a destination that already exists is theirs and is never re-copied over (the
// old seedStock reverted a renamed or re-noted stock thread at every launch), and
// every write lands whole or not at all (the old copyAsset streamed straight onto
// the destination, so an interrupted copy left a truncated file).
//
// Pure java.io — no Activity, no AssetManager, no Android runtime.

package dev.plumbline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.File
import java.io.IOException
import java.io.InputStream

class StockSeedTest {

    @get:Rule
    val tmp = TemporaryFolder()

    private val pristine = """{"name":"Romans Road","refs":[]}"""
    private val edited = """{"name":"The Romans road (mine)","refs":[],"notes":"my own"}"""

    @Test
    fun `an existing destination is never seeded over`() {
        val dir = tmp.newFolder("threads")
        val mine = File(dir, "romans-road.json")
        mine.writeText(edited)
        assertFalse("a file the reader already has must not be re-seeded", shouldSeed(mine))
        assertTrue("a file that isn't there yet must be seeded", shouldSeed(File(dir, "law-and-grace.json")))
    }

    /** The seed pass as copyAsset runs it: for each bundled name, write it only
     *  when [shouldSeed] says so. */
    private fun seed(dir: File, assets: Map<String, String>) {
        for ((name, bytes) in assets) {
            val dest = File(dir, name)
            if (!shouldSeed(dest)) continue
            writeThroughTemp(dest, ByteArrayInputStream(bytes.toByteArray()))
        }
    }

    @Test
    fun `re-seeding keeps the reader's edits and restores only what is missing`() {
        val dir = tmp.newFolder("threads")
        File(dir, "romans-road.json").writeText(edited)
        val assets = mapOf("romans-road.json" to pristine, "law-and-grace.json" to pristine)

        seed(dir, assets)
        seed(dir, assets) // a second launch must change nothing

        assertEquals("the reader's edit was reverted", edited, File(dir, "romans-road.json").readText())
        assertEquals("a missing stock file should come back", pristine, File(dir, "law-and-grace.json").readText())
    }

    @Test
    fun `a write in flight never touches the destination`() {
        val dir = tmp.newFolder("threads")
        val dest = File(dir, "romans-road.json")
        dest.writeText(edited)

        var duringCopy = ""
        var namesDuringCopy = emptyList<String>()
        writeThroughTemp(
            dest,
            twoChunkStream(pristine.toByteArray()) {
                duringCopy = dest.readText()
                namesDuringCopy = dir.list()!!.sorted()
            },
        )

        assertEquals("the destination was written in place, so a crash here truncates it", edited, duringCopy)
        assertTrue(
            "the in-flight bytes must sit in a hidden .tmp sibling, saw $namesDuringCopy",
            namesDuringCopy.any { it.startsWith(".") && it.endsWith(".tmp") },
        )
        assertEquals("the finished copy should be in place", pristine, dest.readText())
        assertEquals("no temp file may be left behind", listOf("romans-road.json"), dir.list()!!.sorted())
    }

    @Test
    fun `an interrupted write leaves the old file whole`() {
        val dir = tmp.newFolder("threads")
        val dest = File(dir, "romans-road.json")
        dest.writeText(edited)

        try {
            writeThroughTemp(dest, failingStream(pristine.toByteArray().copyOfRange(0, 8)))
            fail("the copy should have thrown")
        } catch (e: IOException) {
            assertEquals("the asset stream died", e.message)
        }

        assertEquals("an interrupted copy truncated the reader's file", edited, dest.readText())
        assertEquals("no temp file may be left behind", listOf("romans-road.json"), dir.list()!!.sorted())
    }

    /** Hands [bytes] over in two reads, running [between] after the first has been
     *  written — a window onto what the directory looks like mid-copy. */
    private fun twoChunkStream(bytes: ByteArray, between: () -> Unit): InputStream =
        object : InputStream() {
            private var pos = 0
            private var reads = 0
            override fun read(): Int = if (pos < bytes.size) bytes[pos++].toInt() and 0xff else -1
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                if (pos >= bytes.size) return -1
                if (++reads == 2) between()
                val n = minOf(len, maxOf(1, bytes.size / 2), bytes.size - pos)
                System.arraycopy(bytes, pos, b, off, n)
                pos += n
                return n
            }
        }

    /** Yields [prefix], then fails — a copy cut short partway. */
    private fun failingStream(prefix: ByteArray): InputStream =
        object : InputStream() {
            private var pos = 0
            override fun read(): Int = throw IOException("the asset stream died")
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                if (pos > 0) throw IOException("the asset stream died")
                val n = minOf(len, prefix.size)
                System.arraycopy(prefix, 0, b, off, n)
                pos = n
                return n
            }
        }
}
