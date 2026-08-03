// The safe Kotlin engine wrapper — the whole surface the Compose shell touches.
//
// It owns the native handles, marshals UTF-8, frees every returned string, and
// hands back JSON the UI decodes into the `Wire.kt` models (or plain text where
// the ABI returns text). No study logic lives here — this is the Kotlin twin of
// crates/ffi/bindings/csharp/PureStudy.cs, method-for-method, so the four shells
// stay one product.
//
// It rides the low-level JNA binding `PlumblineNative` (author A —
// dev.plumbline.core), which mirrors the frozen C ABI. Strings the ABI
// returns are owned `char*` typed as a JNA `Pointer`; [take] copies them out and
// frees them through `plumbline_string_free`. Borrowed inputs cross as plain
// Kotlin `String` (JNA encodes UTF-8), and a null `String?` becomes a null
// `char*` for the ABI's optional parameters.
//
// Method names are PascalCase on purpose: the four-author contract fixes the C#
// vocabulary (Open, TocJson, LayoutChapter, TagAdd, …) as the shared names
// every shell calls, so they are identical across StudyEngine.cs and this file.

package dev.plumbline

import com.sun.jna.NativeLong
import com.sun.jna.Pointer
import com.sun.jna.ptr.PointerByReference
import dev.plumbline.core.MeasureCallback
import dev.plumbline.core.PlumblineLayoutConfig
import dev.plumbline.core.PlumblineNative
import java.io.Closeable

/** Raised when the native core cannot satisfy a call (open failure, closed
 *  handle, layout failure). Mirrors Plumbline.cs `PlumblineException`. */
class PlumblineException(message: String) : Exception(message)

/** Token flag bits carried by display-list items and tokens (mirror the
 *  `PLUMBLINE_FLAG_*` #defines in plumbline.h). */
object PlumblineFlags {
    const val ADDED = 1   // supplied by the KJV translators (italic)
    const val DIVINE = 2  // the divine name
    const val TITLE = 4   // psalm superscription
    const val PARA = 8    // a paragraph mark (¶) precedes the word
    /** Display only (core::akjv): an AKJV re-rendering. Never in kjv.jsonl. */
    const val RERENDERED = 16
}

/** Take ownership of a `char*` the ABI returned: copy it to a Kotlin string and
 *  free it through the library's allocator. Null pointer -> null. This is the
 *  single place an owned native string is read + released, so callers never see
 *  a raw [Pointer] for a returned string. */
private fun take(p: Pointer?): String? {
    if (p == null) return null
    val s = p.getString(0, "UTF-8")
    PlumblineNative.INSTANCE.plumbline_string_free(p)
    return s
}

/**
 * The loaded, immutable study core. Construct with [Open] / [OpenFromBytes];
 * call [close] (or use `use { }`, or [Dispose]) to release native memory.
 *
 * All `*Json` getters return the ABI's camelCase JSON as a string (decode with
 * the models in `Wire.kt`); the authoring methods return `null` on success or an
 * owned error message; the static helpers need no engine.
 */
class StudyEngine private constructor(handle: Pointer) : Closeable {

    private var handle: Pointer? = handle
    private val ffi get() = PlumblineNative.INSTANCE

    /** The live handle, or an error if the engine was already closed. */
    private val h: Pointer
        get() = handle ?: throw PlumblineException("StudyEngine has been closed")

    companion object {
        private val ffi get() = PlumblineNative.INSTANCE

        /** Open from an overlay-style home dir (contains data/kjv.jsonl +
         *  strongs.json). On Android prefer [OpenFromBytes] with bundled assets. */
        fun Open(home: String): StudyEngine {
            val err = PointerByReference()
            val e = ffi.plumbline_engine_open(home, err)
                ?: throw PlumblineException(take(err.value) ?: "could not open engine")
            return StudyEngine(e)
        }

        /** Open from bundled bytes (the kjv.jsonl text and the strongs.json
         *  object) — the Android path, feeding it asset bytes. */
        fun OpenFromBytes(kjv: ByteArray, strongs: ByteArray): StudyEngine {
            val err = PointerByReference()
            val e = ffi.plumbline_engine_open_from_bytes(
                kjv, NativeLong(kjv.size.toLong()),
                strongs, NativeLong(strongs.size.toLong()), err,
            ) ?: throw PlumblineException(take(err.value) ?: "could not open engine")
            return StudyEngine(e)
        }

        // ── static, engine-independent helpers ──────────────────────────────

        /** Parse a panel link URI into the typed verb the shell dispatches on
         *  (`{verb, …}`). Null on an unknown verb / malformed payload. */
        fun RouteLinkJson(uri: String): String? = take(ffi.plumbline_route_link_json(uri))

        /** The colour palette for a theme (`light`/`dark`/`night`) as JSON.
         *  Never null. */
        fun PaletteJson(theme: String): String = take(ffi.plumbline_theme_palette_json(theme))!!

        /** The in-app guide / About cards as panel blocks. Static content. */
        fun GuideBlocksJson(): String = take(ffi.plumbline_panel_guide_blocks_json())!!
        fun AboutBlocksJson(): String = take(ffi.plumbline_panel_about_blocks_json())!!

        /** The link this reader hands over, from `{church?, startAsNewBeliever?,
         *  at?}` — plus the cleaned church, the Church button's label and the
         *  site to open. Null only on malformed JSON. See ui/Church.kt. */
        fun ShareJson(request: String): String? = take(ffi.plumbline_share_url_json(request))

        /** The reading map's tuning (`{wordsPerMinute, completeAt, freshDays,
         *  staleDays, …}`) without loading the reader's reading store. */
        fun ReadingSpecJson(): String = take(ffi.plumbline_reading_spec_json())!!

        /** Every string the shell paints, in the reader's language, in ONE call
         *  (`{lang, strings, languages}`). See ui/Strings.kt. */
        fun CatalogJson(chosen: String, device: String): String =
            take(ffi.plumbline_i18n_catalog_json(chosen, device))!!

        /** Tell the ENGINE which language to write BOOK NAMES and REFERENCES in,
         *  and get back the code it resolved. The catalogue's other half — call
         *  both, before anything reads a book name. */
        fun SetLanguage(chosen: String, device: String): String =
            take(ffi.plumbline_i18n_set_language(chosen, device)) ?: "en"
    }

    // ── corpus / lookups ────────────────────────────────────────────────────

    fun TocJson(): String = take(ffi.plumbline_engine_toc_json(h))!!
    fun ChapterCount(book: String): Int = ffi.plumbline_engine_chapter_count(h, book)

    /** Highest verse number in a chapter — the passage picker's range. */
    fun ChapterVerseCount(book: String, chapter: Int): Int =
        ffi.plumbline_engine_chapter_verse_count(h, book, chapter)
    fun VerseJson(reference: String): String? = take(ffi.plumbline_engine_verse_json(h, reference))
    fun TokenJson(reference: String, tokenIndex: Int): String? =
        take(ffi.plumbline_engine_token_json(h, reference, tokenIndex))
    fun StrongsJson(code: String): String? = take(ffi.plumbline_engine_strongs_json(h, code))
    fun StrongsOccurrencesJson(code: String): String? =
        take(ffi.plumbline_engine_strongs_occurrences_json(h, code))

    /** The rendering lens for a code: every English rendering with counts +
     *  (capped) verse refs + token spans. `renderings` empty for an untagged code. */
    fun RenderingsJson(code: String): String? = take(ffi.plumbline_engine_renderings_json(h, code))

    /** The reverse lens: the codes a surface English word translates, with counts. */
    fun WordCodesJson(word: String): String? = take(ffi.plumbline_engine_word_codes_json(h, word))

    fun SearchJson(query: String): String? = take(ffi.plumbline_engine_search_json(h, query))

    // ── study data (read) ────────────────────────────────────────────────────

    fun ThreadsJson(): String? = take(ffi.plumbline_engine_threads_json(h))
    fun TagsJson(): String? = take(ffi.plumbline_engine_tags_json(h))
    fun SuggestedWeavesJson(): String? = take(ffi.plumbline_engine_suggested_weaves_json(h))
    fun WeavesJson(): String? = take(ffi.plumbline_engine_weaves_json(h))

    /** Deduped canonical weave pairs (the connector lines + chord map), each
     *  endpoint located and flagged resolved. */
    fun LinkPairsJson(): String? = take(ffi.plumbline_engine_link_pairs_json(h))

    /** The canon overview segmentation (8 bands + OT/NT divide), frozen in core. */
    fun CanonSegmentsJson(): String? = take(ffi.plumbline_engine_canon_segments_json(h))

    /** The book-to-book weave chord map: canon-ordered book-pair counts + max. */
    fun ChordMapJson(): String? = take(ffi.plumbline_engine_chord_map_json(h))

    /** The hymnal's table of contents, in book-number order. Empty when the
     *  home carries no `data/hymnal.json`. */
    fun HymnalJson(): String? = take(ffi.plumbline_engine_hymnal_json(h))

    /** One hymn, chords transposed by `transpose` semitones and split into
     *  paintable parts. Null for an unknown id. */
    fun HymnJson(id: String, transpose: Int): String? = take(ffi.plumbline_engine_hymn_json(h, id, transpose))

    /** One laid-out page of the constellation. `pins` are weave indices (the
     *  lanes' handles); the shell holds the transient page + pin set. */
    fun ConstellationJson(page: Int, pins: Collection<Int>): String? {
        val pinsJson = pins.joinToString(prefix = "[", postfix = "]", separator = ",")
        return take(ffi.plumbline_engine_constellation_json(h, page, pinsJson))
    }

    fun VerseXrefsJson(refKey: String): String? = take(ffi.plumbline_engine_verse_xrefs_json(h, refKey))

    /** The verse's 1769 margin notes, or null when it has none. */
    fun VerseNotesJson(refKey: String): String? = take(ffi.plumbline_engine_verse_notes_json(h, refKey))

    /** The verse's TSK study cross-references, or null when it has none. */
    fun StudyXrefsJson(refKey: String): String? = take(ffi.plumbline_engine_study_xrefs_json(h, refKey))

    /** Concept stats (distribution, collocates, community, leitwort) — null for
     *  a code that never occurs. First call builds the engine (~seconds). */
    fun ConceptJson(code: String): String? = take(ffi.plumbline_engine_concept_json(h, code))

    /** The short English gloss for a code (plain text, not JSON), or null. */
    fun Gloss(code: String): String? = take(ffi.plumbline_engine_gloss(h, code))

    // ── study-panel content model (typed block lists) ─────────────────────────

    /** Word study for a tapped token as a block list; `full` gates the R&D tiers. */
    /** Word study with per-tier gates: bit 0 = human analysis, bit 1 = machine. */
    fun WordStudyBlocks2Json(refKey: String, tokenIndex: Int, gates: Int): String? =
        take(ffi.plumbline_engine_word_study_blocks2_json(h, refKey, tokenIndex, gates))

    /** Code study card with per-tier gates (see [WordStudyBlocks2Json]). */
    fun CodeStudyBlocks2Json(code: String, word: String?, gates: Int): String? =
        take(ffi.plumbline_engine_code_study_blocks2_json(h, code, word, gates))

    fun WordStudyBlocksJson(refKey: String, tokenIndex: Int, full: Boolean): String? =
        take(ffi.plumbline_engine_word_study_blocks_json(h, refKey, tokenIndex, full))

    /** The standalone `code:CODE[:word]` study card as blocks (`word` may be null). */
    fun CodeStudyBlocksJson(code: String, word: String?, full: Boolean): String? =
        take(ffi.plumbline_engine_code_study_blocks_json(h, code, word, full))

    fun ConcordanceBlocksJson(code: String): String? =
        take(ffi.plumbline_engine_concordance_blocks_json(h, code))

    fun RenderingConcordanceBlocksJson(code: String, rendering: String): String? =
        take(ffi.plumbline_engine_rendering_concordance_blocks_json(h, code, rendering))

    fun ThreadsBlocksJson(): String? = take(ffi.plumbline_engine_threads_blocks_json(h))
    fun ThreadBlocksJson(index: Int): String? = take(ffi.plumbline_engine_thread_blocks_json(h, index))
    fun TagsBlocksJson(): String? = take(ffi.plumbline_engine_tags_blocks_json(h))
    fun TagBlocksJson(index: Int): String? = take(ffi.plumbline_engine_tag_blocks_json(h, index))
    fun WeavesBlocksJson(): String? = take(ffi.plumbline_engine_weaves_blocks_json(h))
    fun SuggestedBlocksJson(): String? = take(ffi.plumbline_engine_suggested_blocks_json(h))

    /** A weave compare card as blocks; `full` adds the edit-notes action. */
    fun CompareBlocksJson(index: Int, full: Boolean): String? =
        take(ffi.plumbline_engine_compare_blocks_json(h, index, full))

    /** Search results as blocks (goto link or ranked hits + snippets); null on a
     *  blank query. */
    fun SearchBlocksJson(query: String): String? = take(ffi.plumbline_engine_search_blocks_json(h, query))

    // ── study data (author; null = success, else an error message) ────────────

    fun ThreadAdd(name: String, refKey: String, note: String?, addedUtc: String): String? =
        take(ffi.plumbline_engine_thread_add(h, name, refKey, note, addedUtc))

    /** Delete a thread and everything on it. Null = success. */
    fun ThreadRemove(name: String): String? =
        take(ffi.plumbline_engine_thread_remove(h, name))

    fun TagAdd(name: String, kind: String, value: String, note: String?, addedUtc: String): String? =
        take(ffi.plumbline_engine_tag_add(h, name, kind, value, note, addedUtc))

    fun TagRemove(name: String, kind: String, value: String): String? =
        take(ffi.plumbline_engine_tag_remove(h, name, kind, value))

    fun WeaveAddLink(name: String, aRef: String, bRef: String, addedUtc: String): String? =
        take(ffi.plumbline_engine_weave_add_link(h, name, aRef, bRef, addedUtc))

    /** Weave a tag's passages (or a subset — a JSON array of refKeys) into a
     *  canon-ordered chain; null weaveName reuses the tag's name. Re-running
     *  after the tag grows just adds the new edges. Null = success. */
    fun WeaveFromTag(tagName: String, refsJson: String?, weaveName: String?, addedUtc: String): String? =
        take(ffi.plumbline_engine_weave_from_tag(h, tagName, refsJson, weaveName, addedUtc))

    /** Author a weave link carrying word spans (inclusive token-index ranges);
     *  pass null for a span-less side. Null = success, else an error message. */
    fun WeaveAddLinkSpans(
        name: String, aRef: String, bRef: String,
        spanA: Pair<Int, Int>?, spanB: Pair<Int, Int>?, addedUtc: String,
    ): String? = take(
        ffi.plumbline_engine_weave_add_link_spans(
            h, name, aRef, bRef,
            spanA?.first ?: -1, spanA?.second ?: -1,
            spanB?.first ?: -1, spanB?.second ?: -1, addedUtc,
        )
    )

    fun WeaveApprove(index: Int): String? = take(ffi.plumbline_engine_weave_approve(h, index))
    fun WeaveReject(index: Int): String? = take(ffi.plumbline_engine_weave_reject(h, index))

    fun ThreadSetNotes(name: String, notes: String): String? =
        take(ffi.plumbline_engine_thread_set_notes(h, name, notes))

    /** A null `note` clears the entry's note. */
    fun ThreadEntrySetNote(name: String, index: Int, note: String?): String? =
        take(ffi.plumbline_engine_thread_entry_set_note(h, name, index, note))

    fun WeaveSetNotes(name: String, notes: String): String? =
        take(ffi.plumbline_engine_weave_set_notes(h, name, notes))

    // ── Tier 0: copy, personal notes, warming ─────────────────────────────────

    /** Clipboard text for a verse / its chapter in one of the CopyKind shapes
     *  (`verse`/`verseRef`/`verseMarkdown`/`chapter`/`chapterMarkdown`). Plain
     *  text (not JSON); null on a bad ref or unknown kind. */
    fun CopyText(refKey: String, kind: String): String? =
        take(ffi.plumbline_engine_copy_text(h, refKey, kind))

    /** The reader's personal note on a verse, or null when it has none. */
    fun UserNoteJson(refKey: String): String? = take(ffi.plumbline_engine_user_note_json(h, refKey))

    /** All personal notes (`{notes:[…]}`), canonical order. */
    fun UserNotesJson(): String? = take(ffi.plumbline_engine_user_notes_json(h))

    /** Set (or clear, with an empty `text`) the personal note on a verse.
     *  Null = success, else an error message. */
    fun UserNoteSet(refKey: String, text: String, stampUtc: String): String? =
        take(ffi.plumbline_engine_user_note_set(h, refKey, text, stampUtc))

    /** Force the lazy analytics indexes to build now (call on a background thread
     *  at startup in Full mode). Safe from any thread; null = success. */
    fun WarmIndexes(): String? = take(ffi.plumbline_engine_warm_indexes(h))

    /** Load the optional R&D artifacts from the home if they were absent at
     *  open (the web shell's late-pack path; a no-op when already loaded or
     *  still missing). Null = success. */
    fun LoadRndData(): String? = take(ffi.plumbline_engine_load_rnd_data(h))

    /** Load the stage-2 core data (Strong's + margin notes) if it arrived
     *  after open — the web's corpus-first boot; a no-op on Android, where
     *  everything is present at open. Null = success. */
    fun LoadCoreData(): String? = take(ffi.plumbline_engine_load_core_data(h))

    // ── memorization (Tier 2 #15): SRS cards, drills, coverage + activity ─────

    /** Grade a verse (`again`/`hard`/`good`/`easy`) at `nowUtc`, creating its
     *  SRS card on first review; SM-2 reschedules. Null = success, else error. */
    fun MemoryGrade(verseRef: String, grade: String, nowUtc: String): String? =
        take(ffi.plumbline_engine_memory_grade(h, verseRef, grade, nowUtc))

    /** Switch the plain-English overlay on/off. Reader only: memorize, Present,
     *  copy and share stay KJV whatever this says. */
    fun SetAkjvOverlay(on: Boolean) = ffi.plumbline_engine_set_akjv_overlay(h, on)

    /** Whether this home carries a usable overlay (false until stage 2). */
    fun AkjvAvailable(): Boolean = ffi.plumbline_engine_akjv_available(h)

    /** `{akjv, kjv}` for a re-rendered token, else null. */
    fun AkjvTokenJson(refKey: String, tokenIndex: Int): String? =
        take(ffi.plumbline_engine_akjv_token_json(h, refKey, tokenIndex))

    /** Start memorizing a verse — seed its SRS card (due now) if absent; no
     *  review is logged. Null = success. */
    fun MemoryAdd(verseRef: String, nowUtc: String): String? =
        take(ffi.plumbline_engine_memory_add(h, verseRef, nowUtc))

    /** Start memorizing `startRef`…`throughRef` as ONE card — a whole section
     *  recalled in one go. `throughRef` must be a later verse of the same
     *  chapter; anything else seeds a plain single-verse card. Null = success. */
    fun MemoryAddPassage(startRef: String, throughRef: String, nowUtc: String): String? =
        take(ffi.plumbline_engine_memory_add_passage(h, startRef, throughRef, nowUtc))

    /** Stop memorizing a verse (remove its card). Null = success. */
    fun MemoryRemove(verseRef: String): String? =
        take(ffi.plumbline_engine_memory_remove(h, verseRef))

    /** A verse's SRS card as JSON (schedule + mastery + review log), or null if
     *  it isn't being memorized. */
    fun MemoryCardJson(verseRef: String): String? =
        take(ffi.plumbline_engine_memory_card_json(h, verseRef))

    /** The study queue — verses due at `nowUtc`, reading order — as `{refs:[…]}`. */
    fun MemoryDueJson(nowUtc: String): String? =
        take(ffi.plumbline_engine_memory_due_json(h, nowUtc))

    /** The coverage-map data at `nowUtc` (`{verses,sections}`). */
    fun MemoryCoverageJson(nowUtc: String): String? =
        take(ffi.plumbline_engine_memory_coverage_json(h, nowUtc))

    /** The activity heatmap (`{days:[{day,reviews}]}`). */
    fun MemoryActivityJson(): String? = take(ffi.plumbline_engine_memory_activity_json(h))

    // ── the reading map ─────────────────────────────────────────────────────

    /** Every book's reading standing at `nowUtc` (`{books,since,spec}`). */
    fun ReadingBooksJson(nowUtc: String): String? =
        take(ffi.plumbline_engine_reading_books_json(h, nowUtc))

    /** One book's chapters at `nowUtc` (`{book,chapters,since,spec}`). */
    fun ReadingChaptersJson(book: String, nowUtc: String): String? =
        take(ffi.plumbline_engine_reading_chapters_json(h, book, nowUtc))

    /** Credit `seconds` of dwell to a chapter, having reached verse `reached`.
     *  Returns `{book,chapter,pct,completed,lastRead?}`, or null when there is
     *  no home to write to. */
    fun ReadingRecordJson(
        book: String,
        chapter: Int,
        reached: Int,
        seconds: Float,
        nowUtc: String,
    ): String? = take(ffi.plumbline_engine_reading_record_json(h, book, chapter, reached, seconds, nowUtc))

    /** One sample of reading time: `stepSeconds` passed with [book] [chapter] in
     *  front of somebody, who has scrolled as far as verse [reached] and touched
     *  something ([interacted]) or not. A null [book] means nothing is being read
     *  — a dialog is up, the app is going to the background — and banks the tail.
     *
     *  The core keeps the counters and the thresholds; this shell keeps only the
     *  clock. Answers null on almost every call, and
     *  `{book,chapter,pct,completed,lastRead?}` when it banked a report. */
    fun ReadingTickJson(
        book: String?,
        chapter: Int,
        reached: Int,
        stepSeconds: Float,
        interacted: Boolean,
        nowUtc: String,
    ): String? =
        take(ffi.plumbline_engine_reading_tick_json(h, book, chapter, reached, stepSeconds, interacted, nowUtc))

    /** Log a chapter as read on `date` (`YYYY-MM-DD`) by hand. Null = success. */
    fun ReadingMarkRead(book: String, chapter: Int, date: String): String? =
        take(ffi.plumbline_engine_reading_mark_read(h, book, chapter, date))

    /** Drop a chapter's reading record (back to unread). Null = success. */
    fun ReadingForget(book: String, chapter: Int): String? =
        take(ffi.plumbline_engine_reading_forget(h, book, chapter))

    /** A drill prompt for a verse at blank-out `level`
     *  (`{ref,text,firstLetters,blanked,level,maxLevel}`). */
    fun MemoryDrillJson(verseRef: String, level: Int): String? =
        take(ffi.plumbline_engine_memory_drill_json(h, verseRef, level))

    /** Score a typed recall of a verse (`{accuracy, words:[{word,ok}]}`). */
    fun MemoryScoreJson(verseRef: String, typed: String): String? =
        take(ffi.plumbline_engine_memory_score_json(h, verseRef, typed))

    // ── R&D tier (null when the artifact is absent) ────────────────────────────

    fun BridgePartnersJson(code: String): String? =
        take(ffi.plumbline_engine_bridge_partners_json(h, code))

    fun MorphJson(refKey: String, tokenIndex: Int): String? =
        take(ffi.plumbline_engine_morph_json(h, refKey, tokenIndex))

    // ── layout ─────────────────────────────────────────────────────────────────

    /**
     * Lay out a chapter, measuring text with [measure] (the shell's text stack).
     * Returns a [Chapter] handle the caller disposes; hit-test and paint off it.
     *
     * The [measure] lambda is wrapped so the callback that crosses the ABI is
     * **total**: it never throws across the boundary (foreign exceptions out of a
     * JNA callback are swallowed as 0.0 at best) and always yields a finite,
     * non-negative width (NaN / negative are clamped to 0.0, matching the ABI
     * contract). A strong reference to the callback is held for the whole native
     * call — layout is synchronous, so the local `cb` binding suffices; JNA must
     * not collect it mid-layout.
     */
    fun LayoutChapter(
        book: String,
        chapter: Int,
        cfg: PlumblineLayoutConfig.ByValue,
        measure: (String) -> Float,
    ): Chapter {
        val cb = MeasureCallback { _, text ->
            try {
                val s = text?.getString(0, "UTF-8") ?: ""
                val w = measure(s)
                if (w.isFinite() && w >= 0f) w else 0f
            } catch (t: Throwable) {
                0f // never unwind a foreign exception across the ABI
            }
        }
        val dl = ffi.plumbline_engine_layout_chapter(h, book, chapter, cfg, cb, null)
            ?: throw PlumblineException("layout failed (null engine or callback)")
        // `cb` stays referenced until here — the whole synchronous native call.
        return Chapter(dl)
    }

    // ── lifecycle ──────────────────────────────────────────────────────────────

    override fun close() {
        val cur = handle ?: return
        handle = null
        ffi.plumbline_engine_free(cur)
    }

    /** C#-parity alias for [close]. */
    fun Dispose() = close()
}

/**
 * A laid-out chapter (opaque native display list). Paint from [Json]; resolve
 * taps with [HitTestJson]. Call [close] / [dispose] to release. Mirrors the C#
 * `Chapter`.
 */
class Chapter internal constructor(handle: Pointer) : Closeable {

    private var handle: Pointer? = handle
    private val ffi get() = PlumblineNative.INSTANCE

    private val h: Pointer
        get() = handle ?: throw PlumblineException("Chapter has been disposed")

    /** Total painted height in device pixels (scrollbar extent). */
    val Height: Float get() = ffi.plumbline_layout_height(h)

    /** The column width the layout targeted. */
    val Width: Float get() = ffi.plumbline_layout_width(h)

    /** Number of placed items in the display list. */
    val ItemCount: Int get() = ffi.plumbline_layout_item_count(h)

    /** The full display list as JSON (decode with `Wire.DisplayList`). */
    fun Json(): String = take(ffi.plumbline_layout_to_json(h))!!

    /** Resolve a point (in the display list's own coordinate space) to the word
     *  under it (`Wire.Hit`), or null when it hits a verse number / gap. */
    fun HitTestJson(x: Float, y: Float): String? = take(ffi.plumbline_layout_hit_test_json(h, x, y))

    override fun close() {
        val cur = handle ?: return
        handle = null
        ffi.plumbline_layout_free(cur)
    }

    /** C#-parity alias for [close]. */
    fun dispose() = close()
}

/**
 * The cross-platform shell config (shared config.json with the GTK + WinUI
 * shells). Mirrors the C# `StudyConfig` static class.
 */
object StudyConfig {
    private val ffi get() = PlumblineNative.INSTANCE

    /** `{studyMode, bodySize, openPanes, activePane, firstRun, versePerLine,
     *  theme}`; never null. */
    fun LoadJson(): String = take(ffi.plumbline_config_load_json())!!

    /** Save from the same JSON shape. Null = success, else an error message. */
    fun SaveJson(json: String): String? = take(ffi.plumbline_config_save_json(json))
}
