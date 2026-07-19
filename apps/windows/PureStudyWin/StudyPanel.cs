// The on-demand study sidebar: word study, concordance, threads/tags
// browsers, the suggested-weave review queue, weave compare cards, and
// search results. Every view is now a **typed block list** built by one Rust
// producer (pure_core::panel) and served over the pure_engine_*_blocks_json
// endpoints; this shell only walks the blocks and paints (RenderBlocks). All
// interactivity funnels through one URI dispatcher that mirrors the GTK shell's
// handle_link scheme exactly (FEATURE-MANIFEST.md).

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
    /// Current weave library (shell keeps it fresh; the router's edit verbs index it).
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
            case "rend":
            {
                var (rcode, rword) = Split2(rest, ':');
                ShowConcordanceFiltered(rcode, rword);
                break;
            }
            case "code":
            {
                var (ccode, cword) = Split2(rest, ':');
                ShowCodeStudy(ccode, cword);
                break;
            }
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

    // ── the block renderer (one small per-block painter) ───────────────────

    private void Add(UIElement e) => _body.Children.Add(e);

    private void ShowError(string message) =>
        _body.Children.Insert(0, new TextBlock
        {
            FontSize = 13,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Palette.InkFaded),
            FontStyle = Windows.UI.Text.FontStyle.Italic,
            Text = message,
        });

    /// Walk the core's typed block list and paint each block. This is the whole
    /// shell-side rendering surface — no derivation, tier order, caps, humanize,
    /// or gloss/lemma formatting live here anymore (they moved to pure_core::panel).
    private void RenderBlocks(string json)
    {
        foreach (var b in Wire.Parse<PanelData>(json).Blocks)
            switch (b.Kind)
            {
                case "rule": Add(RuleLine()); break;
                case "section": Add(SectionBlock(b)); break;
                case "para": Add(ParaBlock(b)); break;
            }
    }

    private static Border RuleLine() => new()
    {
        BorderBrush = new SolidColorBrush(Palette.Rule),
        BorderThickness = new Thickness(0, 1, 0, 0),
        Margin = new Thickness(0, 4, 0, 0),
    };

    /// A spaced, muted-gold section header + an optional tier mark glyph.
    private static TextBlock SectionBlock(PanelBlock b)
    {
        var tb = new TextBlock
        {
            FontSize = 11,
            CharacterSpacing = 120,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            Foreground = new SolidColorBrush(Palette.SectionGold),
            Margin = new Thickness(0, 8, 0, 0),
        };
        tb.Inlines.Add(new Run { Text = b.Title ?? "" });
        if (b.MarkGlyph is { } glyph)
            tb.Inlines.Add(new Run
            {
                Text = "  " + glyph,
                Foreground = new SolidColorBrush(ColorOf(b.MarkColor)),
                FontWeight = Microsoft.UI.Text.FontWeights.Normal,
                FontSize = 10,
                CharacterSpacing = 0,
            });
        return tb;
    }

    /// A flowing paragraph of styled runs; link runs route back through Link().
    private TextBlock ParaBlock(PanelBlock b)
    {
        var runs = b.Runs ?? new();
        var tb = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true,
            Foreground = new SolidColorBrush(Palette.Ink),
            FontSize = runs.Count > 0 ? runs[0].Size : 14,
            Margin = new Thickness(b.Indent ? 12 : 0, b.TopGap ? 6 : 0, 0, 0),
        };
        foreach (var run in runs)
        {
            if (run.Uri is { } uri)
            {
                var link = new Hyperlink { UnderlineStyle = UnderlineStyle.None };
                link.Inlines.Add(RunOf(run));
                link.Click += (_, _) => Link(uri);
                tb.Inlines.Add(link);
            }
            else
            {
                tb.Inlines.Add(RunOf(run));
            }
        }
        return tb;
    }

    private static Run RunOf(PanelRun r)
    {
        var run = new Run
        {
            Text = r.Text,
            FontSize = r.Size,
            Foreground = new SolidColorBrush(ColorOf(r.Color)),
        };
        if (r.Bold) run.FontWeight = Microsoft.UI.Text.FontWeights.Bold;
        if (r.Italic) run.FontStyle = Windows.UI.Text.FontStyle.Italic;
        return run;
    }

    /// A semantic colour role → this shell's palette. Every shell maps these
    /// identically, so the panel reads the same on each platform.
    private static Color ColorOf(string? role) => role switch
    {
        "ink" => Palette.Ink,
        "faded" => Palette.InkFaded,
        "gold" => Palette.Gold,
        "section" => Palette.SectionGold,
        "tierGod" => Palette.TierGod,
        "tierHuman" => Palette.TierHuman,
        "tierMachine" => Palette.TierMachine,
        "tierResearch" => Palette.TierResearch,
        "mono" => Color.FromArgb(255, 136, 136, 136),
        "morph" => Color.FromArgb(255, 106, 90, 42),
        "lemma" => Color.FromArgb(255, 138, 122, 82),
        _ => Palette.Ink,
    };

    // ── the views: fetch the block list, paint it ─────────────────────────

    public void ShowWordStudy(Hit hit)
    {
        if (Engine?.WordStudyBlocksJson(hit.Verse, hit.TokenIndex, IsFull()) is { } j)
        {
            Fresh();
            RenderBlocks(j);
        }
    }

    /// The standalone `code:CODE[:word]` study card (reverse rendering-lens target).
    public void ShowCodeStudy(string code, string word)
    {
        if (Engine?.CodeStudyBlocksJson(code, word.Length == 0 ? null : word, IsFull()) is { } j)
        {
            Fresh();
            RenderBlocks(j);
        }
    }

    public void ShowConcordance(string code)
    {
        if (Engine?.ConcordanceBlocksJson(code) is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowConcordanceFiltered(string code, string rendering)
    {
        if (Engine?.RenderingConcordanceBlocksJson(code, rendering) is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowThreadsList()
    {
        if (Engine?.ThreadsBlocksJson() is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowThreadDetail(int index)
    {
        if (Engine?.ThreadBlocksJson((uint)index) is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowTagsList()
    {
        if (Engine?.TagsBlocksJson() is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowTagDetail(int index)
    {
        if (Engine?.TagBlocksJson((uint)index) is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowWeavesList()
    {
        if (Engine?.WeavesBlocksJson() is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowSuggested()
    {
        if (Engine?.SuggestedBlocksJson() is { } j) { Fresh(); RenderBlocks(j); }
    }

    public void ShowCompareCard(int weaveIndex)
    {
        if (Engine?.CompareBlocksJson((uint)weaveIndex, IsFull()) is { } j) { Fresh(); RenderBlocks(j); }
    }

    /// Search results (the panel runs the query itself now); MainWindow keeps
    /// the direct-navigate short-circuit for a submitted reference.
    public void ShowSearchBlocks(string query)
    {
        if (Engine?.SearchBlocksJson(query) is { } j) { Fresh(); RenderBlocks(j); }
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
