// The ambient weave-connector overlay: a transparent, input-transparent
// Win2D canvas above the pane row that draws soft gold Béziers between
// cross-referenced verses visible in *different* panes, with off-screen
// endpoints clamped to the pane edge as a scroll hint (GTK M:2821–2934).

using System.Numerics;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml;
using Windows.UI;

namespace PureStudyWin;

/// A deduped canonical verse pair (client-side build_links over the library).
public sealed record LinkPair(string A, string B, string BookA, uint ChA, string BookB, uint ChB);

public sealed class ConnectorLayer : Microsoft.UI.Xaml.Controls.UserControl
{
    private const float Inset = 14f;   // LINK_INSET
    private const float YInset = 5f;   // LINK_YINSET
    private static readonly Color Stroke = Color.FromArgb(89, 158, 125, 56);   // α0.35
    private static readonly Color Dot = Color.FromArgb(178, 158, 125, 56);     // α0.7

    public List<LinkPair> Links = new();
    public IReadOnlyList<PaneView> Panes = Array.Empty<PaneView>();

    private readonly CanvasControl _canvas = new() { ClearColor = Color.FromArgb(0, 0, 0, 0) };

    public ConnectorLayer()
    {
        IsHitTestVisible = false;
        _canvas.Draw += OnDraw;
        Content = _canvas;
    }

    public void Redraw() => _canvas.Invalidate();

    private void OnDraw(CanvasControl sender, CanvasDrawEventArgs args)
    {
        if (Panes.Count < 2 || Links.Count == 0) return;
        var ds = args.DrawingSession;

        // Which pane shows which (book, chapter) — later pane wins duplicates.
        var showing = new Dictionary<(string, uint), int>();
        for (int i = 0; i < Panes.Count; i++)
            showing[(Panes[i].Reader.Book, Panes[i].Reader.ChapterNumber)] = i;

        foreach (var link in Links)
        {
            if (!showing.TryGetValue((link.BookA, link.ChA), out var pa) ||
                !showing.TryGetValue((link.BookB, link.ChB), out var pb) || pa == pb)
                continue;

            var (left, right, aRef, bRef) = pa < pb
                ? (pa, pb, link.A, link.B)
                : (pb, pa, link.B, link.A);

            var p1 = Endpoint(Panes[left], aRef, atRightEdge: true);
            var p2 = Endpoint(Panes[right], bRef, atRightEdge: false);
            if (p1 is null || p2 is null) continue;

            var (x1, y1) = p1.Value;
            var (x2, y2) = p2.Value;
            float dx = x2 - x1;
            using var path = new Microsoft.Graphics.Canvas.Geometry.CanvasPathBuilder(sender);
            path.BeginFigure(x1, y1);
            path.AddCubicBezier(
                new Vector2(x1 + dx * 0.4f, y1),
                new Vector2(x2 - dx * 0.4f, y2),
                new Vector2(x2, y2));
            path.EndFigure(Microsoft.Graphics.Canvas.Geometry.CanvasFigureLoop.Open);
            using var geo = Microsoft.Graphics.Canvas.Geometry.CanvasGeometry.CreatePath(path);
            ds.DrawGeometry(geo, Stroke, 1.5f);
            ds.FillCircle(x1, y1, 2f, Dot);
            ds.FillCircle(x2, y2, 2f, Dot);
        }
    }

    /// The layer-space endpoint for a verse in a pane: x rides the gutter
    /// (right edge − 14 / left edge + 14); y is the verse-number line centre,
    /// clamped into the pane's visible band ±5 as a scroll hint.
    private (float x, float y)? Endpoint(PaneView pane, string refKey, bool atRightEdge)
    {
        var vy = pane.Reader.VerseY(refKey);
        if (vy is null) return null;

        var xf = pane.Reader.TransformToVisual(this);
        var (bandTop, bandBottom) = pane.Reader.VisibleBand();
        var topLeft = xf.TransformPoint(new Windows.Foundation.Point(0, 0));
        float w = (float)pane.Reader.ActualWidth;

        float y = (float)topLeft.Y + Math.Clamp(vy.Value, bandTop + YInset, bandBottom - YInset);
        float x = atRightEdge ? (float)topLeft.X + w - Inset : (float)topLeft.X + Inset;
        return (x, y);
    }
}
