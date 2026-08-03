package dev.plumbline

import dev.plumbline.ui.Strings
import kotlinx.serialization.json.Json
import java.io.File

/**
 * The English catalogue, for JVM unit tests.
 *
 * A unit test runs with no native library, so `Strings.load()` — which asks the
 * engine — cannot run, and every `t()` would answer with its own id. Any test
 * that asserts on what a reader sees would then fail for a reason unrelated to
 * what it is checking.
 *
 * Read from `crates/core/src/i18n/en.json` rather than restated here, so a test
 * that says "the reader is told why it did not save" is checking the sentence
 * that actually ships. A missing file is a hard failure: silently leaving the
 * table empty would turn every such test into one that passes on ids.
 */
fun useEnglishCatalogue() {
    val here = File("").absoluteFile // apps/android/app when gradle runs tests
    val json = generateSequence(here) { it.parentFile }
        .map { File(it, "crates/core/src/i18n/en.json") }
        .firstOrNull { it.isFile }
        ?: error("could not find crates/core/src/i18n/en.json from ${here.absolutePath}")
    Strings.seedForTest(Json.decodeFromString<Map<String, String>>(json.readText()))
}
