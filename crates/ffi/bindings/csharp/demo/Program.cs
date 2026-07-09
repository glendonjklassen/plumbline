// A runnable C# consumer of the pure-study core, proving a WinUI-style shell
// stays thin: it opens the engine, lays out a chapter (measuring text itself),
// hit-tests, and decodes the JSON the ABI returns — no study logic in C#.
//
//   dotnet run --project crates/ffi/bindings/csharp/demo -- /home/gjklassen/code/overlay
//
// (libpure_ffi.so must be next to the built demo; the .csproj copies it.)

using System;
using System.Text;
using System.Text.Json;
using PureStudy;
using PureStudy.Native;

string home = args.Length > 0 ? args[0] : ".";

using var engine = StudyEngine.Open(home);
Console.WriteLine($"opened engine from {home}");
Console.WriteLine($"John has {engine.ChapterCount("John")} chapters");

// A stand-in "text engine": ~9px per UTF-8 byte. A real shell measures glyphs.
float Measure(string text) => Encoding.UTF8.GetByteCount(text) * 9.0f;

var cfg = new PureLayoutConfig
{
    width = 640f, line_height = 28f, space_width = 6f,
    verse_num_gap = 8f, para_indent = 24f, para_spacing = 12f,
};

using var chapter = engine.LayoutChapter("John", 3, cfg, Measure);
Console.WriteLine($"laid out John 3: {chapter.ItemCount} items, {chapter.Height:0}px tall");

// Decode the display list and hit-test the first word's centre.
using var doc = JsonDocument.Parse(chapter.Json());
JsonElement? firstWord = null;
foreach (var it in doc.RootElement.GetProperty("items").EnumerateArray())
    if (it.GetProperty("kind").GetString() == "word") { firstWord = it; break; }

if (firstWord is { } w)
{
    float cx = w.GetProperty("x").GetSingle() + w.GetProperty("w").GetSingle() / 2f;
    float cy = w.GetProperty("y").GetSingle() + w.GetProperty("h").GetSingle() / 2f;
    Console.WriteLine($"first word '{w.GetProperty("text").GetString()}' -> hit-test:");
    Console.WriteLine("  " + (chapter.HitTestJson(cx, cy) ?? "<null>"));
}

// Typed decode of a Strong's entry.
var strongsJson = engine.StrongsJson("G2316");
if (strongsJson is not null)
{
    var e = JsonSerializer.Deserialize<StrongsEntry>(strongsJson)!;
    Console.WriteLine($"G2316 = {e.lemma} ({e.def?[..Math.Min(48, e.def.Length)]}...)");
}

var occ = JsonDocument.Parse(engine.StrongsOccurrencesJson("G2316")!);
Console.WriteLine($"G2316 occurs in {occ.RootElement.GetProperty("total").GetInt32()} verses");

var search = JsonDocument.Parse(engine.SearchJson("love")!);
Console.WriteLine($"search 'love' -> {search.RootElement.GetProperty("total").GetInt32()} hits");

Console.WriteLine("OK: C# shell drove the Rust core end-to-end.");

// A record the shell can bind Strong's JSON straight onto.
record StrongsEntry(string code, string? lemma, string? xlit, string? pron, string? deriv, string? def, string? kjv);
