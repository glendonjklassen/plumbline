package dev.plumbline

// A backup zip is untrusted input: it arrives through SAF from anywhere the
// reader can reach, and restore used to stream its entries straight over the
// live home. The vetting is now a pure function, so the decisions it makes are
// pinned here rather than left to a device test nobody runs.

import dev.plumbline.ui.restoreDestination
import dev.plumbline.ui.restoreZipInto
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class RestorePlanTest {
    @get:Rule val tmp = TemporaryFolder()

    /** A zip in memory: entry name → contents, in order. */
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

    private fun home(): File = tmp.newFolder("home")

    private fun seed(home: File, rel: String, body: String): File =
        File(home, rel).apply { parentFile?.mkdirs(); writeText(body) }

    /** [n] chars deflate can do nothing with. Seeded, so the archive is the same
     *  size on every run and "cut it in half" always lands mid-entry. */
    private fun noise(n: Int): String {
        val rng = java.util.Random(1769)
        val alphabet = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/"
        return String(CharArray(n) { alphabet[rng.nextInt(alphabet.length)] })
    }

    @Test
    fun ordinaryEntriesLandWhereTheyAre() {
        assertEquals("tags/foo.json", restoreDestination("tags/foo.json"))
        assertEquals("threads/romans-road.json", restoreDestination("threads/romans-road.json"))
        // Nested paths inside an authored dir are kept whole.
        assertEquals("notes/Gen/1.json", restoreDestination("notes/Gen/1.json"))
        // The six authored dirs, spelled out so the set can't quietly widen.
        for (d in listOf("tags", "threads", "weaves", "notes", "memory", "reading")) {
            assertEquals("$d/x.json", restoreDestination("$d/x.json"))
        }
    }

    @Test
    fun traversalIsRefused() {
        assertNull("`..` above the home must be refused", restoreDestination("../../etc/x"))
        assertNull("`..` out of an authored dir must be refused", restoreDestination("tags/../../etc/x"))
        assertNull(restoreDestination("tags/../plumbline/config.json"))
        assertNull(restoreDestination(".config/../../etc/x"))
        // A Windows-authored archive separates with backslashes; those segments
        // get vetted too, not waved through as one long filename.
        assertNull("backslash traversal must be refused", restoreDestination("tags/..\\..\\etc\\x"))
        assertNull(restoreDestination("tags\\..\\..\\etc\\x"))
        assertNull(restoreDestination("./tags/foo.json"))
    }

    @Test
    fun absolutePathsAreRefused() {
        assertNull("an absolute path must be refused", restoreDestination("/etc/x"))
        assertNull(restoreDestination("/tags/foo.json"))
        assertNull(restoreDestination("\\tags\\foo.json"))
        assertNull(restoreDestination("C:/Windows/x"))
    }

    @Test
    fun dirsOutsideTheUserSubtreeAreRefused() {
        assertNull("the data pack is not restorable", restoreDestination("data/kjv.jsonl"))
        assertNull(restoreDestination("bridge/abbott-smith.json"))
        // The live config dir only arrives under ".config/", never bare.
        assertNull(restoreDestination("plumbline/config.json"))
        // Root-level files (the backup manifest) restore nothing.
        assertNull(restoreDestination("plumbline-backup.json"))
        // A near-miss on an authored dir name is still outside.
        assertNull(restoreDestination("tagsy/foo.json"))
    }

    @Test
    fun directoryAndEmptyEntriesAreRefused() {
        assertNull(restoreDestination("tags/"))
        assertNull(restoreDestination("tags//foo.json"))
        assertNull(restoreDestination(".config/"))
        assertNull(restoreDestination(""))
    }

    @Test
    fun configTravelsUnderDotConfigAndTheLegacyPrefixIsRemapped() {
        assertEquals(
            "plumbline/config.json",
            restoreDestination(".config/plumbline/config.json"),
        )
        // Zips written before the Plumbline rename carry "pure-study/".
        assertEquals(
            "plumbline/config.json",
            restoreDestination(".config/pure-study/config.json"),
        )
    }

    @Test
    fun aGoodZipReplacesFilesAndLeavesNothingBehind() {
        val home = home()
        seed(home, "tags/keep.json", "old")
        val n = restoreZipInto(home) {
            ByteArrayInputStream(
                zipOf(
                    "tags/keep.json" to "new",
                    "threads/added.json" to "thread",
                    ".config/pure-study/config.json" to "cfg",
                    "plumbline-backup.json" to "{}", // the manifest is not restored
                ),
            )
        }
        assertEquals(3, n)
        assertEquals("new", File(home, "tags/keep.json").readText())
        assertEquals("thread", File(home, "threads/added.json").readText())
        assertEquals("cfg", File(home, "plumbline/config.json").readText())
        assertFalse("the staging dir must be gone", File(home, ".restore-tmp").exists())
        assertTrue(
            "no temp files may be left in the authored dirs",
            File(home, "tags").list()!!.none { it.startsWith(".") || it.endsWith(".tmp") },
        )
    }

    /** The whole point of A-01: a zip that goes bad halfway must not have
     *  touched the live home at all. */
    @Test
    fun aTruncatedZipLeavesTheLiveHomeUntouched() {
        val home = home()
        seed(home, "tags/keep.json", "old")
        // Entry one is complete and would have been written by the old
        // stream-straight-over-the-home code; entry two is cut off mid-stream.
        // Its body has to be incompressible, or deflate shrinks the archive to a
        // few hundred bytes and there is nothing left to cut in half.
        val whole = zipOf("tags/keep.json" to "new", "threads/big.json" to noise(40_000))
        val truncated = whole.copyOfRange(0, whole.size / 2)

        val outcome = runCatching { restoreZipInto(home) { ByteArrayInputStream(truncated) } }
        assertTrue("a truncated zip must fail loudly", outcome.isFailure)
        assertEquals(
            "the live file must still hold its old contents",
            "old",
            File(home, "tags/keep.json").readText(),
        )
        assertFalse(File(home, "threads/big.json").exists())
        assertFalse("the staging dir must be gone", File(home, ".restore-tmp").exists())
    }

    @Test
    fun aTraversalEntryWritesNothingOutsideTheHome() {
        val home = home()
        val outside = File(home.parentFile, "pwned.json")
        val n = restoreZipInto(home) {
            ByteArrayInputStream(
                zipOf("tags/../pwned.json" to "gotcha", "tags/ok.json" to "fine"),
            )
        }
        assertEquals(1, n)
        assertEquals("fine", File(home, "tags/ok.json").readText())
        assertFalse("nothing may be written above the home", outside.exists())
    }

    @Test
    fun aZipWithNothingOfOursRestoresNothing() {
        val home = home()
        assertEquals(
            0,
            restoreZipInto(home) { ByteArrayInputStream(zipOf("data/kjv.jsonl" to "not ours")) },
        )
        assertFalse(File(home, "data").exists())
        assertFalse(File(home, ".restore-tmp").exists())
    }
}
