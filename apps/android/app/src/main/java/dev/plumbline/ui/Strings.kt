package dev.plumbline.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.plumbline.PlumblineJson
import dev.plumbline.StudyEngine
import kotlinx.serialization.Serializable

// The Compose shell's half of `core::i18n` — the Kotlin twin of the web's
// lib/i18n.svelte.ts, filled the same way from the same two ABI calls.
//
// One table, read synchronously by every composable. `t()` is a plain function
// over a `mutableStateOf`, which is what makes a language change a recomposition
// rather than a relaunch: nothing subscribes and nothing has to be invalidated.
//
// THE TABLE IS NEVER PARTIAL. The engine resolves the reader's language over
// English key by key (crates/core/src/i18n.rs), so what arrives here answers
// every id that exists. `t()` still has an answer for an id that does not — the
// id itself, which is visible, greppable, and impossible to mistake for copy.
//
// Formatting mirrors `i18n::format`, including the part that looks like a bug: a
// `{placeholder}` with no argument is LEFT ON SCREEN rather than blanked,
// because "Read through {book}" missing its book reads like finished copy while
// the braces name the argument that went missing.

@Serializable
data class WireLanguage(val code: String = "", val endonym: String = "", val name: String = "")

@Serializable
data class WireCatalog(
    val lang: String = "en",
    val strings: Map<String, String> = emptyMap(),
    val languages: List<WireLanguage> = emptyList(),
    val nativeIntros: Boolean = true,
)

object Strings {
    private var table by mutableStateOf<Map<String, String>>(emptyMap())
    private var code by mutableStateOf("en")
    private var choices by mutableStateOf<List<WireLanguage>>(emptyList())
    private var intros by mutableStateOf(true)

    /** The language being painted, as a code. */
    val lang: String get() = code

    /** Whether the first-run welcome and the curious path may be OFFERED in the
     *  language being painted.
     *
     *  Those two screens are somebody speaking to a reader about their own life
     *  — which idioms land, which questions are the live ones — so they are
     *  written by someone inside that culture or they are not written, and a
     *  reader is never led into them in a language nobody has written them in.
     *  The engine decides (`i18n::Lang::has_native_intros`); this carries the
     *  answer. Web twin: `hasNativeIntros()` in lib/i18n.svelte.ts.
     *
     *  Defaults TRUE because the default painted language is English, which is
     *  the language the prose is written in; the engine's answer replaces it
     *  before anything renders (`load()` runs in onCreate). */
    val hasNativeIntros: Boolean get() = intros

    /** Every language this build ships, each labelled in ITSELF — a reader
     *  looking for German is looking for "Deutsch", and they are looking for it
     *  on a screen they may not be able to read a word of. */
    val languages: List<WireLanguage> get() = choices

    /**
     * Load the catalogue and tell the ENGINE which language to write in.
     *
     * BOTH, and in this order, because they cover different halves. The
     * catalogue is what this shell spells; `SetLanguage` is what the core spells
     * — every book name and reference it hands back, in the table of contents,
     * search hits, weave endpoints, note headers, the reading map. A shell that
     * did only the first gets a German interface listing a book called Genesis,
     * which reads as broken rather than as a setting.
     *
     * Called from `onCreate` BEFORE the engine opens, so nothing has read a book
     * name yet.
     *
     * `chosen` is the reader's setting (empty means follow the device) and
     * `device` the platform locale; `i18n::resolve` in the core decides between
     * them, so the two shells cannot disagree about the rule.
     */
    fun load(chosen: String, device: String) {
        StudyEngine.SetLanguage(chosen, device)
        val cat = runCatching {
            PlumblineJson.decodeFromString<WireCatalog>(StudyEngine.CatalogJson(chosen, device))
        }.getOrNull() ?: return
        table = cat.strings
        code = cat.lang
        choices = cat.languages
        intros = cat.nativeIntros
    }

    /**
     * Seed the table directly. FOR TESTS ONLY, and it exists because of a real
     * gap: a JVM unit test has no native library, so `load()` cannot run and
     * every `t()` would answer with its own id — a test that asserted on copy
     * would fail for a reason that has nothing to do with what it is testing.
     * The tests read `crates/core/src/i18n/en.json` and hand it here, so they
     * check the SHIPPED string rather than a second copy of it.
     */
    fun seedForTest(strings: Map<String, String>) {
        table = strings
    }

    /** Fill `{placeholders}`; an unfilled one stays visible. */
    fun fill(template: String, args: Array<out Pair<String, Any?>>): String {
        if (!template.contains('{')) return template
        var out = template
        for ((name, value) in args) {
            if (value != null) out = out.replace("{$name}", value.toString())
        }
        return out
    }

    /** One string, in the reader's language. */
    fun t(id: String, vararg args: Pair<String, Any?>): String {
        val s = table[id] ?: return id
        return if (args.isEmpty()) s else fill(s, args)
    }

    /**
     * Pick between a one-form and a many-form key, lending both `n`.
     *
     * Deliberately not a plural engine — see `i18n::plural`. English and German
     * split exactly one/other; a language with more forms needs CLDR rules and
     * this function replaced, not extended.
     */
    fun plural(idOne: String, idOther: String, n: Int, vararg args: Pair<String, Any?>): String =
        t(if (n == 1) idOne else idOther, "n" to n, *args)
}

/** Shorthand, so a composable reads `t("nav.read")` and not `Strings.t(...)`. */
fun t(id: String, vararg args: Pair<String, Any?>): String = Strings.t(id, *args)
