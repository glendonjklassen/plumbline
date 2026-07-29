// Kotlin/JNA binding to the Plumbline C ABI — the whole "binding" for the
// Jetpack Compose (Android) shell. It binds the SAME flat C ABI as C# does
// (decision #1: one C ABI, thin native shells), via JNA. A Compose reader
// measures glyphs with Android's text engine, hands widths back through the
// `Measure` callback, paints the display-list JSON, and forwards taps to
// hit-test. No study logic lives in Kotlin.
//
// This file is the low-level [PlumblineNative] interface only — it mirrors
// every function in `crates/ffi/include/plumbline.h` (87 of them). The safe
// wrappers (StudyEngine / Chapter / StudyConfig) live in the Compose shell
// (`apps/android/app/src/main/java/dev/plumbline/StudyEngine.kt`), PascalCase to
// match `crates/ffi/bindings/csharp/PureStudy.cs` method-for-method. So there is
// one ABI surface here and one wrapper there — no duplicate.
//
// The .so is built with cargo-ndk into jniLibs/{arm64-v8a,x86_64}/libplumbline_ffi.so
// (see docs/ANDROID-BOOTSTRAP.md); JNA's @aar bundles libjnidispatch.so per ABI.
// UniFFI is the alternative for Kotlin, but it would own its own ABI rather than
// consume this shared C one, so JNA keeps the single-ABI design intact.

package dev.plumbline.core

import com.sun.jna.Callback
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.NativeLong
import com.sun.jna.Pointer
import com.sun.jna.ptr.PointerByReference
import com.sun.jna.Structure

/** Raw JNA view of the C ABI (`plumbline.h`). Owned string returns are typed
 *  [Pointer] (never `String`) so we free them through
 *  [PlumblineNative.plumbline_string_free]; opaque handles are [Pointer] and
 *  freed via their `*_free`. Borrowed `char*` inputs are plain UTF-8 [String] (a
 *  Kotlin `null` marshals to a NULL pointer for the ABI's nullable string
 *  params).
 *
 *  Sections mirror the ordering in `plumbline.h`. */
internal interface PlumblineNative : Library {
    // ── lifecycle: version, string free, engine open/free ──────────────────
    fun plumbline_version(): Pointer?
    fun plumbline_string_free(ptr: Pointer?)

    fun plumbline_engine_open(home: String, outErr: PointerByReference): Pointer?
    fun plumbline_engine_open_from_bytes(
        kjv: ByteArray, kjvLen: NativeLong,
        strongs: ByteArray, strongsLen: NativeLong,
        outErr: PointerByReference,
    ): Pointer?
    fun plumbline_engine_free(engine: Pointer?)

    // ── corpus lookups (verse / token / toc) ───────────────────────────────
    fun plumbline_engine_toc_json(engine: Pointer): Pointer?
    fun plumbline_engine_chapter_count(engine: Pointer, book: String): Int
    fun plumbline_engine_verse_json(engine: Pointer, refKey: String): Pointer?
    fun plumbline_engine_token_json(engine: Pointer, refKey: String, tokenIndex: Int): Pointer?

    // ── layout: chapter -> display list, paint / measure / hit-test ─────────
    fun plumbline_engine_layout_chapter(
        engine: Pointer, book: String, chapter: Int,
        cfg: PlumblineLayoutConfig.ByValue, measure: MeasureCallback, ctx: Pointer?,
    ): Pointer?
    fun plumbline_layout_to_json(dl: Pointer): Pointer?
    fun plumbline_layout_height(dl: Pointer): Float
    fun plumbline_layout_width(dl: Pointer): Float
    fun plumbline_layout_item_count(dl: Pointer): Int
    fun plumbline_layout_hit_test_json(dl: Pointer, x: Float, y: Float): Pointer?
    fun plumbline_layout_free(dl: Pointer?)

    // ── Strong's / renderings / word-codes / search ─────────────────────────
    fun plumbline_engine_strongs_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_strongs_occurrences_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_renderings_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_word_codes_json(engine: Pointer, word: String): Pointer?
    fun plumbline_engine_search_json(engine: Pointer, query: String): Pointer?

    // ── study data (read): threads / tags / weaves / xrefs ──────────────────
    fun plumbline_engine_threads_json(engine: Pointer): Pointer?
    fun plumbline_engine_tags_json(engine: Pointer): Pointer?
    fun plumbline_engine_verse_xrefs_json(engine: Pointer, refKey: String): Pointer?
    fun plumbline_engine_suggested_weaves_json(engine: Pointer): Pointer?

    // ── R&D layer: concept neighbours / bridge / morphology / similarity ────
    fun plumbline_engine_concept_neighbours_json(engine: Pointer, code: String, k: Int): Pointer?
    fun plumbline_engine_bridge_partners_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_morph_json(engine: Pointer, refKey: String, tokenIndex: Int): Pointer?
    fun plumbline_engine_similar_verses_json(engine: Pointer, refKey: String, k: Int): Pointer?

    // ── study data (author; owned return: null = success, else error) ───────
    fun plumbline_engine_thread_add(
        engine: Pointer, name: String, refKey: String, note: String?, added: String,
    ): Pointer?
    fun plumbline_engine_thread_remove(engine: Pointer, name: String): Pointer?
    fun plumbline_engine_tag_add(
        engine: Pointer, name: String, kind: String, value: String, note: String?, added: String,
    ): Pointer?
    fun plumbline_engine_tag_remove(engine: Pointer, name: String, kind: String, value: String): Pointer?
    fun plumbline_engine_weave_add_link(
        engine: Pointer, name: String, aRef: String, bRef: String, added: String,
    ): Pointer?
    fun plumbline_engine_weave_from_tag(
        engine: Pointer, tagName: String, refsJson: String?, weaveName: String?, added: String,
    ): Pointer?
    fun plumbline_engine_weave_approve(engine: Pointer, index: Int): Pointer?
    fun plumbline_engine_weave_reject(engine: Pointer, index: Int): Pointer?
    fun plumbline_engine_thread_set_notes(engine: Pointer, name: String, notes: String): Pointer?
    fun plumbline_engine_thread_entry_set_note(
        engine: Pointer, name: String, index: Int, note: String?,
    ): Pointer?
    fun plumbline_engine_weave_set_notes(engine: Pointer, name: String, notes: String): Pointer?

    // ── translators' notes / study xrefs / weave library / canon ────────────
    fun plumbline_engine_verse_notes_json(engine: Pointer, refKey: String): Pointer?
    fun plumbline_engine_study_xrefs_json(engine: Pointer, refKey: String): Pointer?
    fun plumbline_engine_weaves_json(engine: Pointer): Pointer?
    fun plumbline_engine_link_pairs_json(engine: Pointer): Pointer?
    fun plumbline_engine_canon_segments_json(engine: Pointer): Pointer?

    // ── chord map / constellation ───────────────────────────────────────────
    fun plumbline_engine_chord_map_json(engine: Pointer): Pointer?
    fun plumbline_engine_constellation_json(engine: Pointer, page: Int, pinsJson: String?): Pointer?

    // ── symbolic concept engine + gloss ─────────────────────────────────────
    fun plumbline_engine_concept_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_concept_map_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_gloss(engine: Pointer, code: String): Pointer?

    // ── study-panel content model (typed block lists) ───────────────────────
    fun plumbline_engine_word_study_blocks2_json(
        engine: Pointer, refKey: String, tokenIndex: Int, gates: Int,
    ): Pointer?
    fun plumbline_engine_code_study_blocks2_json(
        engine: Pointer, code: String, word: String?, gates: Int,
    ): Pointer?
    fun plumbline_engine_word_study_blocks_json(
        engine: Pointer, refKey: String, tokenIndex: Int, full: Boolean,
    ): Pointer?
    fun plumbline_engine_code_study_blocks_json(
        engine: Pointer, code: String, word: String?, full: Boolean,
    ): Pointer?
    fun plumbline_engine_concordance_blocks_json(engine: Pointer, code: String): Pointer?
    fun plumbline_engine_rendering_concordance_blocks_json(
        engine: Pointer, code: String, rendering: String,
    ): Pointer?
    fun plumbline_engine_threads_blocks_json(engine: Pointer): Pointer?
    fun plumbline_engine_thread_blocks_json(engine: Pointer, index: Int): Pointer?
    fun plumbline_engine_tags_blocks_json(engine: Pointer): Pointer?
    fun plumbline_engine_tag_blocks_json(engine: Pointer, index: Int): Pointer?
    fun plumbline_engine_weaves_blocks_json(engine: Pointer): Pointer?
    fun plumbline_engine_suggested_blocks_json(engine: Pointer): Pointer?
    fun plumbline_engine_compare_blocks_json(engine: Pointer, index: Int, full: Boolean): Pointer?
    fun plumbline_engine_search_blocks_json(engine: Pointer, query: String): Pointer?

    // ── authoring: weave link with word spans ───────────────────────────────
    fun plumbline_engine_weave_add_link_spans(
        engine: Pointer, name: String, aRef: String, bRef: String,
        aLo: Int, aHi: Int, bLo: Int, bHi: Int, added: String,
    ): Pointer?

    // ── link routing (engine-independent) ───────────────────────────────────
    fun plumbline_route_link_json(uri: String?): Pointer?

    // ── shell config (engine-independent) ───────────────────────────────────
    fun plumbline_config_load_json(): Pointer?
    fun plumbline_config_save_json(json: String?): Pointer?

    // ── Tier 0: copy, personal notes, tag colour, highlights, warming ───────
    fun plumbline_engine_copy_text(engine: Pointer, refKey: String, kind: String): Pointer?
    fun plumbline_engine_user_note_json(engine: Pointer, refKey: String): Pointer?
    fun plumbline_engine_user_notes_json(engine: Pointer): Pointer?
    fun plumbline_engine_user_note_set(
        engine: Pointer, refKey: String, text: String, stamp: String,
    ): Pointer?
    fun plumbline_engine_tag_set_color(engine: Pointer, name: String, color: String?): Pointer?
    fun plumbline_engine_highlight_add(
        engine: Pointer, name: String, color: String?,
        startRef: String, startTok: Int, endRef: String, endTok: Int, added: String,
    ): Pointer?
    fun plumbline_engine_highlight_remove(
        engine: Pointer, name: String, startRef: String, startTok: Int, endRef: String, endTok: Int,
    ): Pointer?
    fun plumbline_engine_highlight_clear_verse(engine: Pointer, verseRef: String): Pointer?
    fun plumbline_engine_chapter_highlights_json(engine: Pointer, book: String, chapter: Int): Pointer?

    // ── theme palettes / highlight tones (engine-independent) ────────────────
    fun plumbline_theme_palette_json(theme: String?): Pointer?
    fun plumbline_theme_highlight_tones_json(): Pointer?

    // ── warm lazy indexes ───────────────────────────────────────────────────
    fun plumbline_engine_warm_indexes(engine: Pointer): Pointer?

    // ── late R&D artifact load (web boots on the core pack; see the header) ──
    fun plumbline_engine_load_rnd_data(engine: Pointer): Pointer?

    // ── stage-2 core load (web boots on the corpus alone; see the header) ──
    fun plumbline_engine_load_core_data(engine: Pointer): Pointer?

    // ── the plain-English overlay (the AKJV delta) ──────────────────────────
    fun plumbline_engine_set_akjv_overlay(engine: Pointer, on: Boolean)
    fun plumbline_engine_akjv_available(engine: Pointer): Boolean
    fun plumbline_engine_akjv_token_json(engine: Pointer, refKey: String, tokenIndex: Int): Pointer?

    // ── memorization (Tier 2 #15): SRS cards, drills, coverage + activity ────
    fun plumbline_engine_memory_grade(
        engine: Pointer, verseRef: String, grade: String, now: String,
    ): Pointer?
    fun plumbline_engine_chapter_verse_count(engine: Pointer, book: String, chapter: Int): Int
    fun plumbline_engine_memory_add(engine: Pointer, verseRef: String, now: String): Pointer?
    fun plumbline_engine_memory_add_passage(
        engine: Pointer, startRef: String, throughRef: String, now: String,
    ): Pointer?
    fun plumbline_engine_memory_remove(engine: Pointer, verseRef: String): Pointer?
    fun plumbline_engine_memory_card_json(engine: Pointer, verseRef: String): Pointer?
    fun plumbline_engine_memory_due_json(engine: Pointer, now: String): Pointer?
    fun plumbline_engine_memory_coverage_json(engine: Pointer, now: String): Pointer?
    fun plumbline_engine_memory_activity_json(engine: Pointer): Pointer?
    fun plumbline_engine_memory_drill_json(engine: Pointer, verseRef: String, level: Int): Pointer?
    fun plumbline_engine_memory_score_json(engine: Pointer, verseRef: String, typed: String): Pointer?

    // ── the reading map: where you've read, and how long ago ────────────────
    fun plumbline_engine_reading_books_json(engine: Pointer, now: String): Pointer?
    fun plumbline_engine_reading_chapters_json(engine: Pointer, book: String, now: String): Pointer?
    fun plumbline_engine_reading_record_json(
        engine: Pointer, book: String, chapter: Int, reached: Int, seconds: Float, now: String,
    ): Pointer?
    fun plumbline_engine_reading_mark_read(
        engine: Pointer, book: String, chapter: Int, date: String,
    ): Pointer?
    fun plumbline_engine_reading_forget(engine: Pointer, book: String, chapter: Int): Pointer?

    // ── static panel content: guide / about ─────────────────────────────────
    fun plumbline_panel_guide_blocks_json(): Pointer?
    fun plumbline_panel_about_blocks_json(): Pointer?

    companion object {
        val INSTANCE: PlumblineNative = Native.load("plumbline_ffi", PlumblineNative::class.java)
    }
}

/** Advance-width callback the shell backs with Android's text engine. */
fun interface MeasureCallback : Callback {
    fun invoke(ctx: Pointer?, text: Pointer?): Float
}

/** `#[repr(C)]` mirror of PlumblineLayoutConfig; passed by value. Field order and
 *  set MUST match `plumbline.h` exactly, `verse_break` included, or the
 *  by-value marshalling misreads the struct. */
@Structure.FieldOrder(
    "width", "lineHeight", "spaceWidth", "verseNumGap", "paraIndent", "paraSpacing", "verseBreak",
)
open class PlumblineLayoutConfig : Structure() {
    @JvmField var width: Float = 0f
    @JvmField var lineHeight: Float = 0f
    @JvmField var spaceWidth: Float = 0f
    @JvmField var verseNumGap: Float = 0f
    @JvmField var paraIndent: Float = 0f
    @JvmField var paraSpacing: Float = 0f
    /** Nonzero: start every verse on a fresh line (verse-per-line mode). */
    @JvmField var verseBreak: Int = 0
    class ByValue : PlumblineLayoutConfig(), Structure.ByValue
}
