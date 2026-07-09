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

internal static unsafe class Utf8
{
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
