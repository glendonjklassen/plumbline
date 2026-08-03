package dev.plumbline

import org.junit.Assert.assertTrue
import java.io.File
import org.junit.Test

/**
 * Every callback a composable takes with a `{}` default is actually forwarded.
 *
 * WHY THIS IS A TEST AND NOT A CODE REVIEW: a defaulted lambda parameter is
 * invisible when you forget it. `PlumblineApp` grew `onLanguage: (String) -> Unit
 * = {}`, `StudyScreen` grew the same, and the call between them was positional
 * and never updated — so the compiler was satisfied, lint was satisfied, every
 * unit test passed, and switching to Deutsch on a real phone closed the dialog
 * and did nothing at all (UAT, 2026-08-03). There is no type error to catch
 * because the default IS a valid argument.
 *
 * A SOURCE TEST, deliberately. The alternative is an instrumented Compose test
 * per callback, which needs a device this project's rules say not to drive
 * (CLAUDE.md §UI testing) and would still only cover the callbacks somebody
 * remembered to write one for. This covers all of them, cheaply, in the JVM
 * suite that already runs on every build.
 */
class CallbackWiringTest {
    /** Walks up to the repo root, the way CatalogForTest.kt does — gradle's CWD
     *  for unit tests is `apps/android/app`, but that is not something to rely on. */
    private fun src(name: String): String {
        val rel = "apps/android/app/src/main/java/dev/plumbline/ui/$name"
        val f = generateSequence(File("").absoluteFile) { it.parentFile }
            .map { File(it, rel) }
            .firstOrNull { it.isFile }
            ?: error("could not find $rel from ${File("").absolutePath}")
        return f.readText()
    }

    /** `name: (…) -> Unit = {}` — a callback whose absence is silent. */
    private val defaulted = Regex("""\b(on[A-Z]\w*)\s*:\s*\([^)]*\)\s*->\s*Unit\s*=\s*\{\s*}""")

    @Test
    fun everyDefaultedCallbackOnStudyScreenIsForwardedFromPlumblineApp() {
        val text = src("StudyScreen.kt")

        // The `fun StudyScreen(` parameter list.
        val open = text.indexOf("fun StudyScreen(")
        assertTrue("StudyScreen not found — did it move?", open > 0)
        val params = text.substring(open, text.indexOf(") {", open))
        val callbacks = defaulted.findAll(params).map { it.groupValues[1] }.toList()
        assertTrue("no defaulted callbacks found; the pattern must have drifted", callbacks.isNotEmpty())

        // The call site inside PlumblineApp.
        val callAt = text.indexOf("StudyScreen(", text.indexOf("fun PlumblineApp("))
        assertTrue("PlumblineApp no longer calls StudyScreen", callAt > 0)
        val call = text.substring(callAt, text.indexOf("\n        )", callAt) + 10)

        for (cb in callbacks) {
            assertTrue(
                "StudyScreen takes `$cb` with a no-op default and PlumblineApp does not pass it, " +
                    "so the feature behind it silently does nothing. Add `$cb = …` to the call.\n" +
                    "call site was:\n$call",
                call.contains("$cb ="),
            )
        }
    }

    /**
     * And the whole reason that call is written with NAMED arguments: a
     * positional call cannot be checked this way, and a positional call is how
     * the bug happened.
     */
    @Test
    fun plumblineAppCallsStudyScreenByName() {
        val text = src("StudyScreen.kt")
        val callAt = text.indexOf("StudyScreen(", text.indexOf("fun PlumblineApp("))
        val call = text.substring(callAt, text.indexOf("\n        )", callAt))
        assertTrue(
            "PlumblineApp calls StudyScreen positionally. Name the arguments — it is the only way " +
                "a forgotten defaulted callback shows up as anything at all.\n$call",
            call.contains("engine =") && call.contains("palette ="),
        )
    }
}
