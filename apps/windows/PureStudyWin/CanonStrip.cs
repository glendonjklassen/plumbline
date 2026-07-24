// The canon overview strip: 66 books in 8 sections under the panes, with the
// OT/NT divide and a pin per pane — click to jump the active pane anywhere
// (GTK M:2938–2989). Section geometry comes from the manifest (frozen in
// core::reference::CANON_SEGMENTS).

using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Windows.UI;

namespace PureStudyWin;

/// The canon overview segmentation — the 8 bands + OT/NT divide — fed once from
/// the core view-model (pure_engine_canon_segments_json), frozen in
/// core::reference. The single app-wide source read by both the strip and the
/// map popups; nothing hardcodes the bands anymore (the old copies had drifted).
public static class Canon
{
    public static CanonSegment[] Segments { get; private set; } = Array.Empty<CanonSegment>();
    public static int OtNtDivide { get; private set; }

    /// Populate once at startup from the engine's canon view-model.
    public static void Set(CanonSegments cs)
    {
        Segments = cs.Segments.ToArray();
        OtNtDivide = cs.OtNtDivide;
    }
}

public sealed class CanonStrip : Microsoft.UI.Xaml.Controls.UserControl
{
    /// Book order → (pin per pane, active flag). Set by the shell.
    public List<(int bookOrder, bool active)> Pins = new();
    private List<TocBook> _books = new();
    // Background painted per-frame from the palette (so a theme switch takes on
    // the next Invalidate); the clear colour is just the initial paint.
    private readonly CanvasControl _canvas = new() { ClearColor = Palette.StripBg };

    /// Click → book id (the shell navigates the active pane to chapter 1).
    public event Action<string>? BookPicked;

    public CanonStrip()
    {
        Height = 30;
        _canvas.Draw += OnDraw;
        _canvas.PointerReleased += OnClick;
        Content = _canvas;
    }

    public void Invalidate() => _canvas.Invalidate();

    public void SetBooks(List<TocBook> books)
    {
        _books = books;
        Invalidate();
    }

    private void OnClick(object sender, PointerRoutedEventArgs e)
    {
        if (_books.Count == 0) return;
        var x = e.GetCurrentPoint(this).Position.X;
        var frac = Math.Clamp(x / Math.Max(1, ActualWidth), 0, 0.999);
        BookPicked?.Invoke(_books[(int)(frac * 66)].Id);
    }

    private void OnDraw(CanvasControl sender, CanvasDrawEventArgs args)
    {
        var ds = args.DrawingSession;
        ds.Clear(Palette.StripBg);
        float w = (float)sender.ActualWidth, h = (float)sender.ActualHeight;
        if (w < 10 || Canon.Segments.Length == 0) return;

        var faded = Palette.Faded;
        // Alternating section wash: subtle dark-on-light / light-on-dark.
        var wash = Palette.Dark ? Color.FromArgb(22, 255, 255, 255) : Color.FromArgb(10, 0, 0, 0);
        using var labelFmt = new CanvasTextFormat { FontSize = 11 };
        for (int s = 0; s < Canon.Segments.Length; s++)
        {
            var seg = Canon.Segments[s];
            float x0 = seg.First / 66f * w;
            float x1 = (seg.Last + 1) / 66f * w;
            if (s % 2 == 1)
                ds.FillRectangle(x0, 0, x1 - x0, h, wash);
            using var tl = new CanvasTextLayout(sender, seg.Label, labelFmt, 1e6f, 1e6f);
            float tw = (float)tl.LayoutBounds.Width;
            if (tw <= x1 - x0 - 6)
                ds.DrawTextLayout(tl, x0 + (x1 - x0 - tw) / 2, (h - (float)tl.LayoutBounds.Height) / 2,
                    Palette.SectionGold);
        }
        // OT/NT divide.
        float dx = Canon.OtNtDivide / 66f * w;
        ds.DrawLine(dx, 0, dx, h, Color.FromArgb(140, faded.R, faded.G, faded.B), 1f);

        foreach (var (order, active) in Pins)
        {
            float px = (order + 0.5f) / 66f * w;
            ds.FillCircle(px, h - 4, 3.5f,
                active ? Palette.Gold : Color.FromArgb(153, faded.R, faded.G, faded.B));
        }
    }
}
