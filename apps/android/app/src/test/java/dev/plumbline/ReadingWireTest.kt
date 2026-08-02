// The reading map's wire contract, from the Kotlin side.
//
// 2026-08-01: the dwell counting moved into the core (`reading::DwellTracker`,
// driven by `plumbline_engine_reading_tick_json`) and `ReadingSpec` was deleted
// from Wire.kt with it. It existed only to hand the shell's own tracker its
// thresholds, and the defaults it carried while the fetch was in flight were a
// second copy of numbers the core owns — `wordsPerMinute` was still 220f two days
// after the core moved to 300.
//
// The WIRE did not change: the core still puts a `spec` object on both reading
// payloads. So the thing to hold down is that dropping the model field left them
// decoding, which they do because [PlumblineJson] ignores unknown keys. Turn that
// off and this test is how you find out.
//
// The golden strings are real ABI answers.

package dev.plumbline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReadingWireTest {

    @Test
    fun the_book_grid_still_decodes_a_payload_that_carries_a_spec() {
        val books = parseWire<ReadingBooks>(
            """{"books":[{"book":"Gen","name":"Genesis","chapters":50,"words":38262,"read":2,""" +
                """"pct":0.04,"standing":"partial","glow":0.5,"days":3,"lastRead":"2026-07-29"}],""" +
                """"since":"2026-07-01","spec":{"wordsPerMinute":300.0,"completeAt":0.9,"freshDays":30,""" +
                """"staleDays":365,"graceSeconds":3.0,"tickSeconds":30.0,"idleSeconds":120.0}}""",
        )
        assertEquals(1, books.books.size)
        assertEquals("Gen", books.books[0].book)
        assertEquals("partial", books.books[0].standing)
        assertEquals(2, books.books[0].read)
        assertEquals("2026-07-01", books.since)
    }

    @Test
    fun the_chapter_grid_still_decodes_a_payload_that_carries_a_spec() {
        val chs = parseWire<ReadingChapters>(
            """{"book":"Ps","chapters":[{"chapter":23,"words":118,"pct":1.0,"standing":"read",""" +
                """"glow":0.0,"days":1,"lastRead":"2026-07-31"}],"since":"2026-07-01",""" +
                """"spec":{"wordsPerMinute":300.0,"completeAt":0.9,"freshDays":30,"staleDays":365,""" +
                """"graceSeconds":3.0,"tickSeconds":30.0,"idleSeconds":120.0}}""",
        )
        assertEquals("Ps", chs.book)
        assertEquals(23, chs.chapters[0].chapter)
        assertEquals("read", chs.chapters[0].standing)
    }

    /** What a tick answers when the core decides the banked seconds are worth
     *  writing down — the same payload `reading_record_json` has always given,
     *  which is why the tracker reacts to `completed` in one place. */
    @Test
    fun a_banked_tick_reads_as_a_completed_pass() {
        val out = parseWire<ReadingRecorded>(
            """{"book":"Jude","chapter":1,"pct":1.0,"completed":true,"lastRead":"2026-08-01"}""",
        )
        assertEquals("Jude", out.book)
        assertEquals(1, out.chapter)
        assertTrue("the shell says 'read through' off this and nothing else", out.completed)
        assertEquals("2026-08-01", out.lastRead)
    }

    /** Most ticks answer nothing at all; when one does answer without completing
     *  a pass, the tile just repaints. */
    @Test
    fun a_partial_pass_does_not_claim_to_be_read_through() {
        val out = parseWire<ReadingRecorded>("""{"book":"Lev","chapter":11,"pct":0.42,"completed":false}""")
        assertEquals(0.42f, out.pct, 1e-6f)
        assertEquals(false, out.completed)
        assertEquals(null, out.lastRead)
    }
}
