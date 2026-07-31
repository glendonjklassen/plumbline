package dev.plumbline

// Two restores at once, over the one staging directory they share.
//
// A hazard the move OFF the main thread creates. `restoreZipInto` unpacks into
// a single ".restore-tmp" inside the home and the first thing it does is wipe
// it, which was safe for exactly one reason: the zip ran inside the SAF result
// callback, on the main thread, so the UI could not accept a second tap while it
// worked. Dispatching that I/O to a background thread (Backup.kt, 2026-07-30)
// removes that guarantee — pick a zip, pick another before the first finishes,
// and the second wipes the first's staged files out from under it.
//
// The failure is not loud. The first restore has already reported nothing; it
// then fails its own did-everything-unpack check and the reader is told
// "Restore failed" for a zip that was perfectly good, while the other one
// quietly won. So the serialization lives with the directory rather than in the
// UI, and this is what holds it there.

import dev.plumbline.ui.restoreZipInto
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class ConcurrentRestoreTest {
    @get:Rule val tmp = TemporaryFolder()

    private fun zipOf(vararg entries: Pair<String, String>): ByteArray {
        val bytes = ByteArrayOutputStream()
        ZipOutputStream(bytes).use { zip ->
            for ((name, body) in entries) {
                zip.putNextEntry(ZipEntry(name))
                zip.write(body.toByteArray())
                zip.closeEntry()
            }
        }
        return bytes.toByteArray()
    }

    @Test
    fun `a second restore waits instead of wiping the first one's staging`() {
        val home = tmp.newFolder("home")

        // The slow reader's zip: a small note, then a much bigger one, so the
        // stall below lands cleanly between the two.
        val firstNote = """{"format":"pure-note-v1","ref":"John 3:16","text":"first"}"""
        val bulk = """{"format":"pure-note-v1","ref":"Rom 8:1","text":"${"x".repeat(200_000)}"}"""
        val slowZip = zipOf("notes/john-3-16.json" to firstNote, "notes/rom-8-1.json" to bulk)
        val otherZip = zipOf("tags/mercy.json" to """{"name":"mercy"}""")

        // Stall the slow restore once its FIRST entry is fully staged — the exact
        // state in which a concurrent wipe destroys work already done.
        val stagedOne = File(home, ".restore-tmp/notes/john-3-16.json")
        val reached = CountDownLatch(1)
        val release = CountDownLatch(1)

        var slowError: Throwable? = null
        var slowCount = 0
        val slow = Thread {
            runCatching {
                slowCount = restoreZipInto(home) {
                    stallWhenStaged(slowZip, stagedOne, firstNote.length.toLong(), reached, release)
                }
            }.onFailure { slowError = it }
        }
        slow.start()
        check(reached.await(10, TimeUnit.SECONDS)) { "the first restore never staged an entry" }

        var otherError: Throwable? = null
        var otherCount = 0
        val other = Thread {
            runCatching {
                otherCount = restoreZipInto(home) { ByteArrayInputStream(otherZip) }
            }.onFailure { otherError = it }
        }
        other.start()
        // Under the lock this join must time out — the second restore is parked
        // on the monitor and cannot reach its own `deleteRecursively`. Without
        // it, the second restore runs to completion here and takes the first
        // one's staged file with it.
        other.join(1_000)
        release.countDown()

        slow.join(10_000)
        other.join(10_000)

        assertNull("the concurrent restore destroyed the first one's work: $slowError", slowError)
        assertNull("the second restore failed: $otherError", otherError)
        assertEquals("both of the first zip's notes should have landed", 2, slowCount)
        assertEquals("the second zip's tag should have landed", 1, otherCount)
        assertEquals(firstNote, File(home, "notes/john-3-16.json").readText())
        assertEquals(bulk, File(home, "notes/rom-8-1.json").readText())
        assertEquals("""{"name":"mercy"}""", File(home, "tags/mercy.json").readText())
        assertEquals("the staging dir must not survive a restore", false, File(home, ".restore-tmp").exists())
    }

    /** [bytes] as a stream that signals and parks the moment [staged] has reached
     *  [wholeSize] on disk — i.e. once the restore reading it has one entry
     *  completely unpacked, and not a byte earlier. */
    private fun stallWhenStaged(
        bytes: ByteArray,
        staged: File,
        wholeSize: Long,
        reached: CountDownLatch,
        release: CountDownLatch,
    ): InputStream = object : InputStream() {
        private var pos = 0
        private var parked = false

        private fun parkIfStaged() {
            if (parked || !staged.isFile || staged.length() < wholeSize) return
            parked = true
            reached.countDown()
            release.await()
        }

        override fun read(): Int {
            parkIfStaged()
            return if (pos < bytes.size) bytes[pos++].toInt() and 0xff else -1
        }

        override fun read(b: ByteArray, off: Int, len: Int): Int {
            parkIfStaged()
            if (pos >= bytes.size) return -1
            val n = minOf(len, bytes.size - pos)
            System.arraycopy(bytes, pos, b, off, n)
            pos += n
            return n
        }
    }
}
