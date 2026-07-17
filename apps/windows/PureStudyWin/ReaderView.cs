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

public static class Palette
{
    public static readonly Color Paper = Color.FromArgb(255, 252, 249, 244);      // #fcf9f4
    public static readonly Color Ink = Color.FromArgb(255, 33, 31, 26);           // rgb(.13,.12,.10)
    public static readonly Color InkFaded = Color.FromArgb(255, 107, 104, 98);    // #6b6862
    public static readonly Color Divine = Color.FromArgb(255, 77, 51, 38);
    public static readonly Color TitleInk = Color.FromArgb(255, 102, 92, 77);
    public static readonly Color Gold = Color.FromArgb(255, 158, 125, 56);        // #9e7d38
    public static readonly Color GoldFaint = Color.FromArgb(77, 158, 125, 56);    // α0.30
    public static readonly Color Band = Color.FromArgb(31, 158, 125, 56);         // α0.12
    public static readonly Color PinBand = Color.FromArgb(56, 64, 115, 191);      // blue α0.22
    public static readonly Color PanelBg = Color.FromArgb(255, 242, 238, 230);    // popup paper
    public static readonly Color PaneNavBg = Color.FromArgb(255, 239, 234, 225);  // #efeae1
    public static readonly Color Rule = Color.FromArgb(255, 216, 203, 168);
    public static readonly Color SectionGold = Color.FromArgb(255, 160, 137, 74); // #a0894a
    public static readonly Color Disputed = Color.FromArgb(255, 176, 74, 58);     // #b04a3a
    // Authority-tier provenance mark colors (see StudyPanel tier marks).
    public static readonly Color TierGod = Gold;                                  // ✝ the text itself
    public static readonly Color TierHuman = Color.FromArgb(255, 111, 143, 106);  // † curated (#6f8f6a)
    public static readonly Color TierMachine = Color.FromArgb(255, 153, 153, 153);// ≈ machine (#999)
    public static readonly Color TierResearch = Disputed;                         // ⚗ research-grade
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

        // Search/goto highlight: one line-wide band per laid-out line.
        if (_highlightVerse is not null)
            foreach (var y in _dl.Items.Where(i => i.Verse == _highlightVerse ||
                         (i.Kind == "verseNumber" && RefOf(i) == _highlightVerse))
                         .GroupBy(i => i.Y))
                ds.FillRectangle(-6, y.Key, _column + 12, y.First().H, Palette.Band);

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
                // Gutter dot: this verse has weave partners.
                if (XrefVerses.Contains(RefOf(it)))
                    ds.FillCircle(-9f, it.Y + it.H * 0.65f, 2.3f,
                        Color.FromArgb(191, 158, 125, 56));
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
