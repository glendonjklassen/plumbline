// A laid-out chapter is native memory. Who frees it, and when?
//
// ReaderPane used to lay the chapter out in a withContext block and assign the
// handle after it returned. That path is only taken when nothing goes wrong, and
// the thing that goes wrong most is CANCELLATION: turning a page re-keys the
// layout effect, which cancels the layout in flight, and the native call cannot
// be interrupted — so the layout completes, withContext throws on the way out,
// and the assignment that would have owned the handle never runs. One leaked
// native display list per fast chapter turn. A throwing JSON parse leaked one the
// same way, from inside a failed Result nothing held.
//
// publishOrClose has exactly two exits for a handle: publish takes it, or it is
// closed. These tests take every path through it with a handle that counts its
// own frees, which is the one thing a JVM test can say about native memory
// without a device.
//
// Plain JUnit + real threads (no coroutines-test on the unit-test classpath): the
// latch standing in for the native call is BLOCKING on purpose, because that is
// exactly why cancellation is late.

package dev.plumbline

import dev.plumbline.ui.publishOrClose
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.Closeable
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Stands in for `Chapter`: the only thing that releases the native layout is
 *  close(), so all this has to do is count how often that happened. */
private class FakeHandle : Closeable {
    var closes = 0
        private set

    override fun close() {
        closes++
    }
}

class NativeHandOffTest {

    @Test
    fun a_layout_that_lands_is_handed_over_and_not_freed() = runBlocking {
        val handle = FakeHandle()
        var published: FakeHandle? = null
        var value: String? = null
        val problems = ArrayList<Throwable>()

        publishOrClose<FakeHandle, String>(
            engineLock = Any(),
            acquire = { handle },
            derive = { "display list" },
            publish = { h, v -> published = h; value = v },
            onProblem = { problems.add(it) },
        )

        assertSame("the pane must get the handle the layout allocated", handle, published)
        assertEquals("display list", value)
        assertEquals("a handle the pane now owns must NOT be freed here", 0, handle.closes)
        assertTrue(problems.isEmpty())
    }

    /** The layout and the parse must see one another's engine state and nobody
     *  else's — two panes lay out concurrently. */
    @Test
    fun the_engine_lock_is_held_across_the_layout_and_the_parse() = runBlocking {
        val engineLock = Any()
        var heldWhileLayingOut = false
        var heldWhileParsing = false

        publishOrClose<FakeHandle, String>(
            engineLock = engineLock,
            acquire = { heldWhileLayingOut = Thread.holdsLock(engineLock); FakeHandle() },
            derive = { heldWhileParsing = Thread.holdsLock(engineLock); "display list" },
            publish = { _, _ -> },
            onProblem = { throw it },
        )

        assertTrue("the layout must run under the engine lock", heldWhileLayingOut)
        assertTrue("the parse reads the same handle, so it must too", heldWhileParsing)
    }

    /**
     * The headline case: a reader turning pages faster than the core lays them
     * out. The layout effect is cancelled with the native call already running,
     * and nothing in the coroutine will ever see its result.
     */
    @Test
    fun a_cancelled_layout_frees_the_handle_it_allocated() {
        val handle = FakeHandle()
        val laying = CountDownLatch(1)   // the native call has started
        val finish = CountDownLatch(1)   // …and may now return
        var published = 0
        val problems = ArrayList<Throwable>()

        // A dispatcher standing in for the main thread a LaunchedEffect body runs
        // on, so withContext(Dispatchers.Default) inside publishOrClose is a real
        // thread switch, as it is on device.
        val main = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        try {
            val job = CoroutineScope(main).launch {
                publishOrClose<FakeHandle, String>(
                    engineLock = Any(),
                    acquire = {
                        // plumbline_layout_chapter: not interruptible. It will
                        // finish and return a handle no matter what the coroutine
                        // has been told in the meantime.
                        laying.countDown()
                        assertTrue(finish.await(10, TimeUnit.SECONDS))
                        handle
                    },
                    derive = { "display list" },
                    publish = { _, _ -> published++ },
                    onProblem = { problems.add(it) },
                )
            }

            assertTrue("the layout never started", laying.await(10, TimeUnit.SECONDS))
            job.cancel()          // the reader turned the page
            finish.countDown()    // the native call returns anyway
            runBlocking { job.join() }

            assertEquals("a cancelled layout must not reach the pane", 0, published)
            assertEquals(
                "the cancelled layout's native handle was never freed — this is the leak",
                1, handle.closes,
            )
            assertTrue(
                "cancellation is not a layout error and must not be reported as one: $problems",
                problems.isEmpty(),
            )
        } finally {
            main.close()
        }
    }

    /**
     * Cancellation must still come OUT of publishOrClose. Swallowing it would
     * leave the effect running — the statement after the call is the rest of a
     * layout the reader has already navigated away from — and would tell a
     * coroutine scope a cancelled child completed.
     */
    @Test
    fun a_cancelled_layout_stays_cancelled() {
        val handle = FakeHandle()
        val laying = CountDownLatch(1)
        val finish = CountDownLatch(1)
        var continuedAfter = false
        val main = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        try {
            val job = CoroutineScope(main).launch {
                publishOrClose<FakeHandle, String>(
                    engineLock = Any(),
                    acquire = { laying.countDown(); finish.await(10, TimeUnit.SECONDS); handle },
                    derive = { "display list" },
                    publish = { _, _ -> },
                    onProblem = { },
                )
                // Unreachable: nothing after a cancelled layout may run. There is
                // no suspension point between here and the call, so a swallowed
                // CancellationException lands right here.
                continuedAfter = true
            }
            assertTrue(laying.await(10, TimeUnit.SECONDS))
            job.cancel()
            finish.countDown()
            runBlocking { job.join() }
            assertTrue("the job must report itself cancelled", job.isCancelled)
            assertFalse(
                "cancellation was swallowed — the effect carried on working after the reader " +
                    "turned the page",
                continuedAfter,
            )
        } finally {
            main.close()
        }
    }

    /** The wire JSON came back unparseable: the handle exists, and used to sit
     *  inside a failed Result that nothing held. */
    @Test
    fun a_parse_that_throws_frees_the_handle_the_layout_allocated() = runBlocking {
        val handle = FakeHandle()
        var published = 0
        val problems = ArrayList<Throwable>()

        publishOrClose<FakeHandle, String>(
            engineLock = Any(),
            acquire = { handle },
            derive = { throw IllegalStateException("bad wire") },
            publish = { _, _ -> published++ },
            onProblem = { problems.add(it) },
        )

        assertEquals(0, published)
        assertEquals("the parse failed with the handle in hand — free it", 1, handle.closes)
        assertEquals(listOf("bad wire"), problems.map { it.message })
    }

    /** Nothing was allocated, so there is nothing to free — and the reader must
     *  still be told. */
    @Test
    fun a_layout_that_fails_before_allocating_reports_and_frees_nothing() = runBlocking {
        var published = 0
        val problems = ArrayList<Throwable>()

        publishOrClose<FakeHandle, String>(
            engineLock = Any(),
            acquire = { throw IllegalStateException("no such chapter") },
            derive = { "display list" },
            publish = { _, _ -> published++ },
            onProblem = { problems.add(it) },
        )

        assertEquals(0, published)
        assertEquals(listOf("no such chapter"), problems.map { it.message })
    }

    /** Ownership passes only when publish RETURNS. If it throws part-way the
     *  handle is still ours to free. */
    @Test
    fun a_publish_that_throws_leaves_the_handle_to_be_freed() = runBlocking {
        val handle = FakeHandle()
        val problems = ArrayList<Throwable>()

        publishOrClose<FakeHandle, String>(
            engineLock = Any(),
            acquire = { handle },
            derive = { "display list" },
            publish = { _, _ -> throw IllegalStateException("state write failed") },
            onProblem = { problems.add(it) },
        )

        assertEquals("publish did not complete, so the handle was never handed over", 1, handle.closes)
        assertEquals(listOf("state write failed"), problems.map { it.message })
    }
}
