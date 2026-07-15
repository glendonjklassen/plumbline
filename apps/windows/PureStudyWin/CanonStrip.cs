// The canon overview strip: 66 books in 8 sections under the panes, with the
// OT/NT divide and a pin per pane — click to jump the active pane anywhere
// (GTK M:2938–2989). Section geometry comes from the manifest (frozen in
// core::reference::CANON_SEGMENTS).

using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Windows.UI;

namespace PureStudyWin;

public sealed class CanonStrip : Microsoft.UI.Xaml.Controls.UserControl
{
    public static readonly (string Name, int First, int Last)[] Segments =
    {
        ("Law", 0, 4), ("History", 5, 16), ("Wisdom", 17, 21), ("Prophets", 22, 38),
        ("Gospels", 39, 42), ("Acts", 43, 43), ("Letters", 44, 64), ("Revelation", 65, 65),
    };
    public const int OtNtDivide = 39;

    /// Book order → (pin per pane, active flag). Set by the shell.
    public List<(int bookOrder, bool active)> Pins = new();
    private List<TocBook> _books = new();
    private readonly CanvasControl _canvas = new()
    {
        ClearColor = Color.FromArgb(255, 235, 230, 219),
    };

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
        float w = (float)sender.ActualWidth, h = (float)sender.ActualHeight;
        if (w < 10) return;

        using var labelFmt = new CanvasTextFormat { FontSize = 11 };
        for (int s = 0; s < Segments.Length; s++)
        {
            var seg = Segments[s];
            float x0 = seg.First / 66f * w;
            float x1 = (seg.Last + 1) / 66f * w;
            if (s % 2 == 1)
                ds.FillRectangle(x0, 0, x1 - x0, h, Color.FromArgb(10, 0, 0, 0));
            using var tl = new CanvasTextLayout(sender, seg.Name, labelFmt, 1e6f, 1e6f);
            float tw = (float)tl.LayoutBounds.Width;
            if (tw <= x1 - x0 - 6)
                ds.DrawTextLayout(tl, x0 + (x1 - x0 - tw) / 2, (h - (float)tl.LayoutBounds.Height) / 2,
                    Color.FromArgb(230, 89, 77, 56));
        }
        // OT/NT divide.
        float dx = OtNtDivide / 66f * w;
        ds.DrawLine(dx, 0, dx, h, Color.FromArgb(128, 102, 77, 51), 1f);

        foreach (var (order, active) in Pins)
        {
            float px = (order + 0.5f) / 66f * w;
            ds.FillCircle(px, h - 4, 3.5f,
                active ? Palette.Gold : Color.FromArgb(153, 77, 77, 77));
        }
    }
}
