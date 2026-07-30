// What is allowed to trigger a chapter layout, and what must happen to the
// handle it allocates.
//
// The two bugs this file guards were both WIRING, not logic: a `Slider` whose
// onValueChange pushed the value up (one native chapter layout per tick, ~120 a
// drag), and a `LayoutChapter` call whose handle was assigned after the
// withContext it ran in (dropped on the floor every time the effect was
// cancelled). NativeHandOffTest and SliderDraftTest pin the two mechanisms; only
// the call sites can put the bugs back, and a call site is composition — which
// this module cannot enter on the JVM (compose-ui-test is androidTest-only, and
// there is no Robolectric on the unit-test classpath; adding either is a
// build-file change).
//
// So this reads the source it guards. That is a blunt instrument and it is
// deliberate: it is the only thing here that fails when the wiring regresses,
// and the wiring is what regressed. Each assertion names the cost it is
// preventing, so a failure reads as the bug and not as a style complaint.

package dev.plumbline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class LayoutTriggerTest {

    /**
     * The three reader-pref sliders (text size, margin, line spacing) are all
     * layout inputs to ReaderPane, so a value that leaves this dialog costs a
     * native chapter layout. The thumb may move at pointer rate; the value may
     * not leave until the drag ends.
     */
    @Test
    fun the_reader_pref_sliders_hand_their_value_up_on_release_not_per_tick() {
        val src = sourceOf("ui/StudyScreen.kt")
        val sliders = callBlocks(src, "Slider")
        assertEquals(
            "StudyScreen has ${sliders.size} sliders, not the 3 reader prefs this guard knows " +
                "about — a new one is a new per-tick layout unless it drafts too",
            3, sliders.size,
        )
        for (slider in sliders) {
            val thumb = plainArg(slider, "value")
            val live = lambdaArg(slider, "onValueChange")
            val released = lambdaArg(slider, "onValueChangeFinished")
            assertTrue(
                "the thumb must show the DRAFT: a slider reading the committed pref cannot " +
                    "move under the finger, because the draft is what is holding the value " +
                    "back:\nvalue = $thumb",
                thumb != null && PREFS.none { it in thumb },
            )
            assertTrue(
                "a reader-pref slider with no onValueChangeFinished pushes every tick: " +
                    "~120 orphaned chapter layouts per two-second drag\n$slider",
                released != null,
            )
            assertTrue(
                "onValueChange must only move the thumb — this one reaches the reader's " +
                    "layout mid-drag, which is one native chapter layout per tick:\n$live",
                live != null && PUSHES.none { it in live },
            )
            assertTrue(
                "onValueChangeFinished must hand the drafted value up, or the setting the " +
                    "reader chose never reaches the page:\n$released",
                released != null && PUSHES.any { it in released },
            )
        }
    }

    /**
     * A laid-out chapter is native memory (see NativeHandOffTest). The layout
     * effect is cancelled routinely — every fast chapter turn — so the handle may
     * only be allocated somewhere that frees it on the way out.
     */
    @Test
    fun every_chapter_layout_allocates_inside_publish_or_close() {
        val src = sourceOf("ui/ReaderPane.kt")
        val allocations = occurrences(src, "LayoutChapter")
        assertEquals(
            "ReaderPane must allocate the chapter exactly once; ${allocations.size} sites means " +
                "one of them is unguarded or this guard is looking at the wrong file",
            1, allocations.size,
        )
        val guarded = callBlocks(src, "publishOrClose").map { it.range }
        for (at in allocations) {
            assertTrue(
                "LayoutChapter at offset $at is outside publishOrClose: a cancelled turn " +
                    "(the common case) leaks the native display list it allocated",
                guarded.any { at in it },
            )
        }
    }

    private companion object {
        /** The three callbacks that reach the reader's config — and so the pane's
         *  layout effect. Naming one inside onValueChange IS the bug. */
        private val PUSHES = listOf("onBodySize", "onSideMargin", "onLineSpacing")

        /** The committed prefs. A thumb bound to one of these is a thumb that
         *  cannot move while the draft holds the value back. */
        private val PREFS = listOf("bodySize", "sideMargin", "lineSpacing")

        /** Read a shell source file. Walks up from the test's working directory
         *  (Gradle: the `app` project dir) so it works from the module or the
         *  repo root, and fails loudly rather than skipping — a guard that
         *  quietly finds nothing guards nothing. */
        private fun sourceOf(relative: String): Source {
            val tail = "src/main/java/dev/plumbline/$relative"
            var dir: File? = File("").absoluteFile
            while (dir != null) {
                for (prefix in listOf("", "app/", "apps/android/app/")) {
                    val f = File(dir, prefix + tail)
                    if (f.isFile) return Source(f.readText())
                }
                dir = dir.parentFile
            }
            throw AssertionError(
                "cannot find $tail from ${File("").absolutePath} — this guard reads the source " +
                    "it guards, so a source it cannot read is a failure, not a pass",
            )
        }
    }
}

/** Kotlin source with the code/not-code question already answered. */
private class Source(val text: String) {
    /** True where [text]'s character is code — not inside a string literal, a
     *  line comment or a block comment. Brace and paren balancing has to skip
     *  those: `"Night (true black)"` and `"${'$'}{p.chapter}"` are not structure.
     *  Char literals are NOT handled (neither file has a bracket in one); one
     *  would unbalance a scan, which fails this test rather than passing it. */
    val isCode: BooleanArray = BooleanArray(text.length) { true }

    init {
        var i = 0
        fun blank(from: Int, to: Int) { for (k in from until to.coerceAtMost(text.length)) isCode[k] = false }
        while (i < text.length) {
            when {
                text.startsWith("//", i) -> {
                    val end = text.indexOf('\n', i).let { if (it < 0) text.length else it }
                    blank(i, end); i = end
                }
                text.startsWith("/*", i) -> {
                    val end = text.indexOf("*/", i).let { if (it < 0) text.length else it + 2 }
                    blank(i, end); i = end
                }
                text.startsWith("\"\"\"", i) -> {
                    val end = text.indexOf("\"\"\"", i + 3).let { if (it < 0) text.length else it + 3 }
                    blank(i, end); i = end
                }
                text[i] == '"' -> {
                    var j = i + 1
                    while (j < text.length && text[j] != '"') j += if (text[j] == '\\') 2 else 1
                    blank(i, j + 1); i = j + 1
                }
                else -> i++
            }
        }
    }

    fun codeIndexOf(needle: String, from: Int): Int {
        var at = text.indexOf(needle, from)
        while (at >= 0 && !isCode[at]) at = text.indexOf(needle, at + 1)
        return at
    }
}

/** One call in a file: where it sits, and the source it came from. */
private class Call(val src: Source, val range: IntRange) {
    val text: String get() = src.text.substring(range.first, range.last + 1)
    override fun toString() = text
}

/** Every `name(...)` call in [src], parens balanced, comments and strings skipped. */
private fun callBlocks(src: Source, name: String): List<Call> {
    val out = ArrayList<Call>()
    var from = 0
    while (true) {
        var at = src.codeIndexOf("$name(", from)
        // `name` must be a whole identifier: RangeSlider( is not Slider(.
        while (at > 0 && (src.text[at - 1].isLetterOrDigit() || src.text[at - 1] == '_')) {
            at = src.codeIndexOf("$name(", at + 1)
        }
        if (at < 0) return out
        var depth = 0
        var i = at + name.length
        while (i < src.text.length) {
            if (src.isCode[i]) {
                if (src.text[i] == '(') depth++
                if (src.text[i] == ')' && --depth == 0) break
            }
            i++
        }
        require(depth == 0) { "unbalanced $name( at $at" }
        out.add(Call(src, at..i))
        from = i + 1
    }
}

/** Offsets of every `name(` call, as [callBlocks] finds them. */
private fun occurrences(src: Source, name: String): List<Int> =
    callBlocks(src, name).map { it.range.first }

/** The text of a `name = …` argument up to the end of its line, or null if absent.
 *  Enough for the one-expression arguments this file asks about. */
private fun plainArg(call: Call, name: String): String? =
    Regex("""(?<![A-Za-z0-9_])$name\s*=\s*([^,\n]+)""").find(call.text)?.groupValues?.get(1)?.trim()

/** The body of a `name = { … }` argument, braces balanced, or null if absent.
 *  `onValueChange` does not match `onValueChangeFinished`: the `=` has to follow
 *  the name. */
private fun lambdaArg(call: Call, name: String): String? {
    val m = Regex("""(?<![A-Za-z0-9_])$name\s*=\s*\{""").find(call.text) ?: return null
    val src = call.src
    var depth = 0
    var i = call.range.first + m.range.last     // the '{', in file offsets
    val start = i
    while (i <= call.range.last) {
        if (src.isCode[i]) {
            if (src.text[i] == '{') depth++
            if (src.text[i] == '}' && --depth == 0) return src.text.substring(start, i + 1)
        }
        i++
    }
    return null
}
