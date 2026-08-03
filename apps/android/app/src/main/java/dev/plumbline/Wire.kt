// Typed views of the ABI's camelCase JSON payloads (schemas frozen in
// crates/ffi/src/wire.rs). The Kotlin twin of apps/windows/PureStudyWin/Wire.cs:
// same record names, same fields, kotlinx.serialization instead of
// System.Text.Json.
//
// The wire JSON is camelCase (serde `rename_all = "camelCase"`). kotlinx uses a
// property's own name as its JSON key, and every property here is already named
// in the camelCase the wire emits (`verseDisplay`, `tokenIndex`, `otNtDivide`,
// `aLaneFrac`, …), so no per-field @SerialName is needed. (The one that was —
// `SimilarVerses.in`, serde-renamed to a Kotlin hard keyword — went with the
// "verses like this" removal, 2026-07-30.) Decode through [PlumblineJson], which
// ignores unknown keys so additive wire evolution never breaks an older shell.
//
// The tagged unions (search answer, panel block, panel link) arrive as a single
// flat object with a discriminator (`kind` / `verb`); we mirror Wire.cs and read
// each as one flat class whose non-applicable fields stay null.

package dev.plumbline

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** The shared decoder. Lenient + tolerant of unknown/absent fields so a shell
 *  built against an older or newer DLL still reads the payloads it understands. */
val PlumblineJson: Json = Json {
    ignoreUnknownKeys = true
    isLenient = true
    coerceInputValues = true
    explicitNulls = false
}

/** Decode a wire payload with [PlumblineJson]. */
inline fun <reified T> parseWire(json: String): T = PlumblineJson.decodeFromString(json)

// ── table of contents ──────────────────────────────────────────────────────

@Serializable
data class Toc(val books: List<TocBook> = emptyList())

@Serializable
data class TocBook(val id: String, val name: String, val chapters: Int)

// ── the reading map ─────────────────────────────────────────────────────────

// There is deliberately NO `ReadingSpec` model here, and no `spec` field on the
// two payloads below that carry one (the decoder ignores unknown keys, so the
// wire is unchanged). It existed to hand the tracker its thresholds, and the
// defaults it carried for the moment before the fetch landed were a second copy
// of numbers the core owns — `wordsPerMinute` was still 220f two days after the
// core moved to 300. The counting itself now lives in the core
// (`reading::DwellTracker`, driven by `StudyEngine.ReadingTickJson`), so this
// shell has no threshold to hold and no way to hold a stale one. 2026-08-01.

/** One chapter's standing. `standing` (`unread` | `partial` | `read`) drives the
 *  hue, `glow` (0–1) the bloom, `pct` the fill. The core flattens its `Heat`
 *  into this object rather than nesting it. */
@Serializable
data class ReadingChapter(
    val chapter: Int = 0,
    val words: Int = 0,
    val pct: Float = 0f,
    val standing: String = "unread",
    val glow: Float = 0f,
    val days: Int? = null,
    val lastRead: String? = null,
)

@Serializable
data class ReadingBook(
    val book: String = "",
    val name: String = "",
    val chapters: Int = 0,
    val words: Int = 0,
    val read: Int = 0,
    val pct: Float = 0f,
    val standing: String = "unread",
    val glow: Float = 0f,
    val days: Int? = null,
    val lastRead: String? = null,
)

@Serializable
data class ReadingBooks(
    val books: List<ReadingBook> = emptyList(),
    val since: String = "",
)

@Serializable
data class ReadingChapters(
    val book: String = "",
    val chapters: List<ReadingChapter> = emptyList(),
    val since: String = "",
)

/** The outcome of a dwell report. `completed` means this call carried the pass
 *  over the bar and the chapter now counts as read through. */
@Serializable
data class ReadingRecorded(
    val book: String = "",
    val chapter: Int = 0,
    val pct: Float = 0f,
    val completed: Boolean = false,
    val lastRead: String? = null,
)

// ── layout display list + hit ───────────────────────────────────────────────

@Serializable
data class DisplayList(
    val width: Float = 0f,
    val height: Float = 0f,
    val items: List<DisplayItem> = emptyList(),
)

@Serializable
data class DisplayItem(
    val x: Float = 0f,
    val y: Float = 0f,
    val w: Float = 0f,
    val h: Float = 0f,
    val text: String = "",
    val kind: String = "",              // "word" | "verseNumber"
    val verse: String? = null,
    val verseDisplay: String? = null,
    val tokenIndex: Int? = null,
    val verseNumber: Int? = null,
    val flags: Int = 0,
    val strongs: List<String> = emptyList(),
)

@Serializable
data class Hit(
    val verse: String,
    val display: String,
    val tokenIndex: Int,
    val strongs: List<String> = emptyList(),
)

// ── Strong's / occurrences ──────────────────────────────────────────────────

@Serializable
data class StrongsEntry(
    val code: String,
    val lemma: String? = null,
    val xlit: String? = null,
    val pron: String? = null,
    val deriv: String? = null,
    val def: String? = null,
    val kjv: String? = null,
)

@Serializable
data class Occurrences(
    val code: String,
    val total: Int = 0,
    val capped: Boolean = false,
    val verses: List<String> = emptyList(),
)

// ── rendering lens ──────────────────────────────────────────────────────────

@Serializable
data class RenderingLens(val code: String, val renderings: List<Rendering1> = emptyList())

@Serializable
data class Rendering1(
    val rendering: String,
    val total: Int = 0,
    val capped: Boolean = false,
    val refs: List<RenderingRef> = emptyList(),
)

@Serializable
data class RenderingRef(val verse: String, val display: String, val span: List<Int> = emptyList())

@Serializable
data class WordCodes(val word: String, val codes: List<WordCode1> = emptyList())

@Serializable
data class WordCode1(val code: String, val count: Int = 0)

// ── search ──────────────────────────────────────────────────────────────────

// Flat view of the tagged `{kind:"goto"|"hits", …}` answer (mirror Wire.cs).
@Serializable
data class SearchResult(
    val kind: String,
    // goto
    val book: String? = null,
    val chapter: Int? = null,
    val verse: Int? = null,
    val display: String? = null,
    // hits
    val how: String? = null,
    val total: Int? = null,
    val capped: Boolean? = null,
    val hits: List<SearchHit>? = null,
)

@Serializable
data class SearchHit(
    val verse: String,
    val display: String,
    val note: Boolean = false,
    val why: String = "",
)

// ── study data: threads / tags / xrefs / suggested weaves ────────────────────

@Serializable
data class Threads(val threads: List<Thread1> = emptyList())

@Serializable
data class Thread1(
    val name: String,
    val notes: String = "",
    val created: String = "",
    val entries: List<ThreadEntry> = emptyList(),
)

@Serializable
data class ThreadEntry(
    val verse: String,
    val display: String,
    val span: List<Int> = emptyList(),
    val text: List<String> = emptyList(),
    val note: String? = null,
    val added: String = "",
)

@Serializable
data class Tags(val tags: List<Tag1> = emptyList())

@Serializable
data class Tag1(
    val name: String,
    val created: String = "",
    val members: List<TagMember> = emptyList(),
)

@Serializable
data class TagMember(
    val kind: String,                   // "verse" | "concept"
    val verse: String? = null,
    val display: String? = null,
    val strongs: String? = null,
    val note: String? = null,
    val added: String = "",
)

@Serializable
data class Xrefs(val verse: String, val partners: List<XrefPartner> = emptyList())

@Serializable
data class XrefPartner(val verse: String, val display: String, val weave: String)

@Serializable
data class SuggestedWeaves(val suggested: List<SuggestedWeave> = emptyList())

@Serializable
data class SuggestedWeave(
    val index: Int,
    val name: String,
    val kind: String,
    val notes: String = "",
    val links: List<SuggestedLink> = emptyList(),
)

@Serializable
data class SuggestedLink(
    val a: String,
    val aDisplay: String,
    val b: String,
    val bDisplay: String,
    val label: String = "",
)

// ── verse / token detail + margin notes + study xrefs ─────────────────────────

@Serializable
data class VerseData(
    val reference: String,
    val display: String,
    val body: String = "",
    val title: String = "",
    val tokens: List<TokenData> = emptyList(),
)

@Serializable
data class TokenData(
    val pre: String = "",
    val word: String = "",
    val post: String = "",
    val render: String = "",
    val flags: Int = 0,
    val strongs: List<String> = emptyList(),
)

@Serializable
data class VerseNotes(val verse: String, val notes: List<String> = emptyList())

@Serializable
data class StudyXrefs(val verse: String, val refs: List<StudyXref> = emptyList())

@Serializable
data class StudyXref(
    val to: String,
    val toDisplay: String,
    val end: String? = null,
    val endDisplay: String? = null,
    val votes: Int = 0,
)

// ── the weave library + connector pairs ──────────────────────────────────────

@Serializable
data class WeaveLib(val weaves: List<Weave1> = emptyList())

@Serializable
data class Weave1(
    val index: Int,
    val name: String,
    val kind: String,
    val kindLabel: String,
    val notes: String = "",
    val notesSource: String = "",
    val created: String = "",
    val approved: Boolean = false,
    val suggested: Boolean = false,
    val links: List<WeaveLink1> = emptyList(),
)

@Serializable
data class WeaveLink1(
    val a: String,
    val aDisplay: String,
    val b: String,
    val bDisplay: String,
    val label: String = "",
    val approved: Boolean = false,
    val spanA: List<Int>? = null,
    val spanB: List<Int>? = null,
    val resolved: Boolean = false,
)

@Serializable
data class LinkPairs(val pairs: List<WeaveLinkPair> = emptyList())

@Serializable
data class WeaveLinkPair(
    val a: String,
    val aBook: String,
    val aChapter: Int,
    val aVerse: Int,
    val b: String,
    val bBook: String,
    val bChapter: Int,
    val bVerse: Int,
    val resolved: Boolean = false,
)

// ── canon overview / chord map / constellation ───────────────────────────────

@Serializable
data class CanonSegments(val segments: List<CanonSegment> = emptyList(), val otNtDivide: Int = 0)

@Serializable
data class CanonSegment(val label: String, val first: Int, val last: Int)

@Serializable
data class ChordMapData(
    val pairs: List<ChordPair> = emptyList(),
    val max: Int = 1,
    val otNtDivide: Int = 0,
    val bookCount: Int = 0,
)

@Serializable
data class ChordPair(val a: Int, val b: Int, val count: Int)

@Serializable
data class ConstellationData(
    val lanes: List<ConstellationLaneData> = emptyList(),
    val nPins: Int = 0,
    val freeTotal: Int = 0,
    val page: Int = 0,
    val maxPage: Int = 0,
    val caption: String = "",
    val laneCapacity: Int = 0,
)

@Serializable
data class ConstellationLaneData(
    val weaveIndex: Int,
    val name: String,
    val pinned: Boolean = false,
    val nodes: List<ConstellationNodeData> = emptyList(),
    val edges: List<ConstellationEdgeData> = emptyList(),
)

@Serializable
data class ConstellationNodeData(
    val x: Float,
    val laneFrac: Float,
    val size: Float,
    val refKey: String,
    val book: String,
    val chapter: Int,
    val display: String,
)

@Serializable
data class ConstellationEdgeData(
    val aX: Float,
    val aLaneFrac: Float,
    val bX: Float,
    val bLaneFrac: Float,
)

// ── the symbolic concept engine ───────────────────────────────────────────────
// Co-occurrence statistics over the corpus (APPEARS ALONGSIDE / MOST USED IN /
// LEITWORT). The embedding-backed concept MAP that used to live here was removed
// 2026-07-30 with the rest of the machine-similarity features.

@Serializable
data class Concept1(
    val code: String,
    val total: Int = 0,
    val ot: Int = 0,
    val nt: Int = 0,
    val topBooks: List<BookCount> = emptyList(),
    val byBook: Map<String, Int> = emptyMap(),
    val collocates: List<Scored> = emptyList(),
    val community: List<String> = emptyList(),
    val leitwort: Leitwort? = null,
)

@Serializable
data class BookCount(val book: String, val display: String, val count: Int = 0)

@Serializable
data class Leitwort(val n: Int, val winCount: Int, val score: Double, val label: String)

// ── shell config / session ────────────────────────────────────────────────────

@Serializable
data class ChurchState(
    val name: String = "",
    val info: String = "",
    val url: String = "",
)

/** What a share surface asks the core for (`plumbline_share_url_json`). A null
 *  [base] lets the core supply the hosted PWA, which is what every caller but
 *  `linkAtVerse` wants. */
@Serializable
data class ShareRequest(
    val base: String? = null,
    val church: ChurchState? = null,
    val startAsNewBeliever: Boolean = false,
    val at: String? = null,
)

/** …and what it gets back: the link, the church as the core cleaned it, and the
 *  two strings a Church button needs. `siteUrl` is null when the church's own
 *  URL is not one we are willing to open. */
@Serializable
data class Share(
    val url: String = "",
    val base: String = "",
    val church: ChurchState = ChurchState(),
    val hasChurch: Boolean = false,
    val title: String = "Your church",
    val siteUrl: String? = null,
)

@Serializable
data class ConfigState(
    val studyMode: String = "",
    val bodySize: Double = 0.0,
    val openPanes: List<PaneRef1>? = null,
    val activePane: Int = 0,
    val firstRun: Boolean = false,
    // Frozen additive field shared with GTK's config.json — must round-trip.
    val versePerLine: Boolean = false,
    // Colour theme choice: "system" | "light" | "dark" | "night". Additive.
    val theme: String = "system",
    // One-tap copy shape: "verse" | "verseRef" | "verseMarkdown". Additive.
    val copyStyle: String = "verseRef",
    // Reader spacing: px margin either side of the text; line-height multiple.
    val sideMargin: Double = 28.0,
    val lineSpacing: Double = 1.35,
    // Reading history, most-recent-first (capped by the core). Additive.
    val history: List<PaneRef1> = emptyList(),
    // Per-tier analysis gates (2026-07-25, additive): curated scholarship and
    // machine/statistical tiers, independently switchable. Null in an older
    // file → the core derives them from studyMode.
    val humanAnalysis: Boolean? = null,
    val machineAnalysis: Boolean? = null,
    /** The reader's home church (additive, 2026-07-27) — carried in shared
     *  links by the web shell. Kept here so an Android save round-trips it
     *  instead of dropping it from the shared config. */
    val church: ChurchState? = null,
    /** Present-screen shares open as a new believer (additive, 2026-07-27). */
    val presentSharesAsNew: Boolean? = null,
    /** The plain-English overlay (the AKJV delta). Off unless asked. */
    val akjvOverlay: Boolean? = null,
    /** The welcome this reader was given, "new" | "curious" (additive). */
    val intro: String? = null,
    /** The reader's interface language, as a code (additive, 2026-08-02).
     *  ABSENT OR EMPTY MEANS "follow the device" — not English. Writing "en" the
     *  first time a locale resolved would have frozen that reader into English
     *  permanently. `i18n::resolve` in the core weighs this against the platform
     *  locale; see ui/Strings.kt. */
    val language: String = "",
)

@Serializable
data class PaneRef1(val book: String, val chapter: Int = 0, val verse: Int? = null)

// ── Tier 0: palette, personal notes ───────────────────────────────────────────

@Serializable
data class PaletteData(
    val dark: Boolean = false,
    val paper: String = "",
    val ink: String = "",
    val faded: String = "",
    val added: String = "",
    val divine: String = "",
    val titleInk: String = "",
    val gold: String = "",
    val section: String = "",
    val tierGod: String = "",
    val tierHuman: String = "",
    val tierMachine: String = "",
    val tierResearch: String = "",
    val mono: String = "",
    val morph: String = "",
    val lemma: String = "",
    val rule: String = "",
    val popupPaper: String = "",
    val paneNavBg: String = "",
    val stripBg: String = "",
    val pin: String = "",
    // The reading map's three hues (core::reading::Standing).
    val readUnread: String = "",
    val readPartial: String = "",
    val readDone: String = "",
)

@Serializable
data class UserNote(
    val verse: String,
    val display: String,
    val text: String = "",
    val created: String = "",
    val updated: String = "",
)

@Serializable
data class UserNotes(val notes: List<UserNote> = emptyList())


// ── study-panel content model (typed block list) ──────────────────────────────

@Serializable
data class PanelData(val blocks: List<PanelBlock> = emptyList())

// Flat view of the tagged `{kind:"section"|"para"|"rule", …}` block.
@Serializable
data class PanelBlock(
    val kind: String,
    // section
    val title: String? = null,
    val markGlyph: String? = null,
    val markColor: String? = null,
    // para
    val runs: List<PanelRun>? = null,
    val indent: Boolean = false,
    val topGap: Boolean = false,
)

@Serializable
data class PanelRun(
    val text: String,
    val size: Float = 0f,
    val color: String = "ink",
    val bold: Boolean = false,
    val italic: Boolean = false,
    val uri: String? = null,
)

// Flat view of the tagged `{verb, …}` panel link (mirror Wire.cs).
@Serializable
data class PanelLinkData(
    val verb: String,
    val book: String? = null,
    val chapter: Int? = null,
    val verse: Int? = null,
    val code: String? = null,
    val rendering: String? = null,
    val word: String? = null,
    val index: Int? = null,
    val refKey: String? = null,
    val tag: Int? = null,
    val thread: Int? = null,
    val entry: Int? = null,
)

// ── R&D tier ──────────────────────────────────────────────────────────────────

@Serializable
data class Scored(val code: String, val score: Float = 0f)

@Serializable
data class Morph(val verse: String, val tokenIndex: Int, val code: String, val gloss: String)

@Serializable
data class BridgePartner(
    val code: String,
    val sources: List<String> = emptyList(),
    val prior: Float = 0f,
    // Additive authority-tier fields; tolerant of a DLL that predates them.
    val tiers: List<String> = emptyList(),
    val researchGrade: Boolean = false,
)

@Serializable
data class BridgePartners(val code: String, val partners: List<BridgePartner> = emptyList())

// ── memorization (Tier 2 #15): SRS cards, drills, coverage + activity ─────────
// Schemas frozen in crates/ffi/src/wire.rs (WireMemory*) + crates/core/src/memory.rs.
// `mastery`/`grade` are lowercase tokens. `ref` is a plain field (not a Kotlin
// keyword, unlike C#, so no @SerialName). VerseCoverage.due is a bool
// (is-due-now); MemoryCard.due is the next-due date string.

@Serializable
data class MemoryCard(
    val ref: String = "",
    /** Reader-facing name: "Ps 23:1–6" for a passage card (additive, 2026-07-27). */
    val label: String = "",
    /** The passage's last verse, when this card spans one. */
    val through: String? = null,
    val ease: Float = 0f,
    val intervalDays: Int = 0,
    val reps: Int = 0,
    val lapses: Int = 0,
    val due: String = "",
    val mastery: String = "new",
    val reviews: List<MemoryReview> = emptyList(),
)

@Serializable
data class MemoryReview(val at: String = "", val grade: String = "", val intervalDays: Int = 0)

@Serializable
data class MemoryDue(val refs: List<String> = emptyList())

@Serializable
data class MemoryCoverage(
    /** Per-verse shading for the coverage map — a passage card contributes
     *  every verse it covers. */
    val verses: List<VerseCoverage> = emptyList(),
    /** One row per card, for the hub's list (additive, 2026-07-27). */
    val cards: List<CardSummary> = emptyList(),
    val sections: List<SectionCoverage> = emptyList(),
)

/** One card as the hub lists it — a passage is ONE row here and every verse it
 *  covers in [MemoryCoverage.verses]. [ref] (its first verse) addresses it. */
@Serializable
data class CardSummary(
    val ref: String = "",
    val label: String = "",
    val verses: Int = 1,
    val mastery: String = "new",
    val reps: Int = 0,
    val lapses: Int = 0,
    val due: Boolean = false,
)

@Serializable
data class VerseCoverage(
    val ref: String = "",
    val mastery: String = "new",
    val reps: Int = 0,
    val lapses: Int = 0,
    val lastAt: String? = null,
    val due: Boolean = false,
)

@Serializable
data class SectionCoverage(
    val label: String = "",
    val cards: Int = 0,
    val mature: Int = 0,
    val reviews: Int = 0,
)

@Serializable
data class MemoryActivity(val days: List<DayActivity> = emptyList())

@Serializable
data class DayActivity(val day: String = "", val reviews: Int = 0)

@Serializable
data class MemoryDrill(
    val ref: String = "",
    /** What the drill is called on screen — "Ps 23:1–6" for a passage. */
    val label: String = "",
    /** Verses in the drill (1 unless this card is a passage). */
    val verses: Int = 1,
    val text: String = "",
    val firstLetters: String = "",
    val blanked: String = "",
    val level: Int = 0,
    val maxLevel: Int = 0,
)

@Serializable
data class RecallScore(val accuracy: Float = 0f, val words: List<WordHit> = emptyList())

@Serializable
data class WordHit(val word: String = "", val ok: Boolean = false)

/** What the AKJV overlay does to one token (`plumbline_engine_akjv_token_json`). */
@Serializable
data class AkjvToken(val akjv: String = "", val kjv: String = "")

// ── the hymnal ────────────────────────────────────────────────────────────────

@Serializable
data class HymnalIndex(val hymns: List<HymnalEntry> = emptyList())

@Serializable
data class HymnalEntry(
    val id: String,
    val number: Int,
    /** Language code → title / first line, every language the hymn ships in. */
    val titles: Map<String, String> = emptyMap(),
    val firstLines: Map<String, String> = emptyMap(),
    val tune: String = "",
    val meter: String = "",
)

@Serializable
data class Hymn1(
    val id: String,
    val number: Int,
    val tune: String = "",
    val meter: String = "",
    /** The written key of the charts. */
    val key: String = "",
    /** Semitones applied (echo), and the key the chords are NOW in. */
    val transpose: Int = 0,
    val transposedKey: String = "",
    val texts: Map<String, HymnText1> = emptyMap(),
)

@Serializable
data class HymnText1(
    val title: String,
    val author: String = "",
    val translator: String? = null,
    val year: Int? = null,
    val stanzas: List<HymnStanza> = emptyList(),
    /** Sung after every stanza; charts live on stanza 1 and here. */
    val chorus: HymnStanza? = null,
)

@Serializable
data class HymnStanza(val lines: List<HymnLine> = emptyList())

@Serializable
data class HymnLine(val parts: List<HymnPart> = emptyList())

@Serializable
data class HymnPart(val chord: String? = null, val text: String = "")
