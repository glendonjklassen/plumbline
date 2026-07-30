package dev.plumbline

import dev.plumbline.ui.restoreDestination
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The temp-name rule, Kotlin leg. `store::is_temp_name` in the core states the
 * same rule for Rust and `collectFiles` states it for the web; all three must
 * accept exactly `.<stem>.<digits>.tmp`.
 *
 * Android is the shell that actually strands these — a process kill between
 * write and rename is ordinary here — so this leg is the one that matters most,
 * and it guards both directions: the backup walk must not ship one, and a
 * restore must not plant one from an older zip that did.
 *
 * The negative cases are the whole difficulty. A rule one character wider drops
 * the reader's own data, permanently and silently, which is worse than the bug
 * it fixes.
 */
class TempNameTest {
    @Test
    fun `every name the writer mints is recognised`() {
        // writeThroughTemp mints ".<dest name>.<tempSeq>.tmp"; the core's native
        // minter uses a pid and the wasm one a counter. All three are digits.
        assertTrue(isTempName(".romans-road.json.1.tmp"))
        assertTrue(isTempName(".romans-road.json.4242.tmp"))
        assertTrue(isTempName(".out.9.tmp"))
        // A stem may keep its own dots — a note file is "Gen.1.7.json".
        assertTrue(isTempName(".Gen.1.7.json.4242.tmp"))
    }

    @Test
    fun `nothing the reader owns looks like a temp`() {
        // Each of these is rescued by a DIFFERENT leg of the rule, which is why
        // all three legs have to be there.
        assertFalse("`.config` is a real dotted user dir", isTempName(".config"))
        assertFalse("the config rescue file must ride in backups", isTempName("config.json.bad"))
        assertFalse("a reader may name a thread `notes.tmp`", isTempName("notes.tmp"))
        assertFalse("no digit discriminator", isTempName(".summer.tmp"))
        assertFalse("no digit discriminator", isTempName(".summer.json.v2.tmp"))
        assertFalse("ordinary authored file", isTempName("romans-road.json"))
        assertFalse("empty stem", isTempName(".4242.tmp"))
        assertFalse("nothing at all", isTempName(""))
    }

    @Test
    fun `a restore refuses a stranded temp from an older zip`() {
        // Zips written before the walk learned to skip temps still carry them;
        // refusing on the way in is what makes the fix retroactive.
        assertNull(restoreDestination("threads/.romans-road.json.4242.tmp"))
        assertNull(restoreDestination(".config/plumbline/.config.json.7.tmp"))
        // …and the entries beside it still restore.
        assertEquals("threads/romans-road.json", restoreDestination("threads/romans-road.json"))
        assertEquals(
            "plumbline/config.json",
            restoreDestination(".config/plumbline/config.json"),
        )
    }
}
