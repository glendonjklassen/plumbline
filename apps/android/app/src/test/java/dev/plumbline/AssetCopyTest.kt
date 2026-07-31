package dev.plumbline

// The extraction copy loop (MainActivity.kt's `writeThroughTemp`).
//
// The first-run extraction stopped calling `copyTo` and now runs its own loop
// over ONE buffer that the whole pass shares, because 34.8 MB of bundled assets
// was going through copyTo's 8 KB default — roughly 4,400 read/write pairs, each
// read a JNI hop into the asset's inflater.
//
// A hand-written copy loop is exactly the kind of change that can corrupt every
// file it writes and still compile, and the corruption it produces is silent:
// the extraction reports success, the marker file is dropped, and the reader
// gets a corpus that never re-extracts. So the loop's two obligations are pinned
// here — write what was READ rather than what the buffer HOLDS, and leave
// nothing of the previous file behind in a buffer the next one reuses.

import org.junit.Assert.assertArrayEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream

class AssetCopyTest {
    @get:Rule val tmp = TemporaryFolder()

    /** Deterministic non-repeating filler, so a misplaced byte is visible. */
    private fun filler(n: Int, seed: Int) = ByteArray(n) { ((it * 31 + seed) and 0xff).toByte() }

    @Test
    fun `every size around the buffer boundary copies byte for byte`() {
        val dir = tmp.newFolder("data")
        val buf = ByteArray(16)
        // Empty, under, exactly one bufferful, one past, an exact multiple, and
        // a payload many bufferfuls long.
        for (size in listOf(0, 1, 15, 16, 17, 32, 1000)) {
            val payload = filler(size, size)
            val dest = File(dir, "asset-$size")
            writeThroughTemp(dest, ByteArrayInputStream(payload), buf)
            assertArrayEquals("a $size-byte asset came back different", payload, dest.readBytes())
        }
    }

    @Test
    fun `a buffer reused across files never leaks the previous one's bytes`() {
        val dir = tmp.newFolder("data")
        // One array for the whole pass, exactly as the extraction runs it: the
        // corpus first, then a stock study file a fraction of its size.
        val buf = ByteArray(64)
        val corpus = filler(64 * 3, 7)
        val stock = """{"name":"Romans Road"}""".toByteArray()

        writeThroughTemp(File(dir, "kjv.jsonl"), ByteArrayInputStream(corpus), buf)
        writeThroughTemp(File(dir, "romans-road.json"), ByteArrayInputStream(stock), buf)

        assertArrayEquals("the corpus did not survive the copy", corpus, File(dir, "kjv.jsonl").readBytes())
        assertArrayEquals(
            "the small file carried the corpus's bytes out of the shared buffer",
            stock,
            File(dir, "romans-road.json").readBytes(),
        )
    }

    @Test
    fun `a short read writes only what arrived`() {
        // AssetManager's stream hands over less than the buffer holds, routinely
        // — a compressed asset delivers an inflater window at a time. A loop that
        // wrote the buffer's length instead of the read's return value would pad
        // every asset out to a multiple of 256 KB.
        val dir = tmp.newFolder("data")
        val payload = filler(500, 3)
        val dest = File(dir, "kjv-notes.jsonl")

        writeThroughTemp(dest, dribble(payload, 7), ByteArray(256 * 1024))

        assertArrayEquals("the copy padded a short read", payload, dest.readBytes())
    }

    /** Hands [bytes] over [chunk] at a time, however much the caller asked for. */
    private fun dribble(bytes: ByteArray, chunk: Int): InputStream = object : InputStream() {
        private var pos = 0
        override fun read(): Int = if (pos < bytes.size) bytes[pos++].toInt() and 0xff else -1
        override fun read(b: ByteArray, off: Int, len: Int): Int {
            if (pos >= bytes.size) return -1
            val n = minOf(chunk, len, bytes.size - pos)
            System.arraycopy(bytes, pos, b, off, n)
            pos += n
            return n
        }
    }
}
