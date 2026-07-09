// Kotlin/JNA binding to the pure-study C ABI — the whole "binding" for the
// Jetpack Compose (Android) shell. It binds the SAME flat C ABI as C# does
// (decision #1: one C ABI, thin native shells), via JNA. A Compose reader
// measures glyphs with Android's text engine, hands widths back through the
// `Measure` callback, paints the display-list JSON, and forwards taps to
// hit-test. No study logic lives in Kotlin.
//
// STATUS: scaffold. Building the aarch64 `libpure_ffi.so` needs the Android NDK
// (see PROGRESS.md); this file compiles against JNA inside the Gradle app once
// the .so is produced with cargo-ndk. UniFFI is the alternative for Kotlin, but
// it would own its own ABI rather than consume this shared C one, so JNA keeps
// the single-ABI design intact.
//
// Gradle deps: net.java.dev.jna:jna:5.14.0@aar  (bundles the .so per ABI under
// jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libpure_ffi.so).

package ca.cavallo.purestudy.core

import com.sun.jna.Callback
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.NativeLong
import com.sun.jna.Pointer
import com.sun.jna.PointerByReference
import com.sun.jna.Structure

/** Raw JNA view of the C ABI. Returned `Pointer`s to strings are owned by us
 *  and freed via [PureFfi.pure_study_string_free]; handles via their `*_free`. */
internal interface PureFfi : Library {
    fun pure_study_version(): Pointer
    fun pure_study_string_free(ptr: Pointer?)

    fun pure_engine_open(home: String, outErr: PointerByReference): Pointer?
    fun pure_engine_open_from_bytes(
        kjv: ByteArray, kjvLen: NativeLong,
        strongs: ByteArray, strongsLen: NativeLong,
        outErr: PointerByReference,
    ): Pointer?
    fun pure_engine_free(engine: Pointer?)

    fun pure_engine_toc_json(engine: Pointer): Pointer?
    fun pure_engine_chapter_count(engine: Pointer, book: String): Int
    fun pure_engine_verse_json(engine: Pointer, refKey: String): Pointer?
    fun pure_engine_token_json(engine: Pointer, refKey: String, tokenIndex: Int): Pointer?
    fun pure_engine_strongs_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_strongs_occurrences_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_search_json(engine: Pointer, query: String): Pointer?

    fun pure_engine_layout_chapter(
        engine: Pointer, book: String, chapter: Int,
        cfg: PureLayoutConfig.ByValue, measure: MeasureCallback, ctx: Pointer?,
    ): Pointer?
    fun pure_layout_to_json(dl: Pointer): Pointer?
    fun pure_layout_height(dl: Pointer): Float
    fun pure_layout_width(dl: Pointer): Float
    fun pure_layout_item_count(dl: Pointer): Int
    fun pure_layout_hit_test_json(dl: Pointer, x: Float, y: Float): Pointer?
    fun pure_layout_free(dl: Pointer?)

    companion object {
        val INSTANCE: PureFfi = Native.load("pure_ffi", PureFfi::class.java)
    }
}

/** Advance-width callback the shell backs with Android's text engine. */
fun interface MeasureCallback : Callback {
    fun invoke(ctx: Pointer?, text: Pointer?): Float
}

/** `#[repr(C)]` mirror of PureLayoutConfig; passed by value. */
@Structure.FieldOrder("width", "lineHeight", "spaceWidth", "verseNumGap", "paraIndent", "paraSpacing")
open class PureLayoutConfig : Structure() {
    @JvmField var width: Float = 0f
    @JvmField var lineHeight: Float = 0f
    @JvmField var spaceWidth: Float = 0f
    @JvmField var verseNumGap: Float = 0f
    @JvmField var paraIndent: Float = 0f
    @JvmField var paraSpacing: Float = 0f
    class ByValue : PureLayoutConfig(), Structure.ByValue
}

class PureStudyException(message: String) : Exception(message)

/** Token flag bits carried by display-list items and tokens (mirror the
 *  PURE_FLAG_* #defines in pure_study.h). */
object PureFlags {
    const val ADDED = 1   // supplied by the KJV translators (italic)
    const val DIVINE = 2  // the divine name
    const val TITLE = 4   // psalm superscription
    const val PARA = 8    // a paragraph mark (¶) precedes the word
}

/** Take ownership of a returned C string: decode UTF-8 and free it. */
private fun take(p: Pointer?): String? {
    if (p == null) return null
    val s = p.getString(0, "UTF-8")
    PureFfi.INSTANCE.pure_study_string_free(p)
    return s
}

/** The loaded study core. Call [close] (or use `use { }`) to free native memory. */
class StudyEngine private constructor(private val handle: Pointer) : AutoCloseable {
    companion object {
        private val ffi get() = PureFfi.INSTANCE

        fun open(home: String): StudyEngine {
            val err = PointerByReference()
            val h = ffi.pure_engine_open(home, err)
                ?: throw PureStudyException(take(err.value) ?: "could not open engine")
            return StudyEngine(h)
        }

        fun openFromBytes(kjv: ByteArray, strongs: ByteArray): StudyEngine {
            val err = PointerByReference()
            val h = ffi.pure_engine_open_from_bytes(
                kjv, NativeLong(kjv.size.toLong()),
                strongs, NativeLong(strongs.size.toLong()), err,
            ) ?: throw PureStudyException(take(err.value) ?: "could not open engine")
            return StudyEngine(h)
        }
    }

    fun tocJson(): String = take(ffi.pure_engine_toc_json(handle))!!
    fun chapterCount(book: String): Int = ffi.pure_engine_chapter_count(handle, book)
    fun verseJson(reference: String): String? = take(ffi.pure_engine_verse_json(handle, reference))
    fun tokenJson(reference: String, tokenIndex: Int): String? =
        take(ffi.pure_engine_token_json(handle, reference, tokenIndex))
    fun strongsJson(code: String): String? = take(ffi.pure_engine_strongs_json(handle, code))
    fun strongsOccurrencesJson(code: String): String? =
        take(ffi.pure_engine_strongs_occurrences_json(handle, code))
    fun searchJson(query: String): String? = take(ffi.pure_engine_search_json(handle, query))

    /** Lay out a chapter, measuring text with [measure]. Returns a [Chapter] to
     *  close; hit-test and paint off it. */
    fun layoutChapter(
        book: String,
        chapter: Int,
        cfg: PureLayoutConfig.ByValue,
        measure: (String) -> Float,
    ): Chapter {
        // Keep a strong ref to the callback for the duration of the call so JNA
        // does not collect it mid-layout.
        val cb = MeasureCallback { _, text -> measure(text?.getString(0, "UTF-8") ?: "") }
        val dl = ffi.pure_engine_layout_chapter(handle, book, chapter, cfg, cb, null)
            ?: throw PureStudyException("layout failed (null engine or callback)")
        return Chapter(dl)
    }

    override fun close() = ffi.pure_engine_free(handle)
}

/** A laid-out chapter (native display list). Paint from [json]; resolve taps
 *  with [hitTestJson]. Call [close] to free. */
class Chapter internal constructor(private val handle: Pointer) : AutoCloseable {
    private val ffi get() = PureFfi.INSTANCE
    val height: Float get() = ffi.pure_layout_height(handle)
    val width: Float get() = ffi.pure_layout_width(handle)
    val itemCount: Int get() = ffi.pure_layout_item_count(handle)
    fun json(): String = take(ffi.pure_layout_to_json(handle))!!
    fun hitTestJson(x: Float, y: Float): String? = take(ffi.pure_layout_hit_test_json(handle, x, y))
    override fun close() = ffi.pure_layout_free(handle)
}
