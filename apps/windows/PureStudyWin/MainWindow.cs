// The shell window: header (study tools + search + mode), 1–3 reading panes
// with the weave-connector overlay, the canon strip, and the study panel.
// All study logic lives across the ABI — this file is orchestration only.
// Behaviors mirror the GTK shell (docs/FEATURE-MANIFEST.md).

using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using PureStudy;
using Windows.System;

namespace PureStudyWin;

public sealed class MainWindow : Window
{
    private StudyEngine? _engine;
    private List<TocBook> _books = new();
    private WeaveLib? _weaves;

    private readonly List<PaneView> _panes = new();
    private int _active;
    private bool _fullMode;
    private bool _versePerLine;
    private float _fontSize = 18f;

    private readonly Grid _paneHost = new();
    private readonly ConnectorLayer _connectors = new();
    private readonly CanonStrip _strip = new();
    private readonly StudyPanel _panel = new();

    private readonly TextBox _search = new()
    {
        PlaceholderText = "search — word, phrase, or reference", MinWidth = 280, IsEnabled = false,
    };
    private readonly Button _threadsBtn = new() { Content = "Threads", IsEnabled = false };
    private readonly Button _tagsBtn = new() { Content = "Tags", IsEnabled = false };
    private readonly Button _weavesBtn = new() { Content = "Weaves", IsEnabled = false };
    private readonly Button _suggestedBtn = new() { Content = "Suggested", IsEnabled = false };
    private readonly Button _mapBtn = new() { Content = "Map", IsEnabled = false };
    private readonly Button _constBtn = new() { Content = "Constellation", IsEnabled = false };
    private readonly Button _linkBtn = new() { Content = "＋ link", IsEnabled = false };
    private readonly Button _modeBtn = new() { Content = "Simple reader", IsEnabled = false };
    private readonly Button _vplBtn = new() { Content = "Flowing text", IsEnabled = false };
    // Always available (no engine needed): the theme toggle and the Help button.
    private readonly Button _themeBtn = new() { Content = "Theme: system" };
    private readonly Button _helpBtn = new() { Content = "?" };
    private string _themeChoice = "system";
    private readonly TextBlock _status = new()
    {
        VerticalAlignment = VerticalAlignment.Center,
        Foreground = new SolidColorBrush(Palette.InkFaded),
        Text = "loading corpus…",
    };

    public MainWindow()
    {
        Title = "pure study";
        AppWindow.Resize(new Windows.Graphics.SizeInt32(1500, 1000));
        var iconPath = System.IO.Path.Combine(AppContext.BaseDirectory, "Assets", "pure-study.ico");
        if (System.IO.File.Exists(iconPath)) AppWindow.SetIcon(iconPath);

        // Resolve + apply the colour theme before building the UI, so chrome and
        // brushes pick it up (Tier 0 #5). Config load is engine-independent.
        try
        {
            _themeChoice = Wire.Parse<ConfigState>(StudyConfig.LoadJson()).Theme ?? "system";
            Palette.ApplyTheme(ResolveTheme(_themeChoice));
        }
        catch { _themeChoice = "system"; /* keep the default light palette */ }
        _themeBtn.Content = ThemeLabel(_themeChoice);

        var header = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            Padding = new Thickness(10, 8, 10, 8),
        };
        header.Children.Add(_threadsBtn);
        header.Children.Add(_tagsBtn);
        header.Children.Add(_weavesBtn);
        header.Children.Add(_suggestedBtn);
        header.Children.Add(_mapBtn);
        header.Children.Add(_constBtn);
        header.Children.Add(_linkBtn);
        header.Children.Add(_search);
        header.Children.Add(_modeBtn);
        header.Children.Add(_vplBtn);
        header.Children.Add(_themeBtn);
        header.Children.Add(_helpBtn);
        header.Children.Add(_status);

        var centre = new Grid();
        centre.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        centre.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Grid.SetRow(_paneHost, 0);
        Grid.SetRow(_strip, 1);
        centre.Children.Add(_paneHost);
        centre.Children.Add(_strip);

        var main = new Grid();
        main.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        main.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(centre, 0);
        Grid.SetColumn(_panel, 1);
        main.Children.Add(centre);
        main.Children.Add(_panel);

        var root = new Grid
        {
            RequestedTheme = Palette.Dark ? ElementTheme.Dark : ElementTheme.Light,
            // Accelerators (Esc, brackets, Ctrl+…) must not surface key-tip
            // tooltips that linger over the reader.
            KeyboardAcceleratorPlacementMode = KeyboardAcceleratorPlacementMode.Hidden,
        };
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(header, 0);
        Grid.SetRow(main, 1);
        root.Children.Add(header);
        root.Children.Add(main);
        Content = root;

        // Re-apply the theme to the field-initialized chrome (their brushes were
        // captured with the default light palette before the theme was applied).
        ApplyThemeToUi();
        WireHeader(root);
        Closed += (_, _) =>
        {
            PersistConfig();
            foreach (var p in _panes) p.Dispose();
            _engine?.Dispose();
        };
        _ = LoadEngineAsync();
    }

    // ── startup / config ───────────────────────────────────────────────────

    private static string? FindHome()
    {
        foreach (var v in new[] { "PURE_STUDY_HOME", "OVERLAY_HOME" })
        {
            var p = Environment.GetEnvironmentVariable(v);
            if (!string.IsNullOrEmpty(p)) return p;
        }
        foreach (var start in new[] { AppContext.BaseDirectory, Environment.CurrentDirectory })
        {
            var dir = new DirectoryInfo(start);
            for (int i = 0; i < 10 && dir is not null; i++, dir = dir.Parent)
                if (File.Exists(Path.Combine(dir.FullName, "data", "kjv.jsonl")))
                    return dir.FullName;
        }
        return null;
    }

    private async Task LoadEngineAsync()
    {
        // Startup is fire-and-forget (`_ = LoadEngineAsync()`), so a faulted
        // task is never observed — anything thrown here (e.g. a missing
        // pure_ffi.dll on the very first native call) would otherwise vanish
        // and leave the window stuck on "loading corpus…".
        try
        {
            await LoadEngineCoreAsync();
        }
        catch (Exception e)
        {
            _status.Text = $"startup failed: {e.Message}";
        }
    }

    private async Task LoadEngineCoreAsync()
    {
        var cfg = Wire.Parse<ConfigState>(StudyConfig.LoadJson());
        _fullMode = cfg.StudyMode == "full";
        _fontSize = (float)Math.Clamp(cfg.BodySize is > 6 and < 96 ? cfg.BodySize : 18.0, 12, 48);
        _versePerLine = cfg.VersePerLine;

        var home = FindHome();
        if (home is null)
        {
            _status.Text = "no data home found (set PURE_STUDY_HOME)";
            return;
        }
        try
        {
            _engine = await Task.Run(() => StudyEngine.Open(home));
        }
        catch (Exception e)
        {
            _status.Text = $"could not open corpus: {e.Message}";
            return;
        }

        _books = Wire.Parse<Toc>(_engine.TocJson()).Books;
        _panel.Engine = _engine;
        _panel.IsFull = () => _fullMode;
        _panel.Navigate = (book, ch, verse) => NavigateActive(book, ch, verse);
        _panel.NavigateOther = (book, ch, verse) => NavigateOtherPane(book, ch, verse);
        _panel.OpenConceptMap = code => Popups.ConceptMap(_engine!, code, _fullMode);
        _panel.StudyDataChanged = RefreshStudyData;

        // Restore the session's panes (≤3; default John 3).
        var panes = (cfg.OpenPanes is { Count: > 0 } op ? op : new() { new PaneRef1("John", 3) })
            .Take(3).ToList();
        foreach (var p in panes) AddPaneInternal(p.Book, p.Chapter);
        _active = Math.Clamp(cfg.ActivePane, 0, _panes.Count - 1);
        RebuildPaneRow();

        foreach (var c in new Control[]
                 { _search, _threadsBtn, _tagsBtn, _weavesBtn, _suggestedBtn, _mapBtn, _constBtn, _modeBtn, _vplBtn })
            c.IsEnabled = true;
        _status.Text = "";
        // Warm the analytics indexes off the UI thread in Full mode, so the
        // first study click doesn't stall building them (Tier 0 #6).
        if (_fullMode && _engine is { } warmEngine)
            _ = Task.Run(() => { try { warmEngine.WarmIndexes(); } catch { /* best-effort */ } });
        ApplyMode(persist: false);
        ApplyVersePerLine(persist: false);
        // Canon bands from the core view-model — the single app-wide source for
        // the strip and the map popups; no shell hardcode (item 5).
        if (_engine?.CanonSegmentsJson() is { } csj)
            Canon.Set(Wire.Parse<CanonSegments>(csj));
        RefreshStudyData();
        _strip.SetBooks(_books);
        _panes[_active].Reader.Focus(FocusState.Programmatic);

        if (cfg.FirstRun) await FirstRunDialogAsync();
    }

    private void PersistConfig()
    {
        var state = new ConfigState(
            _fullMode ? "full" : "simple",
            _fontSize,
            _panes.Select(p => new PaneRef1(p.Reader.Book, (ushort)p.Reader.ChapterNumber)).ToList(),
            _active,
            false,
            _versePerLine,
            _themeChoice);
        StudyConfig.SaveJson(System.Text.Json.JsonSerializer.Serialize(state, Wire.Options));
    }

    private async Task FirstRunDialogAsync()
    {
        var dialog = new ContentDialog
        {
            Title = "Welcome to pure-study",
            Content = new TextBlock
            {
                TextWrapping = TextWrapping.Wrap,
                Text = "Simple reader — just the text: chapters, search, and a double-click "
                     + "for Strong's.\n\nFull study — everything: threads, tags, weave "
                     + "cross-references and authoring, and the review queue.",
            },
            PrimaryButtonText = "Simple reader",
            SecondaryButtonText = "Full study",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        _fullMode = result == ContentDialogResult.Secondary;
        ApplyMode(persist: true);
    }

    // ── panes ──────────────────────────────────────────────────────────────

    private void AddPaneInternal(string book, uint chapter)
    {
        var pane = new PaneView();
        int Idx() => _panes.IndexOf(pane);
        pane.Touched += () => SetActive(Idx());
        pane.AddRequested += () => AddPane(Idx());
        pane.CloseRequested += () => ClosePane(Idx());
        pane.Reader.WordActivated += hit => _panel.ShowWordStudy(hit);
        pane.Reader.Scrolled += () => _connectors.Redraw();
        pane.Reader.ChapterShown += (_, _) =>
        {
            if (Idx() == _active) UpdateTitle();
            UpdateStripPins();
            RefreshVerseDecorations(pane);
            _connectors.Redraw();
        };
        pane.Reader.ContextRequested += (verse, pt) => ShowContextMenu(pane, verse, pt);
        pane.Reader.PinChanged += UpdateLinkButton;
        pane.Reader.ZoomRequested += Zoom;
        pane.Reader.ScrollAllRequested += px => { foreach (var p in _panes) p.Reader.ScrollBy(px); };
        pane.Reader.FontSize = _fontSize;
        pane.Reader.VersePerLine = _versePerLine;
        _panes.Add(pane);
        if (_engine is not null)
        {
            pane.Reader.SetEngine(_engine);
            pane.SetBooks(_books);
            pane.Reader.ShowChapter(book, chapter);
        }
    }

    /// GTK add_pane: insert a copy of pane `after` right after it; make it active.
    private void AddPane(int after)
    {
        if (_panes.Count >= 3 || after < 0) return;
        var src = _panes[after];
        AddPaneInternal(src.Reader.Book, src.Reader.ChapterNumber);
        var pane = _panes[^1];
        _panes.RemoveAt(_panes.Count - 1);
        _panes.Insert(after + 1, pane);
        _active = after + 1;
        RebuildPaneRow();
        RefreshStudyData();
    }

    private void ClosePane(int i)
    {
        if (_panes.Count <= 1 || i < 0) return;
        _panes[i].Dispose();
        _panes.RemoveAt(i);
        _active = Math.Clamp(_active >= i ? _active - 1 : _active, 0, _panes.Count - 1);
        RebuildPaneRow();
    }

    private void RebuildPaneRow()
    {
        _paneHost.Children.Clear();
        _paneHost.ColumnDefinitions.Clear();
        for (int i = 0; i < _panes.Count; i++)
        {
            _paneHost.ColumnDefinitions.Add(
                new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            Grid.SetColumn(_panes[i], i);
            _paneHost.Children.Add(_panes[i]);
        }
        Grid.SetColumn(_connectors, 0);
        Grid.SetColumnSpan(_connectors, Math.Max(1, _panes.Count));
        _paneHost.Children.Add(_connectors);
        _connectors.Panes = _panes;
        SetActive(Math.Clamp(_active, 0, _panes.Count - 1));
        UpdateLinkButton();
    }

    private void SetActive(int i)
    {
        if (i < 0 || i >= _panes.Count) return;
        _active = i;
        for (int p = 0; p < _panes.Count; p++)
            _panes[p].SetChrome(p == _active, _panes.Count);
        UpdateTitle();
        UpdateStripPins();
    }

    private void UpdateTitle()
    {
        if (_panes.Count == 0 || _books.Count == 0) return;
        var r = _panes[_active].Reader;
        var name = _books.FirstOrDefault(b => b.Id == r.Book)?.Name ?? r.Book;
        Title = $"pure study — {name} {r.ChapterNumber} · 1769 KJV";
    }

    private void UpdateStripPins()
    {
        _strip.Pins = _panes
            .Select((p, i) => (_books.FindIndex(b => b.Id == p.Reader.Book), i == _active))
            .Where(t => t.Item1 >= 0)
            .ToList();
        _strip.Invalidate();
    }

    private void NavigateActive(string book, uint chapter, string? verse)
    {
        if (_panes.Count == 0) return;
        _panes[_active].Reader.ShowChapter(book, chapter, verse, highlight: verse is not null);
    }

    // ── study data (weave library → links, xrefs, panel) ──────────────────

    private void RefreshStudyData()
    {
        if (_engine?.WeavesJson() is not { } json) return;
        _weaves = Wire.Parse<WeaveLib>(json);
        _panel.Weaves = _weaves;

        // Connector pairs come from the core view-model (link_pairs): deduped,
        // canonical, each endpoint located — no shell-side dedup or ref parsing.
        // Only resolved pairs are drawable; their endpoints seed the reader's
        // xref gutter set (the same set GTK's build_xrefs feeds the gutter).
        var links = new List<LinkPair>();
        var xrefVerses = new HashSet<string>();
        if (_engine.LinkPairsJson() is { } lpj)
            foreach (var p in Wire.Parse<LinkPairs>(lpj).Pairs)
            {
                if (!p.Resolved) continue;
                links.Add(new LinkPair(p.A, p.B, p.ABook, p.AChapter, p.BBook, p.BChapter));
                xrefVerses.Add(p.A);
                xrefVerses.Add(p.B);
            }
        _connectors.Links = links;

        // Personal-note gutter marks: the whole note set (a global list), shared
        // by every pane (each paints the verses in its own chapter).
        var noteVerses = new HashSet<string>();
        if (_engine.UserNotesJson() is { } unj)
            foreach (var n in Wire.Parse<UserNotes>(unj).Notes)
                noteVerses.Add(n.Verse);

        foreach (var p in _panes)
        {
            p.Reader.XrefVerses = xrefVerses;
            p.Reader.NoteVerses = noteVerses;
            RefreshVerseDecorations(p);
            p.Reader.Redraw();
        }
        _connectors.Redraw();
        UpdateLinkButton();
    }

    /// Refresh the highlight washes for a pane's current chapter (its verses that
    /// belong to a colour-bearing tag). Called on nav + on any study-data change.
    private void RefreshVerseDecorations(PaneView pane)
    {
        if (_engine is null) return;
        var map = new Dictionary<string, Windows.UI.Color>();
        if (_engine.ChapterHighlightsJson(pane.Reader.Book, pane.Reader.ChapterNumber) is { } hj)
            foreach (var v in Wire.Parse<ChapterHighlights>(hj).Verses)
                map[v.Verse] = Palette.Hex(v.Color);
        pane.Reader.Highlights = map;
        pane.Reader.Redraw();
    }

    // ── header wiring ──────────────────────────────────────────────────────

    private void WireHeader(Grid root)
    {
        _threadsBtn.Click += (_, _) => _panel.ShowThreadsList();
        _tagsBtn.Click += (_, _) => _panel.ShowTagsList();
        _weavesBtn.Click += (_, _) => _panel.ShowWeavesList();
        _suggestedBtn.Click += (_, _) => _panel.ShowSuggested();
        _mapBtn.Click += (_, _) =>
        {
            // Book-pair density folded once in the core (pure_engine_chord_map_json);
            // the popup only lays out ribbons over it.
            if (_books.Count > 0 && _engine?.ChordMapJson() is { } cmj
                && Wire.Parse<ChordMapData>(cmj) is { Pairs.Count: > 0 } map)
                Popups.ChordMap(map, _books, (book) => NavigateActive(book, 1, null));
        };
        _constBtn.Click += (_, _) =>
        {
            // The layout comes from the core view-model (pure_engine_constellation_json);
            // the popup holds only the page + pin set and paints it.
            if (_engine is not null)
                Popups.Constellation(_engine,
                    (book, ch, verse) => NavigateActive(book, ch, verse),
                    i => _panel.ShowCompareCard(i));
        };
        _linkBtn.Click += (_, _) => _ = MakeLinkAsync();
        _modeBtn.Click += (_, _) =>
        {
            _fullMode = !_fullMode;
            ApplyMode(persist: true);
        };
        _vplBtn.Click += (_, _) =>
        {
            _versePerLine = !_versePerLine;
            ApplyVersePerLine(persist: true);
        };
        _themeBtn.Click += (_, _) => CycleTheme();
        _helpBtn.Click += (_, _) => _panel.ShowGuide();

        _search.TextChanged += (_, _) => RunSearch(_search.Text, live: true);
        _search.KeyDown += (_, e) =>
        {
            if (e.Key == VirtualKey.Enter) RunSearch(_search.Text, live: false);
            if (e.Key == VirtualKey.Escape) { _panel.Close(); FocusActive(); }
        };

        AddAccel(root, VirtualKey.F, VirtualKeyModifiers.Control,
            () => _search.Focus(FocusState.Programmatic));
        AddAccel(root, VirtualKey.Escape, VirtualKeyModifiers.None, () => { _panel.Close(); FocusActive(); });
        AddAccel(root, (VirtualKey)219 /* [ */, VirtualKeyModifiers.None, () => StepActive(-1));
        AddAccel(root, (VirtualKey)221 /* ] */, VirtualKeyModifiers.None, () => StepActive(+1));
        AddAccel(root, VirtualKey.Left, VirtualKeyModifiers.None, () => StepActive(-1), skipWhenTyping: true);
        AddAccel(root, VirtualKey.Right, VirtualKeyModifiers.None, () => StepActive(+1), skipWhenTyping: true);
        AddAccel(root, VirtualKey.Number0, VirtualKeyModifiers.Control, () => Zoom(0));
        AddAccel(root, (VirtualKey)0xBB /* =/+ */, VirtualKeyModifiers.Control, () => Zoom(+1));
        AddAccel(root, (VirtualKey)0xBD /* - */, VirtualKeyModifiers.Control, () => Zoom(-1));
        // Reading history: Alt+←/→ walk the active pane's history (Tier 0 #2).
        AddAccel(root, VirtualKey.Left, VirtualKeyModifiers.Menu, () => ActiveReader()?.GoBack(), skipWhenTyping: true);
        AddAccel(root, VirtualKey.Right, VirtualKeyModifiers.Menu, () => ActiveReader()?.GoForward(), skipWhenTyping: true);
        // Help: F1 (and ?) open the shortcuts overlay (Tier 0 #7).
        AddAccel(root, VirtualKey.F1, VirtualKeyModifiers.None, () => _ = ShowShortcutsAsync());
        AddAccel(root, (VirtualKey)0xBF /* / → ? */, VirtualKeyModifiers.Shift, () => _ = ShowShortcutsAsync(), skipWhenTyping: true);

        _strip.BookPicked += book => NavigateActive(book, 1, null);
    }

    private void FocusActive()
    {
        if (_panes.Count > 0) _panes[_active].Reader.Focus(FocusState.Programmatic);
    }

    /// Step the active pane a chapter, rolling across book boundaries (Tier 0 #8:
    /// keep pressing past the last chapter to enter the next book, and vice versa).
    private void StepActive(int dir)
    {
        if (_panes.Count == 0 || _books.Count == 0) return;
        var r = _panes[_active].Reader;
        int idx = _books.FindIndex(b => b.Id == r.Book);
        if (idx < 0) return;
        int ch = (int)r.ChapterNumber + dir;
        if (ch < 1)
        {
            if (idx > 0) { var prev = _books[idx - 1]; r.ShowChapter(prev.Id, (uint)Math.Max(1, (int)prev.Chapters)); }
        }
        else if (ch > _books[idx].Chapters)
        {
            if (idx < _books.Count - 1) r.ShowChapter(_books[idx + 1].Id, 1);
        }
        else
        {
            r.ShowChapter(r.Book, (uint)ch);
        }
    }

    private ReaderView? ActiveReader() =>
        _panes.Count > 0 ? _panes[Math.Clamp(_active, 0, _panes.Count - 1)].Reader : null;

    private void AddAccel(UIElement host, VirtualKey key, VirtualKeyModifiers mods,
        Action action, bool skipWhenTyping = false)
    {
        var a = new KeyboardAccelerator { Key = key, Modifiers = mods };
        a.Invoked += (_, e) =>
        {
            if (skipWhenTyping &&
                FocusManager.GetFocusedElement(Content.XamlRoot) is TextBox or NumberBox)
            {
                e.Handled = false;
                return;
            }
            action();
            e.Handled = true;
        };
        host.KeyboardAccelerators.Add(a);
    }

    /// GTK zoom(): 0 = reset to 18, else ±1 pt clamped 12–48; persists at once.
    private void Zoom(int dir)
    {
        _fontSize = dir == 0 ? 18f : Math.Clamp(_fontSize + dir, 12f, 48f);
        foreach (var p in _panes) p.Reader.FontSize = _fontSize;
        _connectors.Redraw();
        PersistConfig();
    }

    /// GTK vpl toggle: flow ↔ verse-per-line for every pane; persists at once.
    private void ApplyVersePerLine(bool persist)
    {
        _vplBtn.Content = _versePerLine ? "Verse / line" : "Flowing text";
        foreach (var p in _panes) p.Reader.VersePerLine = _versePerLine;
        _connectors.Redraw();
        if (persist) PersistConfig();
    }

    private void ApplyMode(bool persist)
    {
        _modeBtn.Content = _fullMode ? "Full study" : "Simple reader";
        var vis = _fullMode ? Visibility.Visible : Visibility.Collapsed;
        _threadsBtn.Visibility = vis;
        _tagsBtn.Visibility = vis;
        _weavesBtn.Visibility = vis;
        _suggestedBtn.Visibility = vis;
        _mapBtn.Visibility = vis;
        _constBtn.Visibility = vis;
        _linkBtn.Visibility = vis;
        if (!_fullMode) _panel.Close();
        if (persist) PersistConfig();
    }

    private void UpdateLinkButton() =>
        _linkBtn.IsEnabled = _fullMode && _panes.Count(p => p.Reader.Pin is not null) >= 2;

    private async Task MakeLinkAsync()
    {
        if (_engine is null) return;
        var pinned = _panes.Where(p => p.Reader.Pin is not null).Take(2).ToList();
        if (pinned.Count < 2) return;
        var a = pinned[0].Reader.Pin!;
        var b = pinned[1].Reader.Pin!;

        var box = new TextBox { PlaceholderText = "weave name (new or existing)", MinWidth = 300 };
        var dialog = new ContentDialog
        {
            Title = $"Weave {a.Verse} ↔ {b.Verse}",
            Content = box,
            PrimaryButtonText = "OK",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        var name = box.Text.Trim();
        if (name.Length == 0) return;

        var err = _engine.WeaveAddLinkSpans(
            name, a.Verse, b.Verse,
            ((int)a.Lo, (int)a.Hi), ((int)b.Lo, (int)b.Hi),
            DateTime.UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'"));
        if (err is not null)
        {
            _status.Text = err;
            return;
        }
        _status.Text = "";
        foreach (var p in _panes) p.Reader.ClearPin();
        RefreshStudyData();
    }

    // ── search ─────────────────────────────────────────────────────────────

    private void RunSearch(string query, bool live)
    {
        if (_engine is null) return;
        query = query.Trim();
        if (query.Length == 0)
        {
            SetHitVerses(null);
            _panel.Close();
            return;
        }
        if (_engine.SearchJson(query) is not { } json) return;
        var r = Wire.Parse<SearchResult>(json);
        if (!live && r.Kind == "goto" && r.Book is not null && r.Chapter is not null)
        {
            SetHitVerses(null);
            var refKey = r.Verse is { } v ? $"{r.Book} {r.Chapter}:{v}" : null;
            NavigateActive(r.Book, r.Chapter.Value, refKey);
            _panel.Close();
            FocusActive();
            return;
        }
        // Band every hit that falls in a visible chapter (Tier 0 #8), then let
        // the panel build its own ranked result blocks from the core.
        SetHitVerses(r.Kind == "hits" ? r.Hits?.Select(h => h.Verse) : null);
        _panel.ShowSearchBlocks(query);
    }

    /// Set (or clear, with null) the search-hit set banded across every pane.
    private void SetHitVerses(IEnumerable<string>? verses)
    {
        var set = verses is null ? new HashSet<string>() : new HashSet<string>(verses);
        foreach (var p in _panes) { p.Reader.HitVerses = set; p.Reader.Redraw(); }
    }

    // ── themes, history target, context menu, shortcuts (Tier 0) ────────────

    private void NavigateOtherPane(string book, uint chapter, string? verse)
    {
        if (_panes.Count < 2) { NavigateActive(book, chapter, verse); return; }
        int other = (_active + 1) % _panes.Count;
        _panes[other].Reader.ShowChapter(book, chapter, verse, highlight: verse is not null);
    }

    private static bool SystemIsDark()
    {
        try
        {
            var bg = new Windows.UI.ViewManagement.UISettings()
                .GetColorValue(Windows.UI.ViewManagement.UIColorType.Background);
            return (bg.R * 299 + bg.G * 587 + bg.B * 114) / 1000 < 128;
        }
        catch { return false; }
    }

    private static string ResolveTheme(string choice) => choice switch
    {
        "light" => "light",
        "dark" => "dark",
        "night" => "night",
        _ => SystemIsDark() ? "dark" : "light",
    };

    private static string NextChoice(string c) => c switch
    {
        "light" => "dark",
        "dark" => "night",
        "night" => "system",
        _ => "light",
    };

    private static string ThemeLabel(string c) => "Theme: " + (c is "light" or "dark" or "night" ? c : "system");

    private void CycleTheme()
    {
        _themeChoice = NextChoice(_themeChoice);
        Palette.ApplyTheme(ResolveTheme(_themeChoice));
        _themeBtn.Content = ThemeLabel(_themeChoice);
        ApplyThemeToUi();
        PersistConfig();
    }

    /// Re-apply the current palette to chrome whose brushes were captured earlier
    /// (a theme switch, or the initial fix-up after the theme is resolved).
    private void ApplyThemeToUi()
    {
        if (Content is Grid g)
            g.RequestedTheme = Palette.Dark ? ElementTheme.Dark : ElementTheme.Light;
        _status.Foreground = new SolidColorBrush(Palette.InkFaded);
        _panel.ApplyTheme();
        _strip.Invalidate();
        _connectors.Redraw();
        foreach (var p in _panes) { p.Reader.ApplyTheme(); p.ApplyTheme(); }
    }

    private static string Now() => DateTime.UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'");

    /// Copy `text` to the clipboard (Tier 0 #1).
    private static void CopyToClipboard(string text)
    {
        var dp = new Windows.ApplicationModel.DataTransfer.DataPackage();
        dp.SetText(text);
        Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(dp);
    }

    /// The right-click verse context menu: copy shapes, tag/thread/note authoring
    /// (routed through the panel dispatcher), and the highlight swatches.
    private void ShowContextMenu(PaneView pane, string verse, Windows.Foundation.Point pt)
    {
        if (_engine is null) return;
        SetActive(_panes.IndexOf(pane));
        var flyout = new MenuFlyout();

        void Item(string label, Action act)
        {
            var mi = new MenuFlyoutItem { Text = label };
            mi.Click += (_, _) => act();
            flyout.Items.Add(mi);
        }
        void Copy(string label, string kind) =>
            Item(label, () => { if (_engine.CopyText(verse, kind) is { } t) CopyToClipboard(t); });

        Copy("Copy verse", "verse");
        Copy("Copy with reference", "verseRef");
        Copy("Copy (markdown)", "verseMarkdown");
        Copy("Copy chapter", "chapter");
        flyout.Items.Add(new MenuFlyoutSeparator());

        // Highlight submenu: the fixed tones + a remove.
        var hi = new MenuFlyoutSubItem { Text = "Highlight" };
        try
        {
            foreach (var tone in Wire.Parse<HighlightTones>(StudyEngine.HighlightTonesJson()).Tones)
            {
                var mi = new MenuFlyoutItem { Text = char.ToUpper(tone.Name[0]) + tone.Name[1..] };
                mi.Click += (_, _) => HighlightVerse(verse, tone.Name, tone.Hex);
                hi.Items.Add(mi);
            }
        }
        catch { /* tones unavailable → just the remove item */ }
        hi.Items.Add(new MenuFlyoutSeparator());
        var rm = new MenuFlyoutItem { Text = "Remove highlight" };
        rm.Click += (_, _) => RemoveHighlight(verse);
        hi.Items.Add(rm);
        flyout.Items.Add(hi);

        Item("Note…", () => _panel.Link($"editnote:{verse}"));
        if (_fullMode)
        {
            flyout.Items.Add(new MenuFlyoutSeparator());
            Item("Tag…", () => _panel.Link($"addtag:{verse}"));
            Item("Add to thread…", () => _panel.Link($"addthread:{verse}"));
        }

        flyout.ShowAt(pane.Reader, new FlyoutShowOptions { Position = pt });
    }

    /// Highlight a verse: add it to the tag for `tone` (creating it, coloured).
    private void HighlightVerse(string verse, string tone, string hex)
    {
        if (_engine is null) return;
        var tag = char.ToUpper(tone[0]) + tone[1..]; // e.g. "Amber"
        var err = _engine.TagAdd(tag, "verse", verse, null, Now());
        if (err is null) err = _engine.TagSetColor(tag, hex);
        if (err is not null) { _status.Text = err; return; }
        RefreshStudyData();
    }

    /// Remove a verse from every colour-bearing tag that holds it.
    private void RemoveHighlight(string verse)
    {
        if (_engine?.TagsJson() is not { } tj) return;
        foreach (var t in Wire.Parse<Tags>(tj).Items)
            if (t.Color is not null && t.Members.Any(m => m.Kind == "verse" && m.Verse == verse))
                _engine.TagRemove(t.Name, "verse", verse);
        RefreshStudyData();
    }

    private async Task ShowShortcutsAsync()
    {
        var rows = new (string, string)[]
        {
            ("↑ / ↓ / Space", "scroll"),
            ("PageUp / PageDown", "scroll a page"),
            ("Home / End", "chapter start / end"),
            ("← / →  (or [ / ])", "step chapters (across books)"),
            ("Alt + ← / →", "back / forward in history"),
            ("Shift + scroll", "lock all panes together"),
            ("Ctrl + scroll, Ctrl +/−", "zoom · Ctrl 0 resets"),
            ("Ctrl + click / double-click", "word study"),
            ("Right-click a verse", "copy · tag · note · highlight"),
            ("Ctrl + F", "search"),
            ("Esc", "close the panel / a popup"),
            ("F1 / ?", "this list"),
        };
        var panel = new StackPanel { Spacing = 4 };
        foreach (var (k, v) in rows)
        {
            var row = new Grid();
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(220) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            var key = new TextBlock { Text = k, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
            var act = new TextBlock { Text = v, TextWrapping = TextWrapping.Wrap };
            Grid.SetColumn(key, 0); Grid.SetColumn(act, 1);
            row.Children.Add(key); row.Children.Add(act);
            panel.Children.Add(row);
        }
        var dialog = new ContentDialog
        {
            Title = "Keyboard shortcuts",
            Content = new ScrollViewer { Content = panel },
            CloseButtonText = "Close",
            XamlRoot = Content.XamlRoot,
        };
        await dialog.ShowAsync();
    }
}
