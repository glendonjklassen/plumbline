package dev.plumbline

// A backup zip written before the Plumbline rename carries the config under
// ".config/pure-study/"; the live home reads "plumbline/config.json". Restore
// remaps it (Backup.kt's `currentConfigDir`), and nothing tested that — so the
// shim sat one careless refactor away from dropping a reader's whole config, and
// that failure does not look like a failure: the restore says it worked, the
// activity recreates, and every setting is quietly back to default.
//
// Driven through `restoreZipInto` over a REAL directory rather than through the
// pure name mapping alone, because the mapping is only half the promise: the
// remapped entry has to survive staging and win the rename that publishes it
// over whatever config the device already had.

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

class LegacyRestoreTest {
    @get:Rule val tmp = TemporaryFolder()

    /** The reader's settings as an older build wrote them — the frozen camelCase
     *  wire keys, with a text size and a theme they would notice losing. */
    private val oldConfig =
        """{"studyMode":"full","bodySize":33,"theme":"night","copyStyle":"verseMarkdown"}"""

    /** One of the reader's own notes, named the way the store names them. */
    private val myNote =
        """{"format":"pure-note-v1","ref":"John 3:16","text":"kept","created":"2026-01-01T00:00:00Z"}"""

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

    /** Every file in [home], home-relative — so a failure says where the restore
     *  actually put things instead of dying on a missing file. */
    private fun restored(home: File): List<String> =
        home.walkTopDown().filter { it.isFile }.map { it.relativeTo(home).invariantSeparatorsPath }
            .sorted().toList()

    @Test
    fun `a pre-rename backup restores the config where the engine reads it`() {
        val home = home()
        val n = restoreZipInto(home) {
            ByteArrayInputStream(
                zipOf(
                    ".config/pure-study/config.json" to oldConfig,
                    "notes/john-3-16.json" to myNote,
                    "plumbline-backup.json" to """{"format":1,"app":"android"}""",
                ),
            )
        }
        assertEquals("the config and the note, not the manifest", 2, n)
        assertTrue(
            "a pre-rename backup's settings must land at plumbline/config.json — anywhere else and " +
                "the reader is told the restore worked, then finds their defaults. Restored: " +
                "${restored(home)}",
            File(home, "plumbline/config.json").isFile,
        )
        assertEquals(oldConfig, File(home, "plumbline/config.json").readText())
        // Nothing writes the old name back: this is a read shim, not a second
        // identity the app now has to keep in step.
        assertFalse("the legacy dir must not exist after a restore", File(home, "pure-study").exists())
        assertFalse(File(home, ".config").exists())
        // A modern-named entry in the same zip is untouched by the shim.
        assertEquals(myNote, File(home, "notes/john-3-16.json").readText())
    }

    @Test
    fun `a pre-rename config replaces the settings already on the device`() {
        // The live failure mode, spelled out: the legacy entry lands somewhere the
        // engine never opens, the config already on the device stays put, and the
        // reader's restored settings are simply the ones they were trying to
        // replace. A count of restored files does not catch that; this does.
        val home = home()
        seed(home, "plumbline/config.json", """{"bodySize":18,"theme":"light"}""")
        val n = restoreZipInto(home) {
            ByteArrayInputStream(zipOf(".config/pure-study/config.json" to oldConfig))
        }
        assertEquals(1, n)
        assertEquals(
            "the restore left the device's own config in place",
            oldConfig,
            File(home, "plumbline/config.json").readText(),
        )
        assertFalse(File(home, ".restore-tmp").exists())
    }

    @Test
    fun `both spellings of the config land on one file, and the modern one is unchanged`() {
        // Old zip and new zip must resolve to the same destination — otherwise the
        // shim has forked the config rather than migrated it. Spelled out rather
        // than compared to each other alone: two nulls are also "the same".
        assertEquals("plumbline/config.json", restoreDestination(".config/plumbline/config.json"))
        assertEquals(
            restoreDestination(".config/plumbline/config.json"),
            restoreDestination(".config/pure-study/config.json"),
        )
        val home = home()
        assertEquals(1, restoreZipInto(home) {
            ByteArrayInputStream(zipOf(".config/plumbline/config.json" to oldConfig))
        })
        assertEquals(oldConfig, File(home, "plumbline/config.json").readText())
    }

    @Test
    fun `the legacy prefix is remapped under dot-config only`() {
        // The shim is one moved directory, not a prefix rewrite. A root-level
        // "pure-study/" is outside the authored dirs and restores nothing —
        // widening the remap would start writing files the vetting never approved.
        assertNull(restoreDestination("pure-study/config.json"))
        assertNull(restoreDestination("pure-study/tags/foo.json"))
        val home = home()
        assertEquals(0, restoreZipInto(home) {
            ByteArrayInputStream(zipOf("pure-study/config.json" to oldConfig))
        })
        assertFalse(File(home, "plumbline").exists())
        assertFalse(File(home, "pure-study").exists())
    }
}
