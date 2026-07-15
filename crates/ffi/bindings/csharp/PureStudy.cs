// A thin, idiomatic C# layer over the generated P/Invoke shim
// (PureStudyNative.g.cs). This is all a WinUI shell needs on top of the ABI:
// it owns the native handles, marshals UTF-8, frees returned strings, and hands
// back JSON the UI can decode (or bind to typed records). No study logic here.
//
// The generated shim + this file are the entire "binding". See demo/ for use.

using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using PureStudy.Native;

namespace PureStudy;

public sealed class PureStudyException(string message) : Exception(message);

/// Token flag bits carried by display-list items and tokens (mirror the
/// PURE_FLAG_* #defines in pure_study.h; csbindgen does not emit Rust consts).
public static class PureFlags
{
    public const uint Added = 1;   // supplied by the KJV translators (italic)
    public const uint Divine = 2;  // the divine name
    public const uint Title = 4;   // psalm superscription
    public const uint Para = 8;    // a paragraph mark (¶) precedes the word
}

/// The loaded study core. Dispose to release native memory.
public sealed unsafe class StudyEngine : IDisposable
{
    private PureEngine* _handle;

    private StudyEngine(PureEngine* handle) => _handle = handle;

    /// Open from an overlay-style home dir (contains data/kjv.jsonl + strongs.json).
    public static StudyEngine Open(string home)
    {
        var homeUtf8 = Utf8.NulTerminated(home);
        fixed (byte* h = homeUtf8)
        {
            byte* err = null;
            var e = PureStudyNative.pure_engine_open(h, &err);
            if (e == null)
                throw new PureStudyException(Utf8.Take(err) ?? "could not open engine");
            return new StudyEngine(e);
        }
    }

    /// Open from bundled bytes (the kjv.jsonl text and strongs.json object).
    public static StudyEngine OpenFromBytes(ReadOnlySpan<byte> kjv, ReadOnlySpan<byte> strongs)
    {
        fixed (byte* k = kjv)
        fixed (byte* s = strongs)
        {
            byte* err = null;
            var e = PureStudyNative.pure_engine_open_from_bytes(
                k, (nuint)kjv.Length, s, (nuint)strongs.Length, &err);
            if (e == null)
                throw new PureStudyException(Utf8.Take(err) ?? "could not open engine");
            return new StudyEngine(e);
        }
    }

    public string TocJson() => Utf8.Take(PureStudyNative.pure_engine_toc_json(_handle))!;

    public uint ChapterCount(string book)
    {
        fixed (byte* b = Utf8.NulTerminated(book))
            return PureStudyNative.pure_engine_chapter_count(_handle, b);
    }

    public string? VerseJson(string reference)
    {
        fixed (byte* r = Utf8.NulTerminated(reference))
            return Utf8.Take(PureStudyNative.pure_engine_verse_json(_handle, r));
    }

    public string? TokenJson(string reference, uint tokenIndex)
    {
        fixed (byte* r = Utf8.NulTerminated(reference))
            return Utf8.Take(PureStudyNative.pure_engine_token_json(_handle, r, tokenIndex));
    }

    public string? StrongsJson(string code)
    {
        fixed (byte* c = Utf8.NulTerminated(code))
            return Utf8.Take(PureStudyNative.pure_engine_strongs_json(_handle, c));
    }

    public string? StrongsOccurrencesJson(string code)
    {
        fixed (byte* c = Utf8.NulTerminated(code))
            return Utf8.Take(PureStudyNative.pure_engine_strongs_occurrences_json(_handle, c));
    }

    public string? SearchJson(string query)
    {
        fixed (byte* q = Utf8.NulTerminated(query))
            return Utf8.Take(PureStudyNative.pure_engine_search_json(_handle, q));
    }

    // ── study data (read) ──────────────────────────────────────────────────

    public string? ThreadsJson() => Utf8.Take(PureStudyNative.pure_engine_threads_json(_handle));
    public string? TagsJson() => Utf8.Take(PureStudyNative.pure_engine_tags_json(_handle));
    public string? SuggestedWeavesJson() => Utf8.Take(PureStudyNative.pure_engine_suggested_weaves_json(_handle));
    public string? WeavesJson() => Utf8.Take(PureStudyNative.pure_engine_weaves_json(_handle));

    public string? VerseXrefsJson(string refKey)
    {
        fixed (byte* r = Utf8.NulTerminated(refKey))
            return Utf8.Take(PureStudyNative.pure_engine_verse_xrefs_json(_handle, r));
    }

    /// The verse's 1769 margin notes, or null when it has none.
    public string? VerseNotesJson(string refKey)
    {
        fixed (byte* r = Utf8.NulTerminated(refKey))
            return Utf8.Take(PureStudyNative.pure_engine_verse_notes_json(_handle, r));
    }

    /// The verse's TSK study cross-references, or null when it has none.
    public string? StudyXrefsJson(string refKey)
    {
        fixed (byte* r = Utf8.NulTerminated(refKey))
            return Utf8.Take(PureStudyNative.pure_engine_study_xrefs_json(_handle, r));
    }

    /// Concept stats (distribution, collocates, community, leitwort) — null
    /// for a code that never occurs. First call builds the engine (~seconds).
    public string? ConceptJson(string code)
    {
        fixed (byte* c = Utf8.NulTerminated(code))
            return Utf8.Take(PureStudyNative.pure_engine_concept_json(_handle, c));
    }

    /// The short English gloss for a code (plain text, not JSON), or null.
    public string? Gloss(string code)
    {
        fixed (byte* c = Utf8.NulTerminated(code))
            return Utf8.Take(PureStudyNative.pure_engine_gloss(_handle, c));
    }

    // ── study data (author; null = success, else an error message) ────────

    public string? ThreadAdd(string name, string refKey, string? note, string addedUtc)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* r = Utf8.NulTerminated(refKey))
        fixed (byte* o = Utf8.NulTerminatedOrNull(note))
        fixed (byte* a = Utf8.NulTerminated(addedUtc))
            return Utf8.Take(PureStudyNative.pure_engine_thread_add(_handle, n, r, o, a));
    }

    public string? TagAdd(string name, string kind, string value, string? note, string addedUtc)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* k = Utf8.NulTerminated(kind))
        fixed (byte* v = Utf8.NulTerminated(value))
        fixed (byte* o = Utf8.NulTerminatedOrNull(note))
        fixed (byte* a = Utf8.NulTerminated(addedUtc))
            return Utf8.Take(PureStudyNative.pure_engine_tag_add(_handle, n, k, v, o, a));
    }

    public string? TagRemove(string name, string kind, string value)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* k = Utf8.NulTerminated(kind))
        fixed (byte* v = Utf8.NulTerminated(value))
            return Utf8.Take(PureStudyNative.pure_engine_tag_remove(_handle, n, k, v));
    }

    public string? WeaveAddLink(string name, string aRef, string bRef, string addedUtc)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* a = Utf8.NulTerminated(aRef))
        fixed (byte* b = Utf8.NulTerminated(bRef))
        fixed (byte* t = Utf8.NulTerminated(addedUtc))
            return Utf8.Take(PureStudyNative.pure_engine_weave_add_link(_handle, n, a, b, t));
    }

    /// Author a weave link carrying word spans (token index ranges); pass null
    /// for a span-less side. Null = success, else an error message.
    public string? WeaveAddLinkSpans(
        string name, string aRef, string bRef,
        (int lo, int hi)? spanA, (int lo, int hi)? spanB, string addedUtc)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* a = Utf8.NulTerminated(aRef))
        fixed (byte* b = Utf8.NulTerminated(bRef))
        fixed (byte* t = Utf8.NulTerminated(addedUtc))
            return Utf8.Take(PureStudyNative.pure_engine_weave_add_link_spans(
                _handle, n, a, b,
                spanA?.lo ?? -1, spanA?.hi ?? -1,
                spanB?.lo ?? -1, spanB?.hi ?? -1, t));
    }

    public string? WeaveApprove(uint index) => Utf8.Take(PureStudyNative.pure_engine_weave_approve(_handle, index));
    public string? WeaveReject(uint index) => Utf8.Take(PureStudyNative.pure_engine_weave_reject(_handle, index));

    public string? ThreadSetNotes(string name, string notes)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* s = Utf8.NulTerminated(notes))
            return Utf8.Take(PureStudyNative.pure_engine_thread_set_notes(_handle, n, s));
    }

    /// A null `note` clears the entry's note.
    public string? ThreadEntrySetNote(string name, uint index, string? note)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* s = Utf8.NulTerminatedOrNull(note))
            return Utf8.Take(PureStudyNative.pure_engine_thread_entry_set_note(_handle, n, index, s));
    }

    public string? WeaveSetNotes(string name, string notes)
    {
        fixed (byte* n = Utf8.NulTerminated(name))
        fixed (byte* s = Utf8.NulTerminated(notes))
            return Utf8.Take(PureStudyNative.pure_engine_weave_set_notes(_handle, n, s));
    }

    // ── R&D tier (null when the artifact is absent) ────────────────────────

    public string? ConceptNeighboursJson(string code, uint k)
    {
        fixed (byte* c = Utf8.NulTerminated(code))
            return Utf8.Take(PureStudyNative.pure_engine_concept_neighbours_json(_handle, c, k));
    }

    public string? BridgePartnersJson(string code)
    {
        fixed (byte* c = Utf8.NulTerminated(code))
            return Utf8.Take(PureStudyNative.pure_engine_bridge_partners_json(_handle, c));
    }

    public string? MorphJson(string refKey, uint tokenIndex)
    {
        fixed (byte* r = Utf8.NulTerminated(refKey))
            return Utf8.Take(PureStudyNative.pure_engine_morph_json(_handle, r, tokenIndex));
    }

    public string? SimilarVersesJson(string refKey, uint k)
    {
        fixed (byte* r = Utf8.NulTerminated(refKey))
            return Utf8.Take(PureStudyNative.pure_engine_similar_verses_json(_handle, r, k));
    }

    /// Lay out a chapter, measuring text with `measure` (the shell's text stack).
    /// Returns a handle the caller disposes; hit-test and paint off it.
    public Chapter LayoutChapter(string book, uint chapter, PureLayoutConfig cfg, Func<string, float> measure)
    {
        var gch = GCHandle.Alloc(measure);
        try
        {
            fixed (byte* b = Utf8.NulTerminated(book))
            {
                var dl = PureStudyNative.pure_engine_layout_chapter(
                    _handle, b, chapter, cfg, &MeasureTrampoline, (void*)GCHandle.ToIntPtr(gch));
                if (dl == null)
                    throw new PureStudyException("layout failed (null engine or callback)");
                return new Chapter(dl);
            }
        }
        finally
        {
            gch.Free(); // safe: the native call is synchronous, done using it now
        }
    }

    [UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
    private static float MeasureTrampoline(void* ctx, byte* text)
    {
        var fn = (Func<string, float>)GCHandle.FromIntPtr((IntPtr)ctx).Target!;
        return fn(Marshal.PtrToStringUTF8((IntPtr)text) ?? string.Empty);
    }

    public void Dispose()
    {
        if (_handle != null)
        {
            PureStudyNative.pure_engine_free(_handle);
            _handle = null;
        }
        GC.SuppressFinalize(this);
    }

    ~StudyEngine() => Dispose();
}

/// A laid-out chapter (opaque native display list). Paint from Json(); resolve
/// clicks with HitTestJson(). Dispose to release.
public sealed unsafe class Chapter : IDisposable
{
    private PureDisplayList* _handle;
    internal Chapter(PureDisplayList* handle) => _handle = handle;

    public float Height => PureStudyNative.pure_layout_height(_handle);
    public float Width => PureStudyNative.pure_layout_width(_handle);
    public uint ItemCount => PureStudyNative.pure_layout_item_count(_handle);
    public string Json() => Utf8.Take(PureStudyNative.pure_layout_to_json(_handle))!;
    public string? HitTestJson(float x, float y) => Utf8.Take(PureStudyNative.pure_layout_hit_test_json(_handle, x, y));

    public void Dispose()
    {
        if (_handle != null)
        {
            PureStudyNative.pure_layout_free(_handle);
            _handle = null;
        }
        GC.SuppressFinalize(this);
    }

    ~Chapter() => Dispose();
}

/// The cross-platform shell config (shared file with the GTK shell).
public static unsafe class StudyConfig
{
    /// `{studyMode, bodySize, openPanes, activePane, firstRun}`; never null.
    public static string LoadJson() => Utf8.Take(PureStudyNative.pure_config_load_json())!;

    /// Save from the same JSON shape. Null = success, else an error message.
    public static string? SaveJson(string json)
    {
        fixed (byte* j = Utf8.NulTerminated(json))
            return Utf8.Take(PureStudyNative.pure_config_save_json(j));
    }
}

internal static unsafe class Utf8
{
    /// Null string -> null array, so `fixed` yields a null pointer for the
    /// ABI's optional (nullable) string parameters.
    public static byte[]? NulTerminatedOrNull(string? s) => s is null ? null : NulTerminated(s);

    /// Managed string -> owned, NUL-terminated UTF-8 bytes (pin before passing).
    public static byte[] NulTerminated(string s)
    {
        int n = Encoding.UTF8.GetByteCount(s);
        var bytes = new byte[n + 1];
        Encoding.UTF8.GetBytes(s, 0, s.Length, bytes, 0);
        bytes[n] = 0;
        return bytes;
    }

    /// Take ownership of a char* the ABI returned: copy to a managed string and
    /// free it through the library's allocator. Null -> null.
    public static string? Take(byte* p)
    {
        if (p == null) return null;
        var s = Marshal.PtrToStringUTF8((IntPtr)p);
        PureStudyNative.pure_study_string_free(p);
        return s;
    }
}
