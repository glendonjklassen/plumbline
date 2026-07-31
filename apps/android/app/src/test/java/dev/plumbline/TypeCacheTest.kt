package dev.plumbline

// The mechanism under the process-wide type cache (ui/Typography.kt).
//
// `Font("fonts/EBGaramond-Regular.ttf", assets)` is not a description of a font:
// AndroidAssetFont parses the TTF in its constructor, on whatever thread builds
// the family. That used to sit behind a `remember`, so the parse happened once
// per composition — five theme call sites, two of them on the boot path, and
// again after every Activity recreate. `Once` is what makes it once per process
// instead, and it is now raced by design: MainActivity warms it on a background
// dispatcher while the first composition may ask for it on the main thread.
//
// The FontFamily itself needs an AssetManager and cannot be built here, so what
// is pinned is the part that can be wrong without a compile error — the identity
// handed back, and what a race does to it.

import dev.plumbline.ui.Once
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.CyclicBarrier
import java.util.concurrent.atomic.AtomicInteger

class TypeCacheTest {

    @Test
    fun `the second caller gets the first caller's value`() {
        val once = Once<Any>()
        val builds = AtomicInteger()
        val first = once.get { builds.incrementAndGet(); Any() }
        val second = once.get { builds.incrementAndGet(); Any() }

        assertSame("a second ask re-parsed the fonts instead of reusing them", first, second)
        assertEquals("the family was built more than once", 1, builds.get())
    }

    /** The startup warm and the first composition, arriving together.
     *
     *  The builder sleeps, and that is the point: a real font parse takes long
     *  enough for every racing thread to walk past an unguarded null check, so
     *  the window is made wide enough that an unlocked cache is CERTAIN to build
     *  once per thread rather than merely likely to. */
    @Test
    fun `threads racing the first build still build once and share it`() {
        val once = Once<Any>()
        val builds = AtomicInteger()
        val racers = 8
        val start = CyclicBarrier(racers)
        val got = arrayOfNulls<Any>(racers)
        val thrown = Collections.synchronizedList(mutableListOf<Throwable>())

        val threads = (0 until racers).map { i ->
            Thread {
                runCatching {
                    start.await()
                    got[i] = once.get {
                        builds.incrementAndGet()
                        Thread.sleep(50)
                        Any()
                    }
                }.onFailure { thrown += it }
            }.also { it.start() }
        }
        threads.forEach { it.join(10_000) }

        assertTrue("a racing thread threw: $thrown", thrown.isEmpty())
        assertEquals("each racing thread parsed its own copy of the fonts", 1, builds.get())
        for (i in 1 until racers) {
            assertSame("thread $i was handed a different family than thread 0", got[0], got[i])
        }
    }
}
