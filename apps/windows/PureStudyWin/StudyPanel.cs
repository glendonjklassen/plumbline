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
    private readonly Border _frame;

    public StudyEngine? Engine;
    public Func<bool> IsFull = () => false;
    /// Navigate the active pane (book, chapter, optional verse refKey to band).
    public Action<string, uint, string?> Navigate = (_, _, _) => { };
    /// Navigate the *other* pane (modifier-click a go: link; Tier 0 #8).
    public Action<string, uint, string?> NavigateOther = (_, _, _) => { };
    public Action<string> OpenConceptMap = _ => { };
    /// The last word study shown, so a note edit can re-render it in place.
    private Hit? _lastHit;
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
        _frame = new Border
        {
            Background = new SolidColorBrush(Palette.PanelBg),
            BorderBrush = new SolidColorBrush(Palette.Rule),
            BorderThickness = new Thickness(1, 0, 0, 0),
            Child = _scroll,
        };
        Content = _frame;
    }

    /// Re-theme the panel frame after a palette change (Tier 0 #5). Base text
    /// follows the element theme, so the on-screen content stays readable; the
    /// accent runs re-colour on the next view open.
    public void ApplyTheme()
    {
        _frame.Background = new SolidColorBrush(Palette.PanelBg);
        _frame.BorderBrush = new SolidColorBrush(Palette.Rule);
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
        // The verb vocabulary is parsed once in the core (pure_route_link_json);
        // the shell dispatches on the typed shape, never re-splitting the URI.
        if (StudyEngine.RouteLinkJson(uri) is not { } j) return;
        var link = Wire.Parse<PanelLinkData>(j);
        switch (link.Verb)
        {
            case "go":
                if (link.Book is { } b && link.Chapter is { } ch)
                {
                    string? verse = link.Verse is { } v ? $"{b} {ch}:{v}" : null;
                    // Modifier-click (Shift/Ctrl) opens the link in the other pane.
                    (ModifierDown() ? NavigateOther : Navigate)(b, ch, verse);
                }
                break;
            case "occurrences": ShowConcordance(link.Code!); break;
            case "rendering": ShowConcordanceFiltered(link.Code!, link.Rendering!); break;
            case "codeStudy": ShowCodeStudy(link.Code!, link.Word ?? ""); break;
            case "thread": ShowThreadDetail(link.Index!.Value); break;
            case "tag": ShowTagDetail(link.Index!.Value); break;
            case "weave": ShowCompareCard(link.Index!.Value); break;
            case "conceptMap": OpenConceptMap(link.Code!); break;
            case "addTag":
            {
                var refKey = link.RefKey!;
                var name = await PromptName($"Tag {DisplayOf(refKey)}", "tag name (new or existing)");
                if (name is null) break;
                var err = Engine.TagAdd(name, "verse", refKey, null, Now());
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                var idx = FindTag(name);
                if (idx >= 0) ShowTagDetail(idx);
                break;
            }
            case "addThread":
            {
                var refKey = link.RefKey!;
                var name = await PromptName($"Add {DisplayOf(refKey)} to thread", "thread name (new or existing)");
                if (name is null) break;
                var err = Engine.ThreadAdd(name, refKey, null, Now());
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                var idx = FindThread(name);
                if (idx >= 0) ShowThreadDetail(idx);
                break;
            }
            case "untag":
            {
                int idx2 = link.Tag!.Value;
                var tags = LoadTags();
                if (idx2 >= tags.Count) break;
                var err = Engine.TagRemove(tags[idx2].Name, "verse", link.RefKey!);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                var again = FindTag(tags[idx2].Name);
                if (again >= 0) ShowTagDetail(again); else ShowTagsList();
                break;
            }
            case "approve":
            case "reject":
            {
                var si = (uint)link.Index!.Value;
                var err = link.Verb == "approve" ? Engine.WeaveApprove(si) : Engine.WeaveReject(si);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                ShowSuggested();
                break;
            }
            case "editThreadNotes":
            {
                int i3 = link.Index!.Value;
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
            case "editEntryNote":
            {
                int t4 = link.Thread!.Value;
                uint e4 = (uint)link.Entry!.Value;
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
            case "editWeaveNotes":
            {
                int i5 = link.Index!.Value;
                if (Weaves is null || i5 >= Weaves.Weaves.Count) break;
                var w = Weaves.Weaves[i5];
                var text = await PromptText($"Notes — {w.Name}", w.Notes);
                if (text is null) break;
                var err = Engine.WeaveSetNotes(w.Name, text);
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                ShowCompareCard(i5);
                break;
            }
            case "editNote":
            {
                var refKey = link.RefKey!;
                var current = Engine.UserNoteJson(refKey) is { } nj ? Wire.Parse<UserNote>(nj).Text : "";
                var text = await PromptText($"Your note — {DisplayOf(refKey)}", current);
                if (text is null) break;
                var err = Engine.UserNoteSet(refKey, text, Now());
                if (err is not null) { ShowError(err); break; }
                StudyDataChanged();
                // Re-render the word study so the note line updates in place.
                if (_lastHit is { } h) ShowWordStudy(h);
                break;
            }
            case "guide": ShowGuide(); break;
            case "about": ShowAbout(); break;
        }
    }

    /// True when Shift or Ctrl is held (modifier-click routing).
    private static bool ModifierDown()
    {
        bool Down(Windows.System.VirtualKey k) =>
            Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(k)
                .HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
        return Down(Windows.System.VirtualKey.Shift) || Down(Windows.System.VirtualKey.Control);
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
        // No explicit base Foreground: "ink" runs inherit the element theme's
        // text colour, so the panel stays readable across a live theme switch.
        var tb = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true,
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
        var run = new Run { Text = r.Text, FontSize = r.Size };
        // "ink" (and unknown) inherits the themed default; accents are explicit.
        if (r.Color is not (null or "ink"))
            run.Foreground = new SolidColorBrush(ColorOf(r.Color));
        if (r.Bold) run.FontWeight = Microsoft.UI.Text.FontWeights.Bold;
        if (r.Italic) run.FontStyle = Windows.UI.Text.FontStyle.Italic;
        return run;
    }

    /// A semantic colour role → this shell's palette. Every shell maps these
    /// identically, so the panel reads the same on each platform.
    private static Color ColorOf(string? role) => role switch
    {
        "ink" => Palette.Ink,
        "faded" => Palette.Faded,
        "gold" => Palette.Gold,
        "section" => Palette.SectionGold,
        "tierGod" => Palette.TierGod,
        "tierHuman" => Palette.TierHuman,
        "tierMachine" => Palette.TierMachine,
        "tierResearch" => Palette.TierResearch,
        "mono" => Palette.Mono,
        "morph" => Palette.Morph,
        "lemma" => Palette.Lemma,
        _ => Palette.Ink,
    };

    // ── the views: fetch the block list, paint it ─────────────────────────

    public void ShowWordStudy(Hit hit)
    {
        if (Engine?.WordStudyBlocksJson(hit.Verse, hit.TokenIndex, IsFull()) is { } j)
        {
            _lastHit = hit;
            Fresh();
            RenderBlocks(j);
        }
    }

    /// The in-app guide / About card (Tier 0 #7); static content from the core.
    public void ShowGuide() { Fresh(); RenderBlocks(StudyEngine.GuideBlocksJson()); }
    public void ShowAbout() { Fresh(); RenderBlocks(StudyEngine.AboutBlocksJson()); }

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
