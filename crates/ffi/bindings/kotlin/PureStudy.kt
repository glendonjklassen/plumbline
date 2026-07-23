// Kotlin/JNA binding to the pure-study C ABI — the whole "binding" for the
// Jetpack Compose (Android) shell. It binds the SAME flat C ABI as C# does
// (decision #1: one C ABI, thin native shells), via JNA. A Compose reader
// measures glyphs with Android's text engine, hands widths back through the
// `Measure` callback, paints the display-list JSON, and forwards taps to
// hit-test. No study logic lives in Kotlin.
//
// The low-level [PureFfi] interface mirrors every function in
// `crates/ffi/include/pure_study.h` (78 of them); the safe [StudyEngine] /
// [Chapter] / [StudyConfig] wrappers mirror the method names + semantics of
// `crates/ffi/bindings/csharp/PureStudy.cs` (camelCased for Kotlin), so the
// two shells consume the identical surface.
//
// STATUS: scaffold. Building the aarch64 `libpure_ffi.so` needs the Android NDK
// (see PROGRESS.md); this file compiles against JNA inside the Gradle app once
// the .so is produced with cargo-ndk. UniFFI is the alternative for Kotlin, but
// it would own its own ABI rather than consume this shared C one, so JNA keeps
// the single-ABI design intact.
//
// Gradle deps: net.java.dev.jna:jna:5.14.0@aar  (bundles the .so per ABI under
// jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libpure_ffi.so).

package dev.purestudy.core

import com.sun.jna.Callback
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.NativeLong
import com.sun.jna.Pointer
import com.sun.jna.PointerByReference
import com.sun.jna.Structure

/** Raw JNA view of the C ABI (`pure_study.h`). Owned string returns are typed
 *  [Pointer] (never `String`) so we free them through
 *  [PureFfi.pure_study_string_free]; opaque handles are [Pointer] and freed via
 *  their `*_free`. Borrowed `char*` inputs are plain UTF-8 [String] (a Kotlin
 *  `null` marshals to a NULL pointer for the ABI's nullable string params).
 *
 *  Sections mirror the ordering in `pure_study.h`. */
internal interface PureFfi : Library {
    // ── lifecycle: version, string free, engine open/free ──────────────────
    fun pure_study_version(): Pointer?
    fun pure_study_string_free(ptr: Pointer?)

    fun pure_engine_open(home: String, outErr: PointerByReference): Pointer?
    fun pure_engine_open_from_bytes(
        kjv: ByteArray, kjvLen: NativeLong,
        strongs: ByteArray, strongsLen: NativeLong,
        outErr: PointerByReference,
    ): Pointer?
    fun pure_engine_free(engine: Pointer?)

    // ── corpus lookups (verse / token / toc) ───────────────────────────────
    fun pure_engine_toc_json(engine: Pointer): Pointer?
    fun pure_engine_chapter_count(engine: Pointer, book: String): Int
    fun pure_engine_verse_json(engine: Pointer, refKey: String): Pointer?
    fun pure_engine_token_json(engine: Pointer, refKey: String, tokenIndex: Int): Pointer?

    // ── layout: chapter -> display list, paint / measure / hit-test ─────────
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

    // ── Strong's / renderings / word-codes / search ─────────────────────────
    fun pure_engine_strongs_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_strongs_occurrences_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_renderings_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_word_codes_json(engine: Pointer, word: String): Pointer?
    fun pure_engine_search_json(engine: Pointer, query: String): Pointer?

    // ── study data (read): threads / tags / weaves / xrefs ──────────────────
    fun pure_engine_threads_json(engine: Pointer): Pointer?
    fun pure_engine_tags_json(engine: Pointer): Pointer?
    fun pure_engine_verse_xrefs_json(engine: Pointer, refKey: String): Pointer?
    fun pure_engine_suggested_weaves_json(engine: Pointer): Pointer?

    // ── R&D layer: concept neighbours / bridge / morphology / similarity ────
    fun pure_engine_concept_neighbours_json(engine: Pointer, code: String, k: Int): Pointer?
    fun pure_engine_bridge_partners_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_morph_json(engine: Pointer, refKey: String, tokenIndex: Int): Pointer?
    fun pure_engine_similar_verses_json(engine: Pointer, refKey: String, k: Int): Pointer?

    // ── study data (author; owned return: null = success, else error) ───────
    fun pure_engine_thread_add(
        engine: Pointer, name: String, refKey: String, note: String?, added: String,
    ): Pointer?
    fun pure_engine_tag_add(
        engine: Pointer, name: String, kind: String, value: String, note: String?, added: String,
    ): Pointer?
    fun pure_engine_tag_remove(engine: Pointer, name: String, kind: String, value: String): Pointer?
    fun pure_engine_weave_add_link(
        engine: Pointer, name: String, aRef: String, bRef: String, added: String,
    ): Pointer?
    fun pure_engine_weave_approve(engine: Pointer, index: Int): Pointer?
    fun pure_engine_weave_reject(engine: Pointer, index: Int): Pointer?
    fun pure_engine_thread_set_notes(engine: Pointer, name: String, notes: String): Pointer?
    fun pure_engine_thread_entry_set_note(
        engine: Pointer, name: String, index: Int, note: String?,
    ): Pointer?
    fun pure_engine_weave_set_notes(engine: Pointer, name: String, notes: String): Pointer?

    // ── translators' notes / study xrefs / weave library / canon ────────────
    fun pure_engine_verse_notes_json(engine: Pointer, refKey: String): Pointer?
    fun pure_engine_study_xrefs_json(engine: Pointer, refKey: String): Pointer?
    fun pure_engine_weaves_json(engine: Pointer): Pointer?
    fun pure_engine_link_pairs_json(engine: Pointer): Pointer?
    fun pure_engine_canon_segments_json(engine: Pointer): Pointer?

    // ── chord map / constellation ───────────────────────────────────────────
    fun pure_engine_chord_map_json(engine: Pointer): Pointer?
    fun pure_engine_constellation_json(engine: Pointer, page: Int, pinsJson: String?): Pointer?

    // ── symbolic concept engine + gloss ─────────────────────────────────────
    fun pure_engine_concept_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_concept_map_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_gloss(engine: Pointer, code: String): Pointer?

    // ── study-panel content model (typed block lists) ───────────────────────
    fun pure_engine_word_study_blocks_json(
        engine: Pointer, refKey: String, tokenIndex: Int, full: Boolean,
    ): Pointer?
    fun pure_engine_code_study_blocks_json(
        engine: Pointer, code: String, word: String?, full: Boolean,
    ): Pointer?
    fun pure_engine_concordance_blocks_json(engine: Pointer, code: String): Pointer?
    fun pure_engine_rendering_concordance_blocks_json(
        engine: Pointer, code: String, rendering: String,
    ): Pointer?
    fun pure_engine_threads_blocks_json(engine: Pointer): Pointer?
    fun pure_engine_thread_blocks_json(engine: Pointer, index: Int): Pointer?
    fun pure_engine_tags_blocks_json(engine: Pointer): Pointer?
    fun pure_engine_tag_blocks_json(engine: Pointer, index: Int): Pointer?
    fun pure_engine_weaves_blocks_json(engine: Pointer): Pointer?
    fun pure_engine_suggested_blocks_json(engine: Pointer): Pointer?
    fun pure_engine_compare_blocks_json(engine: Pointer, index: Int, full: Boolean): Pointer?
    fun pure_engine_search_blocks_json(engine: Pointer, query: String): Pointer?

    // ── authoring: weave link with word spans ───────────────────────────────
    fun pure_engine_weave_add_link_spans(
        engine: Pointer, name: String, aRef: String, bRef: String,
        aLo: Int, aHi: Int, bLo: Int, bHi: Int, added: String,
    ): Pointer?

    // ── link routing (engine-independent) ───────────────────────────────────
    fun pure_route_link_json(uri: String?): Pointer?

    // ── shell config (engine-independent) ───────────────────────────────────
    fun pure_config_load_json(): Pointer?
    fun pure_config_save_json(json: String?): Pointer?

    // ── Tier 0: copy, personal notes, tag colour, highlights, warming ───────
    fun pure_engine_copy_text(engine: Pointer, refKey: String, kind: String): Pointer?
    fun pure_engine_user_note_json(engine: Pointer, refKey: String): Pointer?
    fun pure_engine_user_notes_json(engine: Pointer): Pointer?
    fun pure_engine_user_note_set(
        engine: Pointer, refKey: String, text: String, stamp: String,
    ): Pointer?
    fun pure_engine_tag_set_color(engine: Pointer, name: String, color: String?): Pointer?
    fun pure_engine_highlight_add(
        engine: Pointer, name: String, color: String?,
        startRef: String, startTok: Int, endRef: String, endTok: Int, added: String,
    ): Pointer?
    fun pure_engine_highlight_remove(
        engine: Pointer, name: String, startRef: String, startTok: Int, endRef: String, endTok: Int,
    ): Pointer?
    fun pure_engine_highlight_clear_verse(engine: Pointer, verseRef: String): Pointer?
    fun pure_engine_chapter_highlights_json(engine: Pointer, book: String, chapter: Int): Pointer?

    // ── theme palettes / highlight tones (engine-independent) ────────────────
    fun pure_theme_palette_json(theme: String?): Pointer?
    fun pure_theme_highlight_tones_json(): Pointer?

    // ── warm lazy indexes ───────────────────────────────────────────────────
    fun pure_engine_warm_indexes(engine: Pointer): Pointer?

    // ── static panel content: guide / about ─────────────────────────────────
    fun pure_panel_guide_blocks_json(): Pointer?
    fun pure_panel_about_blocks_json(): Pointer?

    companion object {
        val INSTANCE: PureFfi = Native.load("pure_ffi", PureFfi::class.java)
    }
}

/** Advance-width callback the shell backs with Android's text engine. */
fun interface MeasureCallback : Callback {
    fun invoke(ctx: Pointer?, text: Pointer?): Float
}

/** `#[repr(C)]` mirror of PureLayoutConfig; passed by value. Field order and
 *  set MUST match `pure_study.h` exactly, `verse_break` included, or the
 *  by-value marshalling misreads the struct. */
@Structure.FieldOrder(
    "width", "lineHeight", "spaceWidth", "verseNumGap", "paraIndent", "paraSpacing", "verseBreak",
)
open class PureLayoutConfig : Structure() {
    @JvmField var width: Float = 0f
    @JvmField var lineHeight: Float = 0f
    @JvmField var spaceWidth: Float = 0f
    @JvmField var verseNumGap: Float = 0f
    @JvmField var paraIndent: Float = 0f
    @JvmField var paraSpacing: Float = 0f
    /** Nonzero: start every verse on a fresh line (verse-per-line mode). */
    @JvmField var verseBreak: Int = 0
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

/** The loaded study core. Call [close] (or use `use { }`) to free native memory.
 *  Method names mirror `PureStudy.cs`'s `StudyEngine`, camelCased for Kotlin. */
class StudyEngine private constructor(private val handle: Pointer) : AutoCloseable {
    companion object {
        private val ffi get() = PureFfi.INSTANCE

        /** The core version string. Never null. */
        fun version(): String = take(ffi.pure_study_version())!!

        /** Open from an overlay-style home dir (contains data/kjv.jsonl + strongs.json). */
        fun open(home: String): StudyEngine {
            val err = PointerByReference()
            val h = ffi.pure_engine_open(home, err)
                ?: throw PureStudyException(take(err.value) ?: "could not open engine")
            return StudyEngine(h)
        }

        /** Open from bundled bytes (the kjv.jsonl text and strongs.json object). */
        fun openFromBytes(kjv: ByteArray, strongs: ByteArray): StudyEngine {
            val err = PointerByReference()
            val h = ffi.pure_engine_open_from_bytes(
                kjv, NativeLong(kjv.size.toLong()),
                strongs, NativeLong(strongs.size.toLong()), err,
            ) ?: throw PureStudyException(take(err.value) ?: "could not open engine")
            return StudyEngine(h)
        }

        // ── engine-independent statics ─────────────────────────────────────

        /** Parse a panel link URI into the typed verb the shell dispatches on
         *  (`{verb, …}`). Null on an unknown verb / malformed payload. */
        fun routeLinkJson(uri: String): String? = take(ffi.pure_route_link_json(uri))

        /** The colour palette for a theme (`light`/`dark`/`night`) as JSON. Never null. */
        fun paletteJson(theme: String): String = take(ffi.pure_theme_palette_json(theme))!!

        /** The fixed highlight tones (`{tones:[{name,hex}]}`) — the swatch menu. */
        fun highlightTonesJson(): String = take(ffi.pure_theme_highlight_tones_json())!!

        /** The in-app guide / About card as panel blocks. Static (engine-independent). */
        fun guideBlocksJson(): String = take(ffi.pure_panel_guide_blocks_json())!!
        fun aboutBlocksJson(): String = take(ffi.pure_panel_about_blocks_json())!!
    }

    // ── corpus lookups ─────────────────────────────────────────────────────

    fun tocJson(): String = take(ffi.pure_engine_toc_json(handle))!!
    fun chapterCount(book: String): Int = ffi.pure_engine_chapter_count(handle, book)
    fun verseJson(reference: String): String? = take(ffi.pure_engine_verse_json(handle, reference))
    fun tokenJson(reference: String, tokenIndex: Int): String? =
        take(ffi.pure_engine_token_json(handle, reference, tokenIndex))
    fun strongsJson(code: String): String? = take(ffi.pure_engine_strongs_json(handle, code))
    fun strongsOccurrencesJson(code: String): String? =
        take(ffi.pure_engine_strongs_occurrences_json(handle, code))
    /** The rendering lens for a code: renderings + counts + capped refs. */
    fun renderingsJson(code: String): String? =
        take(ffi.pure_engine_renderings_json(handle, code))
    /** The reverse lens: the codes a surface English word translates. */
    fun wordCodesJson(word: String): String? =
        take(ffi.pure_engine_word_codes_json(handle, word))
    fun searchJson(query: String): String? = take(ffi.pure_engine_search_json(handle, query))

    // ── study data (read) ──────────────────────────────────────────────────

    fun threadsJson(): String? = take(ffi.pure_engine_threads_json(handle))
    fun tagsJson(): String? = take(ffi.pure_engine_tags_json(handle))
    fun suggestedWeavesJson(): String? = take(ffi.pure_engine_suggested_weaves_json(handle))
    fun weavesJson(): String? = take(ffi.pure_engine_weaves_json(handle))
    /** Deduped canonical weave pairs (connector lines + chord map), each endpoint
     *  located and flagged resolved. */
    fun linkPairsJson(): String? = take(ffi.pure_engine_link_pairs_json(handle))
    /** The canon overview segmentation (8 bands + OT/NT divide), frozen in core. */
    fun canonSegmentsJson(): String? = take(ffi.pure_engine_canon_segments_json(handle))
    /** The book-to-book weave chord map: canon-ordered book-pair counts + max. */
    fun chordMapJson(): String? = take(ffi.pure_engine_chord_map_json(handle))

    /** One laid-out page of the constellation. `pins` are weave indices (the
     *  lanes' handles); the shell holds the transient page + pin set. */
    fun constellationJson(page: Int, pins: Collection<Int>): String? {
        val pinsJson = pins.joinToString(prefix = "[", separator = ",", postfix = "]")
        return take(ffi.pure_engine_constellation_json(handle, page, pinsJson))
    }

    fun verseXrefsJson(refKey: String): String? =
        take(ffi.pure_engine_verse_xrefs_json(handle, refKey))
    /** The verse's 1769 margin notes, or null when it has none. */
    fun verseNotesJson(refKey: String): String? =
        take(ffi.pure_engine_verse_notes_json(handle, refKey))
    /** The verse's TSK study cross-references, or null when it has none. */
    fun studyXrefsJson(refKey: String): String? =
        take(ffi.pure_engine_study_xrefs_json(handle, refKey))
    /** Concept stats (distribution, collocates, community, leitwort) — null for a
     *  code that never occurs. First call builds the engine (~seconds). */
    fun conceptJson(code: String): String? = take(ffi.pure_engine_concept_json(handle, code))
    /** The concept map for a code: radial neighbourhood + dispersion counts. */
    fun conceptMapJson(code: String): String? = take(ffi.pure_engine_concept_map_json(handle, code))
    /** The short English gloss for a code (plain text, not JSON), or null. */
    fun gloss(code: String): String? = take(ffi.pure_engine_gloss(handle, code))

    // ── study-panel content model (typed block lists) ──────────────────────

    /** Word study for a tapped token as a block list; `full` gates the R&D tiers. */
    fun wordStudyBlocksJson(refKey: String, tokenIndex: Int, full: Boolean): String? =
        take(ffi.pure_engine_word_study_blocks_json(handle, refKey, tokenIndex, full))
    /** The standalone `code:CODE[:word]` study card as blocks (`word` may be null). */
    fun codeStudyBlocksJson(code: String, word: String?, full: Boolean): String? =
        take(ffi.pure_engine_code_study_blocks_json(handle, code, word, full))
    fun concordanceBlocksJson(code: String): String? =
        take(ffi.pure_engine_concordance_blocks_json(handle, code))
    fun renderingConcordanceBlocksJson(code: String, rendering: String): String? =
        take(ffi.pure_engine_rendering_concordance_blocks_json(handle, code, rendering))
    fun threadsBlocksJson(): String? = take(ffi.pure_engine_threads_blocks_json(handle))
    fun threadBlocksJson(index: Int): String? = take(ffi.pure_engine_thread_blocks_json(handle, index))
    fun tagsBlocksJson(): String? = take(ffi.pure_engine_tags_blocks_json(handle))
    fun tagBlocksJson(index: Int): String? = take(ffi.pure_engine_tag_blocks_json(handle, index))
    fun weavesBlocksJson(): String? = take(ffi.pure_engine_weaves_blocks_json(handle))
    fun suggestedBlocksJson(): String? = take(ffi.pure_engine_suggested_blocks_json(handle))
    /** A weave compare card as blocks; `full` adds the edit-notes action. */
    fun compareBlocksJson(index: Int, full: Boolean): String? =
        take(ffi.pure_engine_compare_blocks_json(handle, index, full))
    /** Search results as blocks (goto link or ranked hits + snippets); null on blank. */
    fun searchBlocksJson(query: String): String? =
        take(ffi.pure_engine_search_blocks_json(handle, query))

    // ── study data (author; null = success, else an error message) ──────────

    fun threadAdd(name: String, refKey: String, note: String?, addedUtc: String): String? =
        take(ffi.pure_engine_thread_add(handle, name, refKey, note, addedUtc))
    fun tagAdd(name: String, kind: String, value: String, note: String?, addedUtc: String): String? =
        take(ffi.pure_engine_tag_add(handle, name, kind, value, note, addedUtc))
    fun tagRemove(name: String, kind: String, value: String): String? =
        take(ffi.pure_engine_tag_remove(handle, name, kind, value))
    fun weaveAddLink(name: String, aRef: String, bRef: String, addedUtc: String): String? =
        take(ffi.pure_engine_weave_add_link(handle, name, aRef, bRef, addedUtc))

    /** Author a weave link carrying word spans (token index ranges); pass null for
     *  a span-less side. Null = success, else an error message. */
    fun weaveAddLinkSpans(
        name: String, aRef: String, bRef: String,
        spanA: Pair<Int, Int>?, spanB: Pair<Int, Int>?, addedUtc: String,
    ): String? = take(
        ffi.pure_engine_weave_add_link_spans(
            handle, name, aRef, bRef,
            spanA?.first ?: -1, spanA?.second ?: -1,
            spanB?.first ?: -1, spanB?.second ?: -1, addedUtc,
        )
    )

    fun weaveApprove(index: Int): String? = take(ffi.pure_engine_weave_approve(handle, index))
    fun weaveReject(index: Int): String? = take(ffi.pure_engine_weave_reject(handle, index))

    fun threadSetNotes(name: String, notes: String): String? =
        take(ffi.pure_engine_thread_set_notes(handle, name, notes))
    /** A null `note` clears the entry's note. */
    fun threadEntrySetNote(name: String, index: Int, note: String?): String? =
        take(ffi.pure_engine_thread_entry_set_note(handle, name, index, note))
    fun weaveSetNotes(name: String, notes: String): String? =
        take(ffi.pure_engine_weave_set_notes(handle, name, notes))

    // ── Tier 0: copy, personal notes, highlights, warming ───────────────────

    /** Clipboard text for a verse / its chapter, in one of the CopyKind shapes
     *  (`verse`/`verseRef`/`verseMarkdown`/`chapter`/`chapterMarkdown`). Plain
     *  text (not JSON); null on a bad ref or unknown kind. */
    fun copyText(refKey: String, kind: String): String? =
        take(ffi.pure_engine_copy_text(handle, refKey, kind))
    /** The reader's personal note on a verse, or null when it has none. */
    fun userNoteJson(refKey: String): String? = take(ffi.pure_engine_user_note_json(handle, refKey))
    /** All personal notes (`{notes:[…]}`), canonical order — gutter marks + browser. */
    fun userNotesJson(): String? = take(ffi.pure_engine_user_notes_json(handle))
    /** Set (or clear, with an empty `text`) the personal note on a verse. Null = success. */
    fun userNoteSet(refKey: String, text: String, stampUtc: String): String? =
        take(ffi.pure_engine_user_note_set(handle, refKey, text, stampUtc))
    /** Set (or clear, with a null `color`) the swatch colour of a tag — drives
     *  highlighting. Null = success. */
    fun tagSetColor(name: String, color: String?): String? =
        take(ffi.pure_engine_tag_set_color(handle, name, color))

    /** The highlight washes for a chapter (`{book,chapter,verses:[…],runs:[…]}`).
     *  Never null on a live engine. */
    fun chapterHighlightsJson(book: String, chapter: Int): String? =
        take(ffi.pure_engine_chapter_highlights_json(handle, book, chapter))
    /** Add a word-precise cross-verse highlight range to a tone tag (created
     *  coloured on first use); endpoints are ordered canonically in core, so a
     *  backwards drag is fine. `color` may be null. Null = success. */
    fun highlightAdd(
        name: String, color: String?, startRef: String, startTok: Int,
        endRef: String, endTok: Int, addedUtc: String,
    ): String? = take(
        ffi.pure_engine_highlight_add(handle, name, color, startRef, startTok, endRef, endTok, addedUtc)
    )
    /** Remove the highlight range with these exact endpoints from a tag. Null = success. */
    fun highlightRemove(
        name: String, startRef: String, startTok: Int, endRef: String, endTok: Int,
    ): String? = take(
        ffi.pure_engine_highlight_remove(handle, name, startRef, startTok, endRef, endTok)
    )
    /** Drop every highlight range covering a verse (the drag-remove path). Null = success. */
    fun highlightClearVerse(verseRef: String): String? =
        take(ffi.pure_engine_highlight_clear_verse(handle, verseRef))

    /** Force the lazy analytics indexes to build now (call on a background thread
     *  at startup in Full mode). Safe from any thread; null = success. */
    fun warmIndexes(): String? = take(ffi.pure_engine_warm_indexes(handle))

    // ── R&D tier (null when the artifact is absent) ─────────────────────────

    fun conceptNeighboursJson(code: String, k: Int): String? =
        take(ffi.pure_engine_concept_neighbours_json(handle, code, k))
    fun bridgePartnersJson(code: String): String? =
        take(ffi.pure_engine_bridge_partners_json(handle, code))
    fun morphJson(refKey: String, tokenIndex: Int): String? =
        take(ffi.pure_engine_morph_json(handle, refKey, tokenIndex))
    fun similarVersesJson(refKey: String, k: Int): String? =
        take(ffi.pure_engine_similar_verses_json(handle, refKey, k))

    // ── layout ───────────────────────────────────────────────────────────────

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

/** The cross-platform shell config (shared file with the GTK / WinUI shells).
 *  Engine-independent — mirrors `PureStudy.cs`'s `StudyConfig`. */
object StudyConfig {
    private val ffi get() = PureFfi.INSTANCE

    /** `{studyMode, bodySize, openPanes, activePane, firstRun}`; never null. */
    fun loadJson(): String = take(ffi.pure_config_load_json())!!

    /** Save from the same JSON shape. Null = success, else an error message. */
    fun saveJson(json: String): String? = take(ffi.pure_config_save_json(json))
}
