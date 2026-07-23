// One reading column: a Win2D canvas that lets the Rust core do the layout.
// The core's line-breaker measures text through our DirectWrite-backed
// callback and hands back a display list + per-word hit regions; this control
// paints items and forwards pointer coordinates back for hit-testing — the
// same thin-shell contract the GTK app follows with Pango. Constants and
// behaviors mirror apps/desktop (see docs/FEATURE-MANIFEST.md).

using System.Numerics;
using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using PureStudy;
using Windows.System;
using Windows.UI;

namespace PureStudyWin;

/// The reader's colour palette. Theme-aware (Tier 0 #5): the single source is
/// core::theme, fetched as JSON (`StudyEngine.PaletteJson`) and applied here, so
/// light/dark/night can't drift between shells. Defaults are the shipped light
/// values, so the reader looks right even before `Apply` runs. Alpha variants
/// (GoldFaint / Band / PinBand / GutterDot) derive from the base tones, so they
/// follow the theme for free. Most consumers read these at paint time, so a
/// theme switch takes effect on the next `Invalidate()` / re-render.
public static class Palette
{
    public static Color Paper { get; private set; } = Color.FromArgb(255, 252, 249, 244);      // #fcf9f4
    public static Color Ink { get; private set; } = Color.FromArgb(255, 33, 31, 26);
    public static Color InkFaded { get; private set; } = Color.FromArgb(255, 107, 104, 98);    // added-word gray
    public static Color Faded { get; private set; } = Color.FromArgb(255, 138, 130, 118);      // panel muted #8a8276
    public static Color Divine { get; private set; } = Color.FromArgb(255, 77, 51, 38);
    public static Color TitleInk { get; private set; } = Color.FromArgb(255, 102, 92, 77);
    public static Color Gold { get; private set; } = Color.FromArgb(255, 158, 125, 56);        // #9e7d38
    public static Color GoldFaint { get; private set; } = Color.FromArgb(77, 158, 125, 56);    // α0.30
    public static Color Band { get; private set; } = Color.FromArgb(31, 158, 125, 56);         // α0.12
    public static Color GutterDot { get; private set; } = Color.FromArgb(191, 158, 125, 56);   // α0.75
    public static Color PinBand { get; private set; } = Color.FromArgb(56, 64, 115, 191);      // blue α0.22
    public static Color PanelBg { get; private set; } = Color.FromArgb(255, 242, 238, 230);    // panel / gloss paper
    public static Color PaneNavBg { get; private set; } = Color.FromArgb(255, 239, 234, 225);  // #efeae1
    public static Color StripBg { get; private set; } = Color.FromArgb(255, 235, 230, 219);    // canon strip
    public static Color Rule { get; private set; } = Color.FromArgb(255, 216, 203, 168);
    public static Color SectionGold { get; private set; } = Color.FromArgb(255, 160, 137, 74); // #a0894a
    public static Color Disputed { get; private set; } = Color.FromArgb(255, 176, 74, 58);     // #b04a3a
    public static Color Mono { get; private set; } = Color.FromArgb(255, 136, 136, 136);
    public static Color Morph { get; private set; } = Color.FromArgb(255, 106, 90, 42);
    public static Color Lemma { get; private set; } = Color.FromArgb(255, 138, 122, 82);
    // Authority-tier provenance mark colors (see StudyPanel tier marks).
    public static Color TierGod { get; private set; } = Color.FromArgb(255, 158, 125, 56);     // ✝ the text itself
    public static Color TierHuman { get; private set; } = Color.FromArgb(255, 111, 143, 106);  // † curated
    public static Color TierMachine { get; private set; } = Color.FromArgb(255, 153, 153, 153);// ≈ machine
    public static Color TierResearch { get; private set; } = Color.FromArgb(255, 176, 74, 58); // ⚗ research-grade

    /// Whether the current theme is dark-ish (drives ElementTheme for chrome).
    public static bool Dark { get; private set; }

    /// Fired after a theme change so shells can rebuild captured brushes.
    public static event Action? Changed;

    /// Apply a palette fetched from the core (`StudyEngine.PaletteJson`).
    public static void Apply(PaletteData p)
    {
        Paper = Hex(p.Paper); Ink = Hex(p.Ink); InkFaded = Hex(p.Added); Faded = Hex(p.Faded);
        Divine = Hex(p.Divine); TitleInk = Hex(p.TitleInk); Gold = Hex(p.Gold);
        PanelBg = Hex(p.PopupPaper); PaneNavBg = Hex(p.PaneNavBg); StripBg = Hex(p.StripBg);
        Rule = Hex(p.Rule); SectionGold = Hex(p.Section); Disputed = Hex(p.TierResearch);
        Mono = Hex(p.Mono); Morph = Hex(p.Morph); Lemma = Hex(p.Lemma);
        TierGod = Hex(p.TierGod); TierHuman = Hex(p.TierHuman); TierMachine = Hex(p.TierMachine);
        TierResearch = Hex(p.TierResearch);
        GoldFaint = WithAlpha(Gold, 77);
        Band = WithAlpha(Gold, 31);
        GutterDot = WithAlpha(Gold, 191);
        PinBand = WithAlpha(Hex(p.Pin), 56);
        Dark = p.Dark;
        Changed?.Invoke();
    }

    /// Apply the palette for a theme token (`light`/`dark`/`night`). Convenience
    /// over parsing `PaletteJson` at every call site.
    public static void ApplyTheme(string themeToken) =>
        Apply(Wire.Parse<PaletteData>(StudyEngine.PaletteJson(themeToken)));

    /// A verse-highlight wash: the tag tone at a soft alpha behind the text.
    public static Color Wash(Color tone) => WithAlpha(tone, (byte)(Dark ? 64 : 92));

    private static Color WithAlpha(Color c, byte a) => Color.FromArgb(a, c.R, c.G, c.B);

    /// Parse `#rrggbb` (opaque). Falls back to ink on a malformed value.
    public static Color Hex(string h)
    {
        try
        {
            h = h.TrimStart('#');
            return Color.FromArgb(255,
                Convert.ToByte(h.Substring(0, 2), 16),
                Convert.ToByte(h.Substring(2, 2), 16),
                Convert.ToByte(h.Substring(4, 2), 16));
        }
        catch { return Color.FromArgb(255, 33, 31, 26); }
    }
}

/// A pinned word span in a pane: first click sets the anchor; another click in
/// the same verse re-spans around the anchor; a different verse resets.
public sealed record PinSpan(string Verse, uint Anchor, uint Lo, uint Hi);

public sealed class ReaderView : UserControl, IDisposable
{
    public const float Margin = 28f;        // GTK MARGIN — all sides
    public const float MaxColumn = 720f;    // GTK MAX_COLUMN

    private StudyEngine? _engine;
    private Chapter? _chapter;
    private DisplayList? _dl;
    private string _book = "John";
    private uint _ch = 3;
    private float _fontSize = 18f;
    private float _scrollY;
    private string? _highlightVerse;
    private string? _pendingScrollVerse;
    private bool _ready;
    private string? _problem;
    private float _originX = Margin;
    private float _column = 400f;

    /// Verses (refKeys) that have weave partners — painted as gutter dots.
    public HashSet<string> XrefVerses = new();
    /// Verses with a personal note — a second gutter mark (Tier 0 #3).
    public HashSet<string> NoteVerses = new();
    /// Every current search hit — banded in whatever chapter shows them (#8).
    public HashSet<string> HitVerses = new();
    /// refKey → highlight tone (member of a colour-bearing tag; Tier 0 #4).
    public Dictionary<string, Color> Highlights = new();

    // Per-pane reading history (Tier 0 #2): the chapters visited, with a cursor.
    private readonly List<(string book, uint ch)> _history = new();
    private int _histIdx = -1;
    private bool _inHistoryNav;

    public PinSpan? Pin { get; private set; }

    private readonly CanvasControl _canvas = new();
    private readonly ScrollBar _bar = new()
    {
        Orientation = Orientation.Vertical,
        IndicatorMode = ScrollingIndicatorMode.MouseIndicator,
        HorizontalAlignment = HorizontalAlignment.Right,
    };
    private readonly Border _gloss = new()
    {
        Visibility = Visibility.Collapsed,
        HorizontalAlignment = HorizontalAlignment.Left,
        VerticalAlignment = VerticalAlignment.Top,
        Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.PanelBg),
        BorderBrush = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.Rule),
        BorderThickness = new Thickness(1),
        CornerRadius = new CornerRadius(4),
        Padding = new Thickness(8, 5, 8, 5),
        MaxWidth = 380,
        IsHitTestVisible = false,
    };
    private readonly TextBlock _glossText = new()
    {
        FontSize = 12.5,
        TextWrapping = TextWrapping.Wrap,
        Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.Ink),
    };
    private readonly DispatcherTimer _hover = new() { Interval = TimeSpan.FromMilliseconds(350) };
    private Windows.Foundation.Point _hoverPos;

    private string _family = "Georgia";
    private string _familyItalic = "Georgia";
    private CanvasTextFormat? _fmt;
    private CanvasTextFormat? _fmtItalic;
    private CanvasTextFormat? _fmtBold;
    private float _fmtSize = -1f;
    private float _textH = 20f;
    private float _lineH = 27f;
    private readonly Dictionary<string, float> _measure = new();

    /// Double-click / Ctrl+click a word.
    public event Action<Hit>? WordActivated;
    /// Any pointer press (GTK: touching a pane makes it active).
    public event Action? Activated;
    public event Action<string, uint>? ChapterShown;
    /// Pin state changed (single-click word).
    public event Action? PinChanged;
    /// Scroll position changed (connector overlay redraws).
    public event Action? Scrolled;
    /// Ctrl+wheel / Ctrl+± zoom request, in ±1-pt steps (0 = reset).
    public event Action<int>? ZoomRequested;
    /// Shift+scroll: move all panes in lockstep by this many pixels.
    public event Action<float>? ScrollAllRequested;
    /// Right-click on a verse: (refKey, canvas point) → the context menu.
    public event Action<string, Windows.Foundation.Point>? ContextRequested;

    public string Book => _book;
    public uint ChapterNumber => _ch;

    public ReaderView()
    {
        IsTabStop = true;
        var grid = new Grid();
        grid.Children.Add(_canvas);
        grid.Children.Add(_bar);
        _gloss.Child = _glossText;
        grid.Children.Add(_gloss);
        Content = grid;

        _canvas.ClearColor = Palette.Paper;
        _canvas.CreateResources += (_, _) => { _ready = true; InitFonts(); Relayout(); };
        _canvas.Draw += OnDraw;
        _canvas.SizeChanged += (_, _) => Relayout();
        _canvas.PointerWheelChanged += OnWheel;
        _canvas.DoubleTapped += OnDoubleTapped;
        _canvas.PointerPressed += OnPressed;
        _canvas.PointerMoved += (_, e) =>
        {
            var pos = e.GetCurrentPoint(_canvas).Position;
            // Resting on a word must not flicker the gloss: ignore sub-pixel
            // jitter while it is showing; only real movement re-arms it.
            if (_gloss.Visibility == Visibility.Visible)
            {
                double dx = pos.X - _hoverPos.X, dy = pos.Y - _hoverPos.Y;
                if (dx * dx + dy * dy < 64) return;
            }
            _hoverPos = pos;
            HideGloss();
            _hover.Stop();
            _hover.Start();
        };
        _canvas.PointerExited += (_, _) => { _hover.Stop(); HideGloss(); };
        _hover.Tick += (_, _) => { _hover.Stop(); ShowGloss(); };
        _bar.Scroll += (_, e) =>
        {
            _scrollY = (float)e.NewValue;
            HideGloss();
            _canvas.Invalidate();
            Scrolled?.Invoke();
        };
        KeyDown += OnKey;
    }

    public void SetEngine(StudyEngine engine)
    {
        _engine = engine;
        Relayout();
    }

    /// App-wide body size (GTK: one zoom for all panes, 12–48 pt).
    public float FontSize
    {
        get => _fontSize;
        set
        {
            var clamped = Math.Clamp(value, 12f, 48f);
            if (Math.Abs(clamped - _fontSize) < 0.01f) return;
            float frac = DocHeight > 1 ? _scrollY / DocHeight : 0;
            _fontSize = clamped;
            Relayout();
            _scrollY = frac * DocHeight;
            UpdateScrollExtent();
            _canvas.Invalidate();
            Scrolled?.Invoke();
        }
    }

    private bool _versePerLine;
    /// Verse-per-line reading (GTK vpl toggle): each verse starts a fresh line.
    /// Feeds PureLayoutConfig.verse_break, which the shared core layout honors.
    public bool VersePerLine
    {
        get => _versePerLine;
        set
        {
            if (value == _versePerLine) return;
            _versePerLine = value;
            Relayout();
            UpdateScrollExtent();
            _canvas.Invalidate();
            Scrolled?.Invoke();
        }
    }

    /// Navigate. `verse` (refKey) scrolls there; `highlight` paints the band
    /// (persists until this pane next navigates).
    public void ShowChapter(string book, uint chapter, string? verse = null, bool highlight = false)
    {
        if (_engine is null) return;
        var count = _engine.ChapterCount(book);
        if (count == 0) return;
        _book = book;
        _ch = Math.Clamp(chapter, 1u, count);
        // Record the destination in the reading history (unless this navigation
        // *is* a history move). Forward entries past the cursor are discarded.
        if (!_inHistoryNav)
        {
            if (_histIdx < _history.Count - 1)
                _history.RemoveRange(_histIdx + 1, _history.Count - _histIdx - 1);
            if (_history.Count == 0 || _history[^1] != (_book, _ch))
            {
                _history.Add((_book, _ch));
                _histIdx = _history.Count - 1;
            }
        }
        _scrollY = 0;
        _pendingScrollVerse = verse;
        _highlightVerse = highlight ? verse : null;
        Relayout();
        ChapterShown?.Invoke(_book, _ch);
        Scrolled?.Invoke();
    }

    public void ClearPin()
    {
        if (Pin is null) return;
        Pin = null;
        _canvas.Invalidate();
        PinChanged?.Invoke();
    }

    // ── reading history (Tier 0 #2) ──────────────────────────────────────────

    public bool CanGoBack => _histIdx > 0;
    public bool CanGoForward => _histIdx >= 0 && _histIdx < _history.Count - 1;

    public void GoBack()
    {
        if (!CanGoBack) return;
        _histIdx--;
        NavigateHistory();
    }

    public void GoForward()
    {
        if (!CanGoForward) return;
        _histIdx++;
        NavigateHistory();
    }

    private void NavigateHistory()
    {
        var (b, c) = _history[_histIdx];
        _inHistoryNav = true;
        ShowChapter(b, c);
        _inHistoryNav = false;
    }

    /// Re-read theme-dependent resources after a palette change, then repaint.
    public void ApplyTheme()
    {
        _canvas.ClearColor = Palette.Paper;
        _gloss.Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.PanelBg);
        _gloss.BorderBrush = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.Rule);
        _glossText.Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.Ink);
        _canvas.Invalidate();
    }

    /// The verse under a canvas point: the hit word's verse, else the nearest
    /// verse-number by y. Null when no chapter is laid out. Drives the context
    /// menu (a right-click anywhere in a verse's lines targets that verse).
    public string? VerseAt(Windows.Foundation.Point p)
    {
        if (HitAt(p) is { } hit) return hit.Verse;
        if (_dl is null) return null;
        float y = (float)p.Y - Margin + _scrollY;
        DisplayItem? best = null;
        float bestD = float.MaxValue;
        foreach (var it in _dl.Items.Where(i => i.Kind == "verseNumber"))
        {
            float d = Math.Abs(it.Y + it.H * 0.5f - y);
            if (d < bestD) { bestD = d; best = it; }
        }
        return best is null ? null : RefOf(best);
    }

    // ── fonts + measurement ────────────────────────────────────────────────

    private void InitFonts()
    {
        try
        {
            _ = new Microsoft.Graphics.Canvas.Text.CanvasFontSet(
                new Uri("ms-appx:///Assets/Fonts/EBGaramond.ttf"));
            _family = "ms-appx:///Assets/Fonts/EBGaramond.ttf#EB Garamond";
            _familyItalic = "ms-appx:///Assets/Fonts/EBGaramond-Italic.ttf#EB Garamond";
        }
        catch
        {
            _family = "Georgia";
            _familyItalic = "Georgia";
        }
    }

    private void EnsureFormats()
    {
        if (_fmt is not null && Math.Abs(_fmtSize - _fontSize) < 0.01f) return;
        _fmt?.Dispose();
        _fmtItalic?.Dispose();
        _fmtBold?.Dispose();
        _fmt = new CanvasTextFormat
        {
            FontFamily = _family, FontSize = _fontSize,
            WordWrapping = CanvasWordWrapping.NoWrap,
        };
        _fmtItalic = new CanvasTextFormat
        {
            FontFamily = _familyItalic, FontSize = _fontSize,
            FontStyle = Windows.UI.Text.FontStyle.Italic,
            WordWrapping = CanvasWordWrapping.NoWrap,
        };
        _fmtBold = new CanvasTextFormat
        {
            FontFamily = _family, FontSize = _fontSize,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            WordWrapping = CanvasWordWrapping.NoWrap,
        };
        _fmtSize = _fontSize;
        _measure.Clear();
        using var probe = new CanvasTextLayout(_canvas, "Hgy", _fmt, 1e6f, 1e6f);
        _textH = (float)probe.LayoutBounds.Height;
        _lineH = _textH * 1.35f;   // GTK: (ascent+descent)*1.35
    }

    private float MeasureText(string text)
    {
        if (_measure.TryGetValue(text, out var w)) return w;
        using var tl = new CanvasTextLayout(_canvas, text, _fmt, 1e6f, 1e6f);
        w = (float)tl.LayoutBounds.Width;
        _measure[text] = w;
        return w;
    }

    // ── layout ─────────────────────────────────────────────────────────────

    private void Relayout()
    {
        if (!_ready || _engine is null) return;
        float avail = (float)_canvas.ActualWidth - 2 * Margin;
        if (avail < 60) return;
        EnsureFormats();
        _column = Math.Min(avail, MaxColumn);
        _originX = ((float)_canvas.ActualWidth - _column) / 2;

        float space = Math.Max(1f, MeasureText("n n") - MeasureText("nn"));
        var cfg = new PureStudy.Native.PureLayoutConfig
        {
            width = _column,
            line_height = _lineH,
            space_width = space,
            verse_num_gap = space * 1.4f,
            para_indent = _lineH * 0.9f,
            para_spacing = _lineH * 0.45f,
            verse_break = (uint)(_versePerLine ? 1 : 0),
        };

        try
        {
            _chapter?.Dispose();
            _chapter = _engine.LayoutChapter(_book, _ch, cfg, MeasureText);
            _dl = Wire.Parse<DisplayList>(_chapter.Json());
            _problem = null;
        }
        catch (PureStudyException e)
        {
            _chapter = null;
            _dl = null;
            _problem = e.Message;
        }

        UpdateScrollExtent();
        if (_pendingScrollVerse is not null)
        {
            ScrollToVerse(_pendingScrollVerse);
            _pendingScrollVerse = null;
        }
        _canvas.Invalidate();
    }

    private float DocHeight => (_dl?.Height ?? 0) + 2 * Margin;

    private void UpdateScrollExtent()
    {
        float viewH = (float)_canvas.ActualHeight;
        float max = Math.Max(0, DocHeight - viewH);
        _scrollY = Math.Clamp(_scrollY, 0, max);
        _bar.Maximum = max;
        _bar.ViewportSize = viewH;
        _bar.LargeChange = viewH * 0.85;
        _bar.SmallChange = _fontSize * 3;
        _bar.Value = _scrollY;
        _bar.Visibility = max > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    public void ScrollBy(float px)
    {
        float viewH = (float)_canvas.ActualHeight;
        _scrollY = Math.Clamp(_scrollY + px, 0, Math.Max(0, DocHeight - viewH));
        _bar.Value = _scrollY;
        _canvas.Invalidate();
        Scrolled?.Invoke();
    }

    public void ScrollPage(int dir) =>
        ScrollBy(dir * Math.Max((float)_canvas.ActualHeight * 0.85f, _fontSize * 3));

    public void ScrollHome() => ScrollBy(float.NegativeInfinity);
    public void ScrollEnd() => ScrollBy(float.PositiveInfinity);

    private void ScrollToVerse(string refKey)
    {
        var item = _dl?.Items.FirstOrDefault(i =>
            i.Kind == "verseNumber" ? RefOf(i) == refKey : i.Verse == refKey);
        if (item is null) return;
        float viewH = (float)_canvas.ActualHeight;
        _scrollY = Math.Clamp(Margin + item.Y - 8, 0, Math.Max(0, DocHeight - viewH));
        _bar.Value = _scrollY;
    }

    /// A verse-number item's refKey ("Book c:v" from the chapter + number).
    private string RefOf(DisplayItem it) =>
        it.Verse ?? (it.VerseNumber is { } n ? $"{_book} {_ch}:{n}" : "");

    // ── connector-overlay queries ──────────────────────────────────────────

    public bool ShowsChapter(string book, uint chapter) => _book == book && _ch == chapter;

    /// Viewport-space y of a verse's number line (centre), unclamped; null if
    /// the verse isn't in this chapter's layout.
    public float? VerseY(string refKey)
    {
        var it = _dl?.Items.FirstOrDefault(i => i.Kind == "verseNumber" && RefOf(i) == refKey);
        if (it is null) return null;
        return Margin + it.Y + it.H * 0.5f - _scrollY;
    }

    /// The vertical band of this pane actually showing text, in viewport coords.
    public (float top, float bottom) VisibleBand() => (0f, (float)_canvas.ActualHeight);

    // ── painting ───────────────────────────────────────────────────────────

    private void OnDraw(CanvasControl sender, CanvasDrawEventArgs args)
    {
        var ds = args.DrawingSession;
        ds.Clear(Palette.Paper);

        if (_dl is null)
        {
            ds.DrawText(_problem ?? "loading…", new Vector2(Margin, Margin),
                Palette.InkFaded, _fmt ?? new CanvasTextFormat { FontSize = 14 });
            return;
        }

        float viewH = (float)sender.ActualHeight;
        float top = _scrollY - Margin;
        ds.Transform = Matrix3x2.CreateTranslation(_originX, Margin - _scrollY);

        // Highlight washes (persistent user tag colours) — underneath everything.
        if (Highlights.Count > 0)
            foreach (var grp in _dl.Items.GroupBy(RefOf).Where(g => Highlights.ContainsKey(g.Key)))
            {
                var wash = Palette.Wash(Highlights[grp.Key]);
                foreach (var line in grp.GroupBy(i => i.Y))
                    ds.FillRectangle(-6, line.Key, _column + 12, line.First().H, wash);
            }

        // Every current search hit in this chapter (Tier 0 #8), as a soft band.
        if (HitVerses.Count > 0)
            foreach (var grp in _dl.Items.GroupBy(RefOf).Where(g => HitVerses.Contains(g.Key)))
                foreach (var line in grp.GroupBy(i => i.Y))
                    ds.FillRectangle(-6, line.Key, _column + 12, line.First().H, Palette.Band);

        // Search/goto highlight: the primary jump target (banded, one per line).
        if (_highlightVerse is not null)
            foreach (var line in _dl.Items.Where(i => RefOf(i) == _highlightVerse).GroupBy(i => i.Y))
                ds.FillRectangle(-6, line.Key, _column + 12, line.First().H, Palette.Band);

        // Pinned span: blue band per word rect.
        if (Pin is { } pin)
            foreach (var it in _dl.Items.Where(i => i.Verse == pin.Verse &&
                         i.TokenIndex is { } t && t >= pin.Lo && t <= pin.Hi))
                ds.FillRectangle(it.X - 1.5f, it.Y, it.W + 3, it.H, Palette.PinBand);

        foreach (var it in _dl.Items)
        {
            if (it.Y + it.H < top || it.Y > top + viewH) continue;
            float dy = it.Y + (it.H - _textH) * 0.5f;

            if (it.Kind == "verseNumber")
            {
                ds.DrawText(it.Text, new Vector2(it.X, dy), Palette.Gold, _fmtBold);
                var rk = RefOf(it);
                // Gutter dot: this verse has weave partners.
                if (XrefVerses.Contains(rk))
                    ds.FillCircle(-9f, it.Y + it.H * 0.65f, 2.3f, Palette.GutterDot);
                // A second, square gutter mark: this verse has a personal note.
                if (NoteVerses.Contains(rk))
                    ds.FillRectangle(-13f, it.Y + it.H * 0.30f, 3.2f, 3.2f, Palette.InkFaded);
                continue;
            }
            bool added = (it.Flags & PureFlags.Added) != 0;
            bool divine = (it.Flags & PureFlags.Divine) != 0;
            bool title = (it.Flags & PureFlags.Title) != 0;
            var color = added ? Palette.InkFaded
                : divine ? Palette.Divine
                : title ? Palette.TitleInk
                : Palette.Ink;
            ds.DrawText(it.Text, new Vector2(it.X, dy), color, added ? _fmtItalic : _fmt);
            if (it.Strongs.Count > 0)
                ds.FillRectangle(it.X, it.Y + it.H - 3f, it.W, 1f, Palette.GoldFaint);
        }

        ds.Transform = Matrix3x2.Identity;
    }

    // ── input ──────────────────────────────────────────────────────────────

    private Hit? HitAt(Windows.Foundation.Point p)
    {
        if (_chapter is null) return null;
        var json = _chapter.HitTestJson(
            (float)p.X - _originX, (float)p.Y - Margin + _scrollY);
        return json is null ? null : Wire.Parse<Hit>(json);
    }

    private void OnPressed(object sender, PointerRoutedEventArgs e)
    {
        HideGloss();
        Focus(FocusState.Programmatic);
        Activated?.Invoke();

        var pt = e.GetCurrentPoint(_canvas);
        // Mouse back/forward buttons walk the reading history (Tier 0 #2).
        var upd = pt.Properties.PointerUpdateKind;
        if (upd == Microsoft.UI.Input.PointerUpdateKind.XButton1Pressed) { GoBack(); e.Handled = true; return; }
        if (upd == Microsoft.UI.Input.PointerUpdateKind.XButton2Pressed) { GoForward(); e.Handled = true; return; }
        // Right-click → the verse context menu (Tier 0 #1).
        if (upd == Microsoft.UI.Input.PointerUpdateKind.RightButtonPressed)
        {
            if (VerseAt(pt.Position) is { } v) ContextRequested?.Invoke(v, pt.Position);
            e.Handled = true;
            return;
        }
        if (!pt.Properties.IsLeftButtonPressed) return;
        bool ctrl = e.KeyModifiers.HasFlag(VirtualKeyModifiers.Control);
        var hit = HitAt(pt.Position);
        if (hit is null) return;

        if (ctrl)
        {
            WordActivated?.Invoke(hit);
            return;
        }
        // Single click: pin/widen a span (GTK weave-authoring flow).
        Pin = Pin is { } p && p.Verse == hit.Verse
            ? p with { Lo = Math.Min(p.Anchor, hit.TokenIndex), Hi = Math.Max(p.Anchor, hit.TokenIndex) }
            : new PinSpan(hit.Verse, hit.TokenIndex, hit.TokenIndex, hit.TokenIndex);
        _canvas.Invalidate();
        PinChanged?.Invoke();
    }

    private void OnDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        var hit = HitAt(e.GetPosition(_canvas));
        if (hit is not null) WordActivated?.Invoke(hit);
    }

    private void OnWheel(object sender, PointerRoutedEventArgs e)
    {
        int delta = e.GetCurrentPoint(_canvas).Properties.MouseWheelDelta;
        if (e.KeyModifiers.HasFlag(VirtualKeyModifiers.Control))
            ZoomRequested?.Invoke(delta > 0 ? 1 : -1);
        else if (e.KeyModifiers.HasFlag(VirtualKeyModifiers.Shift))
            ScrollAllRequested?.Invoke(-delta / 120f * _fontSize * 3f);
        else
            ScrollBy(-delta / 120f * _fontSize * 3f);
        e.Handled = true;
    }

    private void OnKey(object sender, KeyRoutedEventArgs e)
    {
        bool shift = Microsoft.UI.Input.InputKeyboardSource
            .GetKeyStateForCurrentThread(VirtualKey.Shift)
            .HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
        float line = _fontSize * 3;
        void Move(float px)
        {
            if (shift) ScrollAllRequested?.Invoke(px);
            else ScrollBy(px);
        }
        switch (e.Key)
        {
            case VirtualKey.Down: Move(line); break;
            case VirtualKey.Up: Move(-line); break;
            case VirtualKey.PageDown:
            case VirtualKey.Space:
                Move(Math.Max((float)_canvas.ActualHeight * 0.85f, line)); break;
            case VirtualKey.PageUp:
                Move(-Math.Max((float)_canvas.ActualHeight * 0.85f, line)); break;
            case VirtualKey.Home: ScrollHome(); break;
            case VirtualKey.End: ScrollEnd(); break;
            default: return;
        }
        e.Handled = true;
    }

    // ── hover gloss ────────────────────────────────────────────────────────

    private void ShowGloss()
    {
        if (_engine is null || HitAt(_hoverPos) is not { } hit || hit.Strongs.Count == 0) return;
        var parts = new List<string>();
        foreach (var code in hit.Strongs)
        {
            if (_engine.StrongsJson(code) is not { } sj) continue;
            var e = Wire.Parse<StrongsEntry>(sj);
            var body = e.Kjv ?? e.Def ?? "";
            if (body.Length > 80) body = body[..80].TrimEnd() + "…";
            parts.Add($"{e.Code}  {e.Lemma}{(e.Xlit is not null ? $" ({e.Xlit})" : "")}{(body.Length > 0 ? " — " + body : "")}");
        }
        if (parts.Count == 0) return;
        _glossText.Text = string.Join("\n", parts);
        _gloss.Visibility = Visibility.Visible;
        // Keep the card fully inside the pane — flip left/up near the edges
        // instead of letting the layout squeeze it to a sliver.
        _gloss.Measure(new Windows.Foundation.Size(380, double.PositiveInfinity));
        var sz = _gloss.DesiredSize;
        double left = Math.Max(0, Math.Min(_hoverPos.X + 14, ActualWidth - sz.Width - 10));
        double top = Math.Max(0, Math.Min(_hoverPos.Y + 20, ActualHeight - sz.Height - 8));
        _gloss.Margin = new Thickness(left, top, 0, 0);
    }

    private void HideGloss() => _gloss.Visibility = Visibility.Collapsed;

    public void Redraw() => _canvas.Invalidate();

    public void Dispose()
    {
        _chapter?.Dispose();
        _chapter = null;
        _fmt?.Dispose();
        _fmtItalic?.Dispose();
        _fmtBold?.Dispose();
        _canvas.RemoveFromVisualTree();
    }
}
