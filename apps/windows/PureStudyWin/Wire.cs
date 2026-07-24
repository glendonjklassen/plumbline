// Typed views of the ABI's camelCase JSON payloads (schemas frozen in
// crates/ffi/src/wire.rs). Only the fields this shell reads.

using System.Text.Json;
using System.Text.Json.Serialization;

namespace PureStudyWin;

public static class Wire
{
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    public static T Parse<T>(string json) => JsonSerializer.Deserialize<T>(json, Options)!;
}

public sealed record Toc(List<TocBook> Books);
public sealed record TocBook(string Id, string Name, ushort Chapters);

public sealed record DisplayList(float Width, float Height, List<DisplayItem> Items);

public sealed record DisplayItem(
    float X, float Y, float W, float H,
    string Text, string Kind,
    string? Verse, string? VerseDisplay,
    uint? TokenIndex, ushort? VerseNumber,
    uint Flags, List<string> Strongs);

public sealed record Hit(string Verse, string Display, uint TokenIndex, List<string> Strongs);

public sealed record StrongsEntry(
    string Code, string? Lemma, string? Xlit, string? Pron,
    string? Deriv, string? Def, string? Kjv);

public sealed record Occurrences(string Code, int Total, bool Capped, List<string> Verses);

// ── rendering lens ───────────────────────────────────────────────────────────
public sealed record RenderingLens(string Code, List<Rendering1> Renderings);
public sealed record Rendering1(string Rendering, int Total, bool Capped, List<RenderingRef> Refs);
public sealed record RenderingRef(string Verse, string Display, ushort[] Span);
public sealed record WordCodes(string Word, List<WordCode1> Codes);
public sealed record WordCode1(string Code, int Count);

public sealed record SearchResult(
    string Kind,                 // "goto" | "hits"
    // goto
    string? Book, ushort? Chapter, ushort? Verse, string? Display,
    // hits
    string? How, int? Total, bool? Capped, List<SearchHit>? Hits);

public sealed record SearchHit(string Verse, string Display, bool Note, string Why);

// ── study data ─────────────────────────────────────────────────────────────

public sealed record Threads(
    [property: System.Text.Json.Serialization.JsonPropertyName("threads")] List<Thread1> Items);
public sealed record Thread1(string Name, string Notes, string Created, List<ThreadEntry> Entries);
public sealed record ThreadEntry(
    string Verse, string Display, ushort[] Span, List<string> Text, string? Note, string Added);

public sealed record Tags(
    [property: System.Text.Json.Serialization.JsonPropertyName("tags")] List<Tag1> Items);
public sealed record Tag1(string Name, string? Color, string Created, List<TagMember> Members);
public sealed record TagMember(
    string Kind, string? Verse, string? Display, string? Strongs, string? Note, string Added);

public sealed record Xrefs(string Verse, List<XrefPartner> Partners);
public sealed record XrefPartner(string Verse, string Display, string Weave);

public sealed record SuggestedWeaves(List<SuggestedWeave> Suggested);
public sealed record SuggestedWeave(
    int Index, string Name, string Kind, string Notes, List<SuggestedLink> Links);
public sealed record SuggestedLink(string A, string ADisplay, string B, string BDisplay, string Label);

// ── parity endpoints (see docs/FEATURE-MANIFEST.md) ────────────────────────

public sealed record VerseData(
    string Reference, string Display, string Body, string Title, List<TokenData> Tokens);
public sealed record TokenData(
    string Pre, string Word, string Post, string Render, uint Flags, List<string> Strongs);

public sealed record VerseNotes(string Verse, List<string> Notes);

public sealed record StudyXrefs(string Verse, List<StudyXref> Refs);
public sealed record StudyXref(
    string To, string ToDisplay, string? End, string? EndDisplay, int Votes);

public sealed record WeaveLib(List<Weave1> Weaves);
public sealed record Weave1(
    int Index, string Name, string Kind, string KindLabel, string Notes,
    string NotesSource, string Created, bool Approved, bool Suggested,
    List<WeaveLink1> Links);
public sealed record WeaveLink1(
    string A, string ADisplay, string B, string BDisplay, string Label,
    bool Approved, ushort[]? SpanA, ushort[]? SpanB, bool Resolved);

// Deduped canonical weave pairs from the core view-model
// (pure_engine_link_pairs_json): each endpoint located + a resolvability flag,
// so the connector layer neither dedupes nor parses ref keys itself.
public sealed record LinkPairs(List<WeaveLinkPair> Pairs);
public sealed record WeaveLinkPair(
    string A, string ABook, ushort AChapter, ushort AVerse,
    string B, string BBook, ushort BChapter, ushort BVerse, bool Resolved);

// The canon overview segmentation (pure_engine_canon_segments_json), frozen in
// core::reference — the strip reads it instead of hardcoding the 8 bands.
public sealed record CanonSegments(List<CanonSegment> Segments, int OtNtDivide);
public sealed record CanonSegment(string Label, int First, int Last);

// The book-to-book chord map (pure_engine_chord_map_json): canon-ordered
// book-pair counts + max, folded once in the core. The popup lays out ribbons
// off this instead of folding link pairs and deriving the max itself.
public sealed record ChordMapData(List<ChordPair> Pairs, uint Max, int OtNtDivide, int BookCount);
public sealed record ChordPair(int A, int B, uint Count);

// One laid-out page of the constellation (pure_engine_constellation_json). All
// positions are fractions (X a canon fraction, LaneFrac 0..1 within a lane,
// Size a 0..1 witness degree); the shell holds the transient Page + pin set,
// maps fractions to pixels, and paints. It derives nothing (review item 3).
public sealed record ConstellationData(
    List<ConstellationLaneData> Lanes, int NPins, int FreeTotal, int Page, int MaxPage,
    string Caption, int LaneCapacity);
public sealed record ConstellationLaneData(
    int WeaveIndex, string Name, bool Pinned,
    List<ConstellationNodeData> Nodes, List<ConstellationEdgeData> Edges);
public sealed record ConstellationNodeData(
    float X, float LaneFrac, float Size, string RefKey, string Book, ushort Chapter, string Display);
public sealed record ConstellationEdgeData(float AX, float ALaneFrac, float BX, float BLaneFrac);

public sealed record Concept1(
    string Code, uint Total, uint Ot, uint Nt, List<BookCount> TopBooks,
    Dictionary<string, uint> ByBook, List<Scored> Collocates,
    List<string> Community, Leitwort? Leitwort);
public sealed record BookCount(string Book, string Display, uint Count);
public sealed record Leitwort(int N, int WinCount, double Score, string Label);

// The concept-map popup's view-model (pure_engine_concept_map_json): spokes
// (near ∪ community, deduped, labels pre-baked) + canon-ordered dispersion. The
// popup renders this wholesale — no shell-side assembly, gloss/lemma lookups,
// or book-order table (ByBook is indexed by canon position, 0 where absent).
public sealed record ConceptMapData(
    string Code, string CenterLabel, List<ConceptSpoke> Spokes,
    List<uint> ByBook, int OtNtDivide, int BookCount,
    // The cross-testament "bridge" row: the strongest other-testament
    // equivalents of Code (Christ G5547 ↔ Messiah H4899) + their unioned
    // per-book dispersion. Canon-ordered, length = BookCount, indexed exactly
    // like ByBook — so the strip paints it as a second row. Absent (the Rust
    // side omits the JSON field) when Code has no cross-testament partner;
    // nullable with a null default so an older payload still decodes.
    ConceptBridge? Bridge = null);
public sealed record ConceptSpoke(string Code, string Label, bool Semantic);
// Partners already truncated to the row count and ByBook already unioned on the
// Rust side (pure_engine_concept_map_json) — the shell paints them wholesale.
public sealed record ConceptBridge(List<BridgeNode> Partners, List<uint> ByBook);
// One cross-testament partner: Label is "gloss\nlemma" (like the centre/spoke
// labels), Prior the fused trust of the strongest witness tying it (0–1).
public sealed record BridgeNode(string Code, string Label, float Prior);

public sealed record ConfigState(
    string StudyMode, double BodySize, List<PaneRef1>? OpenPanes, int ActivePane, bool FirstRun,
    // Frozen additive field shared with GTK's config.json. Must round-trip even
    // before the toggle UI reads it, or a WinUI save silently resets it to
    // false and clobbers a GTK user's verse-per-line preference.
    bool VersePerLine = false,
    // Colour theme choice: "system" | "light" | "dark" | "night" (Tier 0 #5).
    // Additive; must round-trip so a save doesn't clobber the GTK preference.
    string Theme = "system",
    // Additive reader prefs — must round-trip so a WinUI save doesn't clobber a
    // preference set on another shell. CopyStyle: "verse"|"verseRef"|"verseMarkdown".
    string CopyStyle = "verseRef",
    double SideMargin = 28.0,
    double LineSpacing = 1.35,
    List<PaneRef1>? History = null);
public sealed record PaneRef1(string Book, ushort Chapter);

// ── Tier 0: themes, personal notes, highlights ──────────────────────────────

// The colour palette for a theme (pure_theme_palette_json): every semantic role
// as a #rrggbb hex. The single source is core::theme, so light/dark/night can't
// drift between shells. `Dark` drives the system-chrome (ElementTheme) choice.
public sealed record PaletteData(
    bool Dark, string Paper, string Ink, string Faded, string Added, string Divine,
    string TitleInk, string Gold, string Section, string TierGod, string TierHuman,
    string TierMachine, string TierResearch, string Mono, string Morph, string Lemma,
    string Rule, string PopupPaper, string PaneNavBg, string StripBg, string Pin);

public sealed record HighlightTones(List<HighlightTone> Tones);
public sealed record HighlightTone(string Name, string Hex);

public sealed record UserNote(
    string Verse, string Display, string Text, string Created, string Updated);
public sealed record UserNotes(List<UserNote> Notes);

public sealed record ChapterHighlights(string Book, ushort Chapter, List<VerseHighlight> Verses, List<HighlightRun>? Runs = null);
public sealed record VerseHighlight(string Verse, string Color);
/// One word-precise wash run within a verse: inclusive token indices [Lo, Hi]
/// plus the tone — the cross-verse drag highlights (Tier 0 #4).
public sealed record HighlightRun(string Verse, ushort Lo, ushort Hi, string Color);

// The study-panel content model (pure_engine_*_blocks_json): a typed block
// list the panel renders wholesale — no shell-side derivation. A run's Color is
// a semantic role (mapped to the palette), Size a logical point size, Uri makes
// it a link the panel dispatcher routes.
public sealed record PanelData(List<PanelBlock> Blocks);
public sealed record PanelBlock(
    string Kind,
    // section
    string? Title, string? MarkGlyph, string? MarkColor,
    // para
    List<PanelRun>? Runs, bool Indent, bool TopGap);
public sealed record PanelRun(
    string Text, float Size, string Color, bool Bold, bool Italic, string? Uri);

// A parsed panel link (pure_route_link_json): the one verb vocabulary, so the
// shell dispatches on the typed shape instead of re-splitting the URI string.
public sealed record PanelLinkData(
    string Verb,
    string? Book, uint? Chapter, uint? Verse,
    string? Code, string? Rendering, string? Word,
    int? Index, string? RefKey, int? Tag, int? Thread, int? Entry);

// ── R&D tier ───────────────────────────────────────────────────────────────

public sealed record Scored(string Code, float Score);
public sealed record ConceptNeighbours(string Code, List<Scored> Near, List<Scored> Cross);
public sealed record Morph(string Verse, uint TokenIndex, string Code, string Gloss);
// Tiers/ResearchGrade are additive fields carrying overlay's authority-tier
// classification (`["god","human","machine"]`, research-grade flag) so the
// shell needn't reimplement source→tier mapping. Nullable-tolerant of an
// older DLL that predates the fields.
public sealed record BridgePartner(string Code, List<string> Sources, float Prior,
    List<string>? Tiers = null, bool ResearchGrade = false);
public sealed record BridgePartners(string Code, List<BridgePartner> Partners);
public sealed record SimilarVerse(string Verse, string Display, float Score);
public sealed record SimilarVerses(string Verse, List<SimilarVerse> In, List<SimilarVerse> Cross);

// ── memorization (Tier 2 #15): SRS cards, drills, coverage + activity ───────
// Schemas frozen in crates/ffi/src/wire.rs (WireMemory*) and
// crates/core/src/memory.rs. camelCase; `Mastery` is a lowercase token
// ("new"/"learning"/"young"/"mature") and `Grade` a lowercase token
// ("again"/"hard"/"good"/"easy"). The compact ref key crosses as the "ref"
// JSON field, so — like the other ref-bearing records — it takes an explicit
// JsonPropertyName rather than relying on the camelCase policy.

/// A verse's SRS card: SM-2 schedule, mastery bucket, and full review log.
/// (`Due` here is the next-due date string; contrast VerseCoverage.Due, a bool.)
public sealed record MemoryCard(
    [property: System.Text.Json.Serialization.JsonPropertyName("ref")] string Ref,
    float Ease, int IntervalDays, int Reps, int Lapses,
    string Due, string Mastery, List<MemoryReview> Reviews);
public sealed record MemoryReview(string At, string Grade, int IntervalDays);

/// The study queue: verses due for review now, in reading order.
public sealed record MemoryDue(List<string> Refs);

/// The coverage-map data at a given `now`: per-verse standing + the 8-section
/// rollup. The map shades books by average mastery; the sections are a summary.
public sealed record MemoryCoverage(List<VerseCoverage> Verses, List<SectionCoverage> Sections);
public sealed record VerseCoverage(
    [property: System.Text.Json.Serialization.JsonPropertyName("ref")] string Ref,
    string Mastery, int Reps, int Lapses, string? LastAt, bool Due);
public sealed record SectionCoverage(string Label, int Cards, int Mature, int Reviews);

/// The activity heatmap: reviews per calendar day, oldest first.
public sealed record MemoryActivity(List<DayActivity> Days);
public sealed record DayActivity(string Day, int Reviews);

/// A drill prompt for a verse at a blank-out level: the plain text, its
/// first-letter skeleton, and the progressively-blanked form. `FirstLetters`
/// and `Text` are level-independent; only `Blanked` changes with `Level`.
public sealed record MemoryDrill(
    [property: System.Text.Json.Serialization.JsonPropertyName("ref")] string Ref,
    string Text, string FirstLetters, string Blanked, int Level, int MaxLevel);

/// The result of scoring a typed recall against the verse (LCS-aligned).
public sealed record RecallScore(float Accuracy, List<WordHit> Words);
public sealed record WordHit(string Word, bool Ok);
