// The on-demand study sidebar: word study, concordance, threads/tags
// browsers, the suggested-weave review queue, weave compare cards, and
// search results. All interactivity funnels through one URI dispatcher that
// mirrors the GTK shell's handle_link scheme exactly (FEATURE-MANIFEST.md),
// so behavior parity is auditable link by link.

using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Media;
using PureStudy;
using Windows.UI;

namespace PureStudyWin;

public sealed class StudyPanel : UserControl
{
    private readonly StackPanel _body = new() { Spacing = 8 };
    private readonly ScrollViewer _scroll;

    public StudyEngine? Engine;
    public Func<bool> IsFull = () => false;
    /// Navigate the active pane (book, chapter, optional verse refKey to band).
    public Action<string, uint, string?> Navigate = (_, _, _) => { };
    public Action<string> OpenConceptMap = _ => { };
    /// Study data changed on disk (weaves/threads/tags) — shell refreshes
    /// its link/xref indexes and repaints connectors.
    public Action StudyDataChanged = () => { };
    /// Current weave library (shell keeps it fresh; compare cards index it).
    public WeaveLib? Weaves;

    public StudyPanel()
    {
        Width = 400;
        Visibility = Visibility.Collapsed;
        _scroll = new ScrollViewer { Padding = new Thickness(18, 14, 18, 14), Content = _body };
        Content = new Border
        {
            Background = new SolidColorBrush(Palette.PanelBg),
            BorderBrush = new SolidColorBrush(Palette.Rule),
            BorderThickness = new Thickness(1, 0, 0, 0),
            Child = _scroll,
        };
    }

    public void Open() => Visibility = Visibility.Visible;

    public void Close()
    {
        Visibility = Visibility.Collapsed;
        _body.Children.Clear();
    }

    private void Fresh()
    {
        _body.Children.Clear();
        _scroll.ChangeView(null, 0, null, true);
        Open();
    }

    private static string Now() => DateTime.UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'");

    // ── the link router (GTK handle_link) ──────────────────────────────────

    public async void Link(string uri)
    {
        if (Engine is null) return;
        var (verb, rest) = Split2(uri, ':');
        switch (verb)
        {
            case "go":
            {
                var parts = rest.Split(':');
                if (parts.Length >= 2 && uint.TryParse(parts[1], out var ch))
                {
                    string? verse = parts.Length >= 3 && ushort.TryParse(parts[2], out var v)
                        ? $"{parts[0]} {ch}:{v}" : null;
                    Navigate(parts[0], ch, verse);
                }
                break;
            }
            case "occ": ShowConcordance(rest); break;
            case "thread": if (int.TryParse(rest, out var ti)) ShowThreadDetail(ti); break;
            case "tag": if (int.TryParse(rest, out var gi)) ShowTagDetail(gi); break;
            case "weave": if (int.TryParse(rest, out var wi)) ShowCompareCard(wi); break;
            case "conceptmap": OpenConceptMap(rest); break;
            case "addtag":
            {
                var name = await PromptName($"Tag {DisplayOf(rest)}", "tag name (new or existing)");
                if (name is null) break;
                var err = Engine.TagAdd(name, "verse", rest, null, Now());
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                var idx = FindTag(name);
                if (idx >= 0) ShowTagDetail(idx);
                break;
            }
            case "addthread":
            {
                var name = await PromptName($"Add {DisplayOf(rest)} to thread", "thread name (new or existing)");
                if (name is null) break;
                var err = Engine.ThreadAdd(name, rest, null, Now());
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                var idx = FindThread(name);
                if (idx >= 0) ShowThreadDetail(idx);
                break;
            }
            case "untag":
            {
                var (i, refKey) = Split2(rest, ':');
                if (!int.TryParse(i, out var idx2)) break;
                var tags = LoadTags();
                if (idx2 >= tags.Count) break;
                var err = Engine.TagRemove(tags[idx2].Name, "verse", refKey);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                var again = FindTag(tags[idx2].Name);
                if (again >= 0) ShowTagDetail(again); else ShowTagsList();
                break;
            }
            case "approve":
            case "reject":
            {
                if (!uint.TryParse(rest, out var si)) break;
                var err = verb == "approve" ? Engine.WeaveApprove(si) : Engine.WeaveReject(si);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                ShowSuggested();
                break;
            }
            case "editthreadnotes":
            {
                if (!int.TryParse(rest, out var i3)) break;
                var threads = LoadThreads();
                if (i3 >= threads.Count) break;
                var text = await PromptText($"Notes — {threads[i3].Name}", threads[i3].Notes);
                if (text is null) break;
                var err = Engine.ThreadSetNotes(threads[i3].Name, text);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                ShowThreadDetail(i3);
                break;
            }
            case "editentrynote":
            {
                var (a, b) = Split2(rest, ':');
                if (!int.TryParse(a, out var t4) || !uint.TryParse(b, out var e4)) break;
                var threads = LoadThreads();
                if (t4 >= threads.Count || e4 >= threads[t4].Entries.Count) break;
                var text = await PromptText(
                    $"Note — {threads[t4].Entries[(int)e4].Display}",
                    threads[t4].Entries[(int)e4].Note ?? "");
                if (text is null) break;
                var err = Engine.ThreadEntrySetNote(threads[t4].Name, e4,
                    text.Length == 0 ? null : text);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                ShowThreadDetail(t4);
                break;
            }
            case "editweavenotes":
            {
                if (!int.TryParse(rest, out var i5) || Weaves is null ||
                    i5 >= Weaves.Weaves.Count) break;
                var w = Weaves.Weaves[i5];
                var text = await PromptText($"Notes — {w.Name}", w.Notes);
                if (text is null) break;
                var err = Engine.WeaveSetNotes(w.Name, text);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                ShowCompareCard(i5);
                break;
            }
        }
    }

    private static (string, string) Split2(string s, char c)
    {
        int i = s.IndexOf(c);
        return i < 0 ? (s, "") : (s[..i], s[(i + 1)..]);
    }

    private string DisplayOf(string refKey) =>
        Engine?.VerseJson(refKey) is { } j ? Wire.Parse<VerseData>(j).Display : refKey;

    private List<Thread1> LoadThreads() =>
        Engine?.ThreadsJson() is { } j ? Wire.Parse<Threads>(j).Items : new();

    private List<Tag1> LoadTags() =>
        Engine?.TagsJson() is { } j ? Wire.Parse<Tags>(j).Items : new();

    private int FindThread(string name) =>
        LoadThreads().FindIndex(t => string.Equals(t.Name, name, StringComparison.OrdinalIgnoreCase));

    private int FindTag(string name) =>
        LoadTags().FindIndex(t => string.Equals(t.Name, name, StringComparison.OrdinalIgnoreCase));

    // ── content builders: primitives ───────────────────────────────────────

    private void Add(UIElement e) => _body.Children.Add(e);

    private void ShowError(string message)
    {
        _body.Children.Insert(0, Para(13, Italic(message)));
    }

    private static Run R(string text, Color? color = null, double? size = null,
        bool bold = false, bool italic = false)
    {
        var r = new Run { Text = text };
        if (color is { } c) r.Foreground = new SolidColorBrush(c);
        if (size is { } s) r.FontSize = s;
        if (bold) r.FontWeight = Microsoft.UI.Text.FontWeights.Bold;
        if (italic) r.FontStyle = Windows.UI.Text.FontStyle.Italic;
        return r;
    }

    private static Run Italic(string text) => R(text, Palette.InkFaded, italic: true);

    private Hyperlink H(string label, string uri, Color? color = null, double? size = null)
    {
        var link = new Hyperlink { UnderlineStyle = UnderlineStyle.None };
        link.Inlines.Add(R(label, color ?? Palette.Gold, size));
        link.Click += (_, _) => Link(uri);
        return link;
    }

    /// A flowing paragraph from strings, Runs and Hyperlinks.
    private static TextBlock Para(double size, params object[] parts)
    {
        var tb = new TextBlock
        {
            FontSize = size,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Palette.Ink),
            IsTextSelectionEnabled = true,
        };
        foreach (var p in parts)
            tb.Inlines.Add(p switch
            {
                string s => new Run { Text = s },
                Inline i => i,
                _ => new Run { Text = p.ToString() },
            });
        return tb;
    }

    private static Border RuleLine() => new()
    {
        BorderBrush = new SolidColorBrush(Palette.Rule),
        BorderThickness = new Thickness(0, 1, 0, 0),
        Margin = new Thickness(0, 4, 0, 0),
    };

    /// The GTK shead(): a spaced, muted-gold, small-caps-feel section header.
    private static TextBlock SHead(string title) => new()
    {
        Text = title,
        FontSize = 11,
        CharacterSpacing = 120,
        FontWeight = Microsoft.UI.Text.FontWeights.Bold,
        Foreground = new SolidColorBrush(Palette.SectionGold),
        Margin = new Thickness(0, 8, 0, 0),
    };

    private Hyperlink Go(string refKey, string? display = null) =>
        H(display ?? refKey, GoUri(refKey));

    /// "Gen 1:7" → "go:Gen:1:7".
    private static string GoUri(string refKey)
    {
        int sp = refKey.LastIndexOf(' ');
        return sp < 0 ? $"go:{refKey}" : $"go:{refKey[..sp]}:{refKey[(sp + 1)..]}";
    }

    /// "gloss lemma" concept chips joined by middots, each linking occ:CODE.
    private TextBlock ConceptChips(double size, IEnumerable<string> codes)
    {
        var tb = new TextBlock
        {
            FontSize = size, TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Palette.Ink),
        };
        bool first = true;
        foreach (var code in codes)
        {
            if (!first) tb.Inlines.Add(new Run { Text = "  ·  " });
            first = false;
            var gloss = Engine?.Gloss(code);
            string? lemma = Engine?.StrongsJson(code) is { } sj
                ? Wire.Parse<StrongsEntry>(sj).Lemma : null;
            tb.Inlines.Add(H(gloss ?? lemma ?? code, $"occ:{code}"));
            if (gloss is not null && lemma is not null)
                tb.Inlines.Add(R($" {lemma}", Color.FromArgb(255, 138, 122, 82), size - 1));
        }
        return tb;
    }

    // ── word study ─────────────────────────────────────────────────────────

    public void ShowWordStudy(Hit hit)
    {
        if (Engine is null) return;
        Fresh();
        bool full = IsFull();

        string word = "";
        if (hit.Verse.Length > 0 &&
            Engine.TokenJson(hit.Verse, hit.TokenIndex) is { } tj)
            word = Wire.Parse<TokenData>(tj).Word;

        Add(Para(14, R(hit.Display, bold: true)));
        if (word.Length > 0) Add(Para(26, word));

        if (full && hit.Verse.Length > 0 &&
            Engine.MorphJson(hit.Verse, hit.TokenIndex) is { } mj)
        {
            var m = Wire.Parse<Morph>(mj);
            Add(Para(12.5, R(m.Gloss, Color.FromArgb(255, 106, 90, 42), italic: true)));
        }

        if (hit.Strongs.Count == 0)
            Add(Para(14, Italic("no Strong's tag on this word")));

        foreach (var code in hit.Strongs)
        {
            if (Engine.StrongsJson(code) is not { } json) continue;
            var e = Wire.Parse<StrongsEntry>(json);
            Add(RuleLine());

            int occTotal = Engine.StrongsOccurrencesJson(code) is { } oj
                ? Wire.Parse<Occurrences>(oj).Total : 0;
            Add(Para(14, R(e.Code, bold: true), "   ",
                H($"{occTotal} occurrence{(occTotal == 1 ? "" : "s")} ▸", $"occ:{code}")));
            if (e.Lemma is not null) Add(Para(22, e.Lemma));
            if (e.Xlit is not null) Add(Para(13, R(e.Xlit, italic: true)));
            if (e.Pron is not null) Add(Para(13, R($"/{e.Pron}/", Color.FromArgb(255, 136, 136, 136))));
            if (e.Deriv is not null) Add(Para(13, Italic(e.Deriv)));
            if (e.Def is not null) Add(Para(14, e.Def));
            if (e.Kjv is not null) Add(Para(12.5, R($"KJV: {e.Kjv}", Palette.InkFaded)));

            if (!full) continue;

            if (Engine.BridgePartnersJson(code) is { } bj)
            {
                var bp = Wire.Parse<BridgePartners>(bj);
                if (bp.Partners.Count > 0)
                {
                    Add(SHead("SAME ROOT ACROSS TESTAMENTS"));
                    foreach (var p in bp.Partners.Take(6))
                    {
                        var tb = ConceptChips(13.5, new[] { p.Code });
                        tb.Inlines.Add(R($"   {string.Join(" + ", p.Sources.Select(Humanize))}",
                            Palette.InkFaded, 11.5));
                        Add(tb);
                    }
                }
            }

            if (Engine.ConceptNeighboursJson(code, 6) is { } nj)
            {
                var nb = Wire.Parse<ConceptNeighbours>(nj);
                if (nb.Near.Count > 0)
                {
                    Add(SHead("SIMILAR CONCEPTS"));
                    Add(ConceptChips(13.5, nb.Near.Select(s => s.Code)));
                }
                if (nb.Cross.Count > 0)
                {
                    Add(Para(12, Italic("across the testaments —")));
                    Add(ConceptChips(13.5, nb.Cross.Select(s => s.Code)));
                }
            }

            if (Engine.ConceptJson(code) is { } cj)
            {
                var c = Wire.Parse<Concept1>(cj);
                if (c.Community.Count > 0)
                {
                    Add(SHead("APPEARS ALONGSIDE"));
                    Add(ConceptChips(13.5, c.Community.Take(8)));
                }
                if (c.TopBooks.Count > 0)
                {
                    Add(SHead("WHERE IT CONCENTRATES"));
                    Add(Para(13,
                        string.Join(" · ", c.TopBooks.Select(b => $"{b.Display} ×{b.Count}")),
                        R($"   (OT {c.Ot} · NT {c.Nt})", Palette.InkFaded, 12)));
                }
                if (c.Leitwort is { } lw)
                {
                    Add(SHead("LEITWORT"));
                    Add(Para(13,
                        $"{lw.WinCount} of its {lw.N} uses cluster in {lw.Label} ",
                        R($"(p ≈ 10^−{lw.Score:0.#})", Palette.InkFaded, 12)));
                }
            }

            Add(Para(13, H("▸ open concept map", $"conceptmap:{code}")));
        }

        if (full && hit.Verse.Length > 0)
        {
            var actions = Para(13.5,
                H("＋ tag verse", $"addtag:{hit.Verse}"), "     ",
                H("＋ add to thread", $"addthread:{hit.Verse}"));
            actions.Margin = new Thickness(0, 6, 0, 0);
            Add(actions);
        }

        if (hit.Verse.Length > 0 && Engine.VerseXrefsJson(hit.Verse) is { } xj)
        {
            var x = Wire.Parse<Xrefs>(xj);
            if (x.Partners.Count > 0)
            {
                Add(Para(14.5, R($"cross-references ({x.Partners.Count})", bold: true)));
                foreach (var p in x.Partners.Take(40))
                {
                    int wIdx = Weaves?.Weaves.FindIndex(w => w.Name == p.Weave) ?? -1;
                    Add(Para(13.5, Go(p.Verse, p.Display), "   ",
                        wIdx >= 0
                            ? H(p.Weave, $"weave:{wIdx}", Palette.InkFaded, 12)
                            : R(p.Weave, Palette.InkFaded, 12)));
                }
            }
        }

        if (full && hit.Verse.Length > 0 && Engine.StudyXrefsJson(hit.Verse) is { } sxj)
        {
            var sx = Wire.Parse<StudyXrefs>(sxj);
            if (sx.Refs.Count > 0)
            {
                Add(Para(14.5, R($"study cross-references ({sx.Refs.Count})", bold: true),
                    R("  TSK", Color.FromArgb(255, 136, 136, 136), 11.5)));
                foreach (var r2 in sx.Refs.Take(40))
                    Add(r2.End is null
                        ? Para(13.5, Go(r2.To, r2.ToDisplay))
                        : Para(13.5, Go(r2.To, r2.ToDisplay), "–", Go(r2.End, r2.EndDisplay!)));
                if (sx.Refs.Count > 40)
                    Add(Para(12, Italic($"… {sx.Refs.Count - 40} more")));
            }
        }

        if (full && hit.Verse.Length > 0 && Engine.SimilarVersesJson(hit.Verse, 6) is { } svj)
        {
            var s = Wire.Parse<SimilarVerses>(svj);
            if (s.In.Count > 0 || s.Cross.Count > 0)
            {
                Add(Para(14.5, R("verses like this", bold: true)));
                foreach (var v in s.In.Take(6)) Add(Para(13.5, Go(v.Verse, v.Display)));
                if (s.Cross.Count > 0)
                {
                    Add(Para(12, Italic("across the testaments:")));
                    foreach (var v in s.Cross.Take(4)) Add(Para(13.5, Go(v.Verse, v.Display)));
                }
            }
        }

        if (full && hit.Verse.Length > 0)
        {
            var tags = LoadTags();
            var holding = new List<(int i, Tag1 t)>();
            for (int i = 0; i < tags.Count; i++)
                if (tags[i].Members.Any(m => m.Kind == "verse" && m.Verse == hit.Verse))
                    holding.Add((i, tags[i]));
            if (holding.Count > 0)
            {
                Add(Para(14.5, R("tags", bold: true)));
                foreach (var (i, t) in holding)
                    Add(Para(13.5, H(t.Name, $"tag:{i}"), "  ",
                        H("✕", $"untag:{i}:{hit.Verse}", Palette.InkFaded)));
            }
        }

        if (hit.Verse.Length > 0 && Engine.VerseNotesJson(hit.Verse) is { } vnj)
        {
            var notes = Wire.Parse<VerseNotes>(vnj);
            Add(Para(14.5, R("margin notes", bold: true)));
            foreach (var n in notes.Notes)
                Add(Para(12.5, R(n, Palette.InkFaded)));
        }
    }

    private static string Humanize(string source) => source switch
    {
        "lxx" => "Septuagint",
        "quotation" => "NT quotation",
        _ => source,
    };

    // ── concordance ────────────────────────────────────────────────────────

    public void ShowConcordance(string code)
    {
        if (Engine is null) return;
        Fresh();
        string? lemma = Engine.StrongsJson(code) is { } sj
            ? Wire.Parse<StrongsEntry>(sj).Lemma : null;
        if (Engine.StrongsOccurrencesJson(code) is not { } oj)
        {
            Add(Para(14, Italic($"no occurrences of {code}")));
            return;
        }
        var occ = Wire.Parse<Occurrences>(oj);
        Add(Para(18, R(code, bold: true), lemma is not null ? $"  {lemma}" : ""));
        Add(Para(13, R($"{occ.Total} occurrence{(occ.Total == 1 ? "" : "s")}", Palette.Gold)));
        foreach (var v in occ.Verses.Take(300))
            Add(Para(13.5, Go(v)));
        if (occ.Total > 300)
            Add(Para(12, Italic($"… {occ.Total - 300} more")));
    }

    // ── threads / tags ─────────────────────────────────────────────────────

    public void ShowThreadsList()
    {
        Fresh();
        var threads = LoadThreads();
        Add(Para(18, R($"Threads ({threads.Count})", bold: true)));
        if (threads.Count == 0)
            Add(Para(13, Italic("No threads yet — open a word study and “＋ add to thread”.")));
        for (int i = 0; i < threads.Count; i++)
            Add(Para(14, H(threads[i].Name, $"thread:{i}"),
                R($"   {threads[i].Entries.Count} passage{(threads[i].Entries.Count == 1 ? "" : "s")}",
                    Palette.InkFaded, 12)));
    }

    public void ShowThreadDetail(int index)
    {
        var threads = LoadThreads();
        if (index >= threads.Count) { ShowThreadsList(); return; }
        var t = threads[index];
        Fresh();
        Add(Para(18, R(t.Name, bold: true)));
        Add(Para(13, R($"{t.Entries.Count} passage{(t.Entries.Count == 1 ? "" : "s")}", Palette.InkFaded),
            "   ", H("✎ notes", $"editthreadnotes:{index}", Palette.InkFaded, 12)));
        if (t.Notes.Length > 0) Add(Para(12.5, R(t.Notes, Palette.InkFaded)));
        for (int e = 0; e < t.Entries.Count; e++)
        {
            var en = t.Entries[e];
            Add(RuleLine());
            Add(Para(13.5, Go(en.Verse, en.Display), "   ",
                H("✎ note", $"editentrynote:{index}:{e}", Palette.InkFaded, 12)));
            var snap = string.Join(" ", en.Text);
            if (snap.Length > 70) snap = snap[..70].TrimEnd() + "…";
            if (snap.Length > 0) Add(Para(12.5, Italic($"“{snap}”")));
            if (!string.IsNullOrEmpty(en.Note))
                Add(Para(12.5, R($"— {en.Note}", Color.FromArgb(255, 136, 136, 136))));
        }
    }

    public void ShowTagsList()
    {
        Fresh();
        var tags = LoadTags();
        Add(Para(18, R($"Tags ({tags.Count})", bold: true)));
        if (tags.Count == 0)
            Add(Para(13, Italic("No tags yet — open a word study and “＋ tag verse”.")));
        for (int i = 0; i < tags.Count; i++)
            Add(Para(14, H(tags[i].Name, $"tag:{i}"),
                R($"   {tags[i].Members.Count} member{(tags[i].Members.Count == 1 ? "" : "s")}",
                    Palette.InkFaded, 12)));
    }

    public void ShowTagDetail(int index)
    {
        var tags = LoadTags();
        if (index >= tags.Count) { ShowTagsList(); return; }
        var t = tags[index];
        Fresh();
        Add(Para(18, R(t.Name, bold: true)));
        foreach (var m in t.Members)
        {
            var p = m.Kind == "verse" && m.Verse is not null
                ? Para(13.5, Go(m.Verse, m.Display ?? m.Verse))
                : Para(13.5, H($"≈ {m.Strongs}", $"occ:{m.Strongs}"));
            if (!string.IsNullOrEmpty(m.Note))
                p.Inlines.Add(R($"   {m.Note}", Color.FromArgb(255, 136, 136, 136), 12));
            Add(p);
        }
    }

    /// The whole weave library, flat: name → compare card. (The constellation
    /// is the graphical view of the same list.)
    public void ShowWeavesList()
    {
        Fresh();
        var ws = Weaves?.Weaves ?? new List<Weave1>();
        Add(Para(18, R($"Weaves ({ws.Count})", bold: true)));
        foreach (var w in ws.OrderByDescending(x => x.Links.Count))
            Add(Para(14, H(w.Name, $"weave:{w.Index}"),
                R($"   {w.KindLabel} · {w.Links.Count} link{(w.Links.Count == 1 ? "" : "s")}" +
                  (w.Suggested ? " · suggested" : ""), Palette.InkFaded, 12)));
    }

    // ── suggested weaves ───────────────────────────────────────────────────

    public void ShowSuggested()
    {
        if (Engine?.SuggestedWeavesJson() is not { } json) return;
        Fresh();
        var suggested = Wire.Parse<SuggestedWeaves>(json).Suggested;
        Add(Para(18, R($"Suggested weaves ({suggested.Count})", bold: true)));
        if (suggested.Count == 0)
            Add(Para(13, Italic("The review queue is empty (weaves/suggested).")));
        foreach (var w in suggested)
        {
            Add(RuleLine());
            int libIdx = Weaves?.Weaves.FindIndex(x => x.Suggested && x.Name == w.Name) ?? -1;
            Add(Para(15, R(w.Name, bold: true),
                R($"   {w.Kind}", Color.FromArgb(255, 136, 136, 136), 12)));
            if (w.Notes.Length > 0) Add(Para(12.5, R(w.Notes, Palette.InkFaded)));
            foreach (var l in w.Links.Take(40))
                Add(Para(13.5, Go(l.A, l.ADisplay), "  ↔  ", Go(l.B, l.BDisplay),
                    l.Label.Length > 0 ? R($"   {l.Label}", Palette.InkFaded, 12) : R("")));
            if (w.Links.Count > 40)
                Add(Para(12, Italic($"… {w.Links.Count - 40} more")));
            var actions = Para(13.5,
                libIdx >= 0 ? H("⇔ compare", $"weave:{libIdx}") : R(""), "   ",
                H("✓ approve", $"approve:{w.Index}"), "   ",
                H("✕ reject", $"reject:{w.Index}"), "   ",
                libIdx >= 0 ? H("✎ note", $"editweavenotes:{libIdx}", Palette.InkFaded, 12) : R(""));
            Add(actions);
        }
    }

    // ── weave compare card ─────────────────────────────────────────────────

    public void ShowCompareCard(int weaveIndex)
    {
        if (Engine is null || Weaves is null || weaveIndex >= Weaves.Weaves.Count) return;
        var w = Weaves.Weaves[weaveIndex];
        Fresh();
        Add(Para(18, R(w.Name, bold: true),
            R($"   {w.KindLabel}{(w.Suggested ? "  (suggested)" : "")}",
                Color.FromArgb(255, 136, 136, 136), 12)));
        var head = Para(13, R($"{w.Links.Count} link{(w.Links.Count == 1 ? "" : "s")}", Palette.InkFaded));
        if (IsFull())
        {
            head.Inlines.Add(new Run { Text = "   " });
            head.Inlines.Add(H("✎ note", $"editweavenotes:{weaveIndex}", Palette.InkFaded, 12));
        }
        Add(head);
        if (w.Notes.Length > 0) Add(Para(12.5, R(w.Notes, Palette.InkFaded)));

        foreach (var l in w.Links.Take(40))
        {
            Add(RuleLine());
            if (l.Label.Length > 0)
                Add(Para(12.5, R($"“{l.Label}”", Palette.Gold)));
            AddCompareSide(l.A, l.ADisplay, l.SpanA);
            AddCompareSide(l.B, l.BDisplay, l.SpanB);
        }
        if (w.Links.Count > 40)
            Add(Para(12, Italic($"… {w.Links.Count - 40} more")));
    }

    /// One side of a compare card: the verse link, then its text small with
    /// span words bold and added words italic gray.
    private void AddCompareSide(string refKey, string display, ushort[]? span)
    {
        Add(Para(13.5, Go(refKey, display)));
        if (Engine?.VerseJson(refKey) is not { } vj) return;
        var v = Wire.Parse<VerseData>(vj);
        var tb = new TextBlock
        {
            FontSize = 12.5, TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Palette.Ink),
            Margin = new Thickness(10, 0, 0, 0),
        };
        for (int ti = 0; ti < v.Tokens.Count; ti++)
        {
            var t = v.Tokens[ti];
            bool inSpan = span is not null && ti >= span[0] && ti <= span[1];
            bool added = (t.Flags & PureFlags.Added) != 0;
            tb.Inlines.Add(R(t.Render + " ",
                added ? Palette.InkFaded : Palette.Ink,
                bold: inSpan, italic: added));
        }
        Add(tb);
    }

    // ── search results ─────────────────────────────────────────────────────

    public void ShowSearch(string query, SearchResult r)
    {
        Fresh();
        if (r.Kind == "goto" && r.Book is not null && r.Chapter is not null)
        {
            var uri = r.Verse is { } v ? $"go:{r.Book}:{r.Chapter}:{v}" : $"go:{r.Book}:{r.Chapter}";
            Add(Para(17, H($"go to {r.Display}", uri)));
            return;
        }
        Add(Para(15, R($"{r.Total} result{(r.Total == 1 ? "" : "s")}", bold: true)));
        if (!string.IsNullOrEmpty(r.How)) Add(Para(12, Italic(r.How!)));
        foreach (var h in r.Hits ?? new())
        {
            var p = Para(13.5, Go(h.Verse, h.Display));
            if (h.Why.Length > 0)
                p.Inlines.Add(R($"   {h.Why}", Color.FromArgb(255, 136, 136, 136), 12));
            if (h.Note)
                p.Inlines.Add(R("   ※ note", Palette.Gold, 12));
            Add(p);
            if (Snippet(h.Verse, query) is { } snip) Add(snip);
        }
        if (r.Capped == true)
            Add(Para(12, Italic($"… {r.Total - (r.Hits?.Count ?? 0)} more")));
    }

    /// A one-line context snippet for a search hit: the verse text windowed
    /// around the first match of the query's first word, match emboldened.
    private TextBlock? Snippet(string refKey, string query)
    {
        if (Engine?.VerseJson(refKey) is not { } vj) return null;
        var body = Wire.Parse<VerseData>(vj).Body;
        if (body.Length == 0) return null;

        var needle = query.Split(' ', StringSplitOptions.RemoveEmptyEntries).FirstOrDefault() ?? "";
        int at = needle.Length > 0
            ? body.IndexOf(needle, StringComparison.OrdinalIgnoreCase) : -1;

        const int Window = 46;
        int start = at < 0 ? 0 : Math.Max(0, at - Window);
        int end = at < 0 ? Math.Min(body.Length, 2 * Window)
            : Math.Min(body.Length, at + needle.Length + Window);
        // Snap to word boundaries.
        while (start > 0 && body[start - 1] != ' ') start--;
        while (end < body.Length && body[end] != ' ') end++;

        var tb = new TextBlock
        {
            FontSize = 12,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Palette.InkFaded),
            Margin = new Thickness(12, -4, 0, 2),
        };
        if (start > 0) tb.Inlines.Add(new Run { Text = "…" });
        if (at >= 0 && at >= start)
        {
            tb.Inlines.Add(new Run { Text = body[start..at] });
            tb.Inlines.Add(R(body[at..(at + needle.Length)], Palette.Ink, bold: true));
            tb.Inlines.Add(new Run { Text = body[(at + needle.Length)..end] });
        }
        else
        {
            tb.Inlines.Add(new Run { Text = body[start..end] });
        }
        if (end < body.Length) tb.Inlines.Add(new Run { Text = "…" });
        return tb;
    }

    // ── prompts ────────────────────────────────────────────────────────────

    private async Task<string?> PromptName(string title, string placeholder)
    {
        var box = new TextBox { PlaceholderText = placeholder, MinWidth = 300 };
        var dialog = new ContentDialog
        {
            Title = title,
            Content = box,
            PrimaryButtonText = "OK",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot,
        };
        var result = await dialog.ShowAsync();
        var name = box.Text.Trim();
        return result == ContentDialogResult.Primary && name.Length > 0 ? name : null;
    }

    private async Task<string?> PromptText(string title, string prefill)
    {
        var box = new TextBox
        {
            Text = prefill, MinWidth = 340, AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap, MinHeight = 90,
        };
        var dialog = new ContentDialog
        {
            Title = title,
            Content = box,
            PrimaryButtonText = "Save",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot,
        };
        var result = await dialog.ShowAsync();
        // Empty submission is allowed — it clears.
        return result == ContentDialogResult.Primary ? box.Text.Trim() : null;
    }
}
